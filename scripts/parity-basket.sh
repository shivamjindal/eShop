#!/usr/bin/env bash
set -euo pipefail

# Behavioral parity harness for the Basket.API .NET -> Rust migration.
#
#   ./scripts/parity-basket.sh                 record from .NET (when it still exists), then replay against Rust
#   ./scripts/parity-basket.sh --record-only   only refresh the committed transcript
#   ./scripts/parity-basket.sh --replay-only   only replay the committed transcript against Rust
#
# Both services run against throwaway Redis/RabbitMQ containers and a stub identity
# provider, so the recorded transcript is real service behavior rather than mocks.

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

TRANSCRIPT="${TRANSCRIPT:-scripts/parity/basket-transcript.jsonl}"
DOTNET_PROJECT="src/Basket.API/Basket.API.csproj"
NATIVE_MANIFEST="native/Cargo.toml"
KID="parity-key"
REDIS_IMAGE="${REDIS_IMAGE:-redis:8.6}"
RABBIT_IMAGE="${RABBIT_IMAGE:-rabbitmq:4.2}"

MODE="both"
case "${1:-}" in
  --record-only) MODE="record" ;;
  --replay-only) MODE="replay" ;;
  "") ;;
  *) echo "unknown option: $1" >&2; exit 2 ;;
esac

for tool in docker cargo openssl python3; do
  command -v "$tool" >/dev/null 2>&1 || { echo "parity-basket: $tool is required" >&2; exit 2; }
done

WORKDIR="$(mktemp -d)"
REDIS_CONTAINER="basket-parity-redis-$$"
RABBIT_CONTAINER="basket-parity-rabbit-$$"
PIDS=()

cleanup() {
  local status=$?
  for pid in "${PIDS[@]:-}"; do
    [[ -n "$pid" ]] && kill "$pid" 2>/dev/null || true
  done
  docker rm -f "$REDIS_CONTAINER" "$RABBIT_CONTAINER" >/dev/null 2>&1 || true
  rm -rf "$WORKDIR"
  exit "$status"
}
trap cleanup EXIT INT TERM

free_port() {
  python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1]); s.close()'
}

wait_for_tcp() {
  local port="$1" label="$2" attempts="${3:-120}"
  for _ in $(seq 1 "$attempts"); do
    if python3 -c "import socket,sys; s=socket.socket(); s.settimeout(1); sys.exit(s.connect_ex(('127.0.0.1',$port)))"; then
      return 0
    fi
    sleep 1
  done
  echo "parity-basket: timed out waiting for $label on port $port" >&2
  return 1
}

echo "parity-basket: minting an ephemeral signing key (never committed)"
openssl genrsa -out "$WORKDIR/idp.pem" 2048 2>/dev/null
MODULUS="$(openssl rsa -in "$WORKDIR/idp.pem" -noout -modulus | cut -d= -f2)"

IDP_PORT="$(free_port)"
ISSUER="http://localhost:$IDP_PORT"
python3 scripts/parity/idp_stub.py "$IDP_PORT" "$MODULUS" "$KID" &
PIDS+=("$!")
wait_for_tcp "$IDP_PORT" "identity stub" 30
echo "parity-basket: identity stub on $ISSUER"

REDIS_PORT="$(free_port)"
RABBIT_PORT="$(free_port)"
docker run -d --name "$REDIS_CONTAINER" -p "$REDIS_PORT:6379" "$REDIS_IMAGE" >/dev/null
wait_for_tcp "$REDIS_PORT" redis 60

# Everything that enters the broker container must run as `rabbitmq`: a root `docker
# exec` writes a root-owned /var/lib/rabbitmq/.erlang.cookie, and the server then dies
# with eacces on its next read.
docker run -d --name "$RABBIT_CONTAINER" -p "$RABBIT_PORT:5672" "$RABBIT_IMAGE" >/dev/null
rabbit_started=0
for _ in $(seq 1 90); do
  if docker logs "$RABBIT_CONTAINER" 2>&1 | grep -q "Server startup complete"; then
    rabbit_started=1
    break
  fi
  if [[ "$(docker inspect -f '{{.State.Status}}' "$RABBIT_CONTAINER" 2>/dev/null)" != "running" ]]; then
    break
  fi
  sleep 2
done
if [[ "$rabbit_started" -ne 1 ]]; then
  echo "parity-basket: rabbitmq never finished starting" >&2
  docker logs "$RABBIT_CONTAINER" 2>&1 | tail -30 >&2
  exit 1
fi
wait_for_tcp "$RABBIT_PORT" rabbitmq 60
echo "parity-basket: redis on $REDIS_PORT, rabbitmq on $RABBIT_PORT"

REDIS_CS="127.0.0.1:$REDIS_PORT"
AMQP_URI="amqp://guest:guest@127.0.0.1:$RABBIT_PORT"

echo "parity-basket: building the Rust binaries"
cargo build --release --manifest-path "$NATIVE_MANIFEST" --bin basket-service --bin basket-parity

parity_tool() {
  local endpoint="$1"; shift
  native/target/release/basket-parity "$@" \
    --endpoint "$endpoint" \
    --redis "redis://127.0.0.1:$REDIS_PORT" \
    --amqp "$AMQP_URI" \
    --signing-key "$WORKDIR/idp.pem" \
    --kid "$KID" \
    --issuer "$ISSUER"
}

reset_state() {
  docker exec "$REDIS_CONTAINER" redis-cli flushall >/dev/null
  docker exec -u rabbitmq "$RABBIT_CONTAINER" rabbitmqctl -q purge_queue Basket >/dev/null 2>&1 || true
}

if [[ "$MODE" != "replay" ]]; then
  if [[ ! -f "$DOTNET_PROJECT" ]]; then
    if [[ "$MODE" == "record" ]]; then
      echo "parity-basket: --record-only needs $DOTNET_PROJECT, which no longer exists" >&2
      exit 1
    fi
    echo "parity-basket: $DOTNET_PROJECT is gone (migration complete); replaying the committed transcript"
    MODE="replay"
  fi
fi

if [[ "$MODE" != "replay" ]]; then
  command -v dotnet >/dev/null 2>&1 || { echo "parity-basket: dotnet is required to record" >&2; exit 2; }
  reset_state
  DOTNET_PORT="$(free_port)"
  echo "parity-basket: starting the .NET Basket.API on $DOTNET_PORT"
  ASPNETCORE_URLS="http://127.0.0.1:$DOTNET_PORT" \
  ConnectionStrings__redis="$REDIS_CS" \
  ConnectionStrings__eventbus="$AMQP_URI" \
  Identity__Url="$ISSUER" \
    dotnet run --project "$DOTNET_PROJECT" --no-launch-profile >"$WORKDIR/dotnet.log" 2>&1 &
  DOTNET_PID="$!"
  PIDS+=("$DOTNET_PID")
  if ! wait_for_tcp "$DOTNET_PORT" "Basket.API (.NET)" 180; then
    tail -40 "$WORKDIR/dotnet.log" >&2
    exit 1
  fi
  sleep 5 # let the event bus subscription bind before the event cases run

  parity_tool "http://127.0.0.1:$DOTNET_PORT" record --out "$TRANSCRIPT"

  kill "$DOTNET_PID" 2>/dev/null || true
  wait "$DOTNET_PID" 2>/dev/null || true
  echo "parity-basket: transcript recorded from the .NET service"
fi

if [[ "$MODE" == "record" ]]; then
  echo "parity-basket: path=record exit_code=0"
  exit 0
fi

[[ -f "$TRANSCRIPT" ]] || { echo "parity-basket: no transcript at $TRANSCRIPT" >&2; exit 1; }

reset_state
RUST_PORT="$(free_port)"
echo "parity-basket: starting the Rust basket-service on $RUST_PORT"
PORT="$RUST_PORT" \
ConnectionStrings__redis="$REDIS_CS" \
ConnectionStrings__eventbus="$AMQP_URI" \
Identity__Url="$ISSUER" \
  native/target/release/basket-service >"$WORKDIR/rust.log" 2>&1 &
RUST_PID="$!"
PIDS+=("$RUST_PID")
if ! wait_for_tcp "$RUST_PORT" "basket-service (rust)" 60; then
  tail -40 "$WORKDIR/rust.log" >&2
  exit 1
fi
sleep 3 # same grace period for the event bus subscription

set +e
parity_tool "http://127.0.0.1:$RUST_PORT" replay --in "$TRANSCRIPT"
ec=$?
set -e

if [[ "$ec" -ne 0 ]]; then
  echo "parity-basket: rust service log:" >&2
  tail -40 "$WORKDIR/rust.log" >&2
fi

echo "parity-basket: path=replay exit_code=$ec"
exit "$ec"
