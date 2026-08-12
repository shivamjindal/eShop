#!/usr/bin/env bash
set -euo pipefail

# Behavioral parity lever for the Basket migration (.NET → Rust).
#
#   ./scripts/parity-basket.sh                  replay the recorded transcript against Rust
#   ./scripts/parity-basket.sh --record-dotnet  re-record the baseline from src/Basket.API
#   ./scripts/parity-basket.sh --record-rust    re-record from the Rust service (bootstrap only)
#
# Brings up throwaway Redis and RabbitMQ containers plus a local OIDC stub, starts the service under
# test against them, and drives scripts/parity/basket-*.transcript through `basket-parity`.
# Requires Docker, openssl, python3 and cargo.

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

TRANSCRIPT="scripts/parity/basket-dotnet.transcript"
NATIVE_MANIFEST="native/Cargo.toml"
MODE="replay-rust"

case "${1:-}" in
  "") ;;
  --record-dotnet) MODE="record-dotnet" ;;
  --record-rust) MODE="record-rust" ;;
  *) echo "usage: $0 [--record-dotnet|--record-rust]" >&2; exit 2 ;;
esac

for tool in docker openssl python3 cargo; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "parity-basket: path=P:missing-tool tool=$tool exit_code=2" >&2
    exit 2
  fi
done

if [[ "$MODE" == "record-dotnet" && ! -f "src/Basket.API/Basket.API.csproj" ]]; then
  echo "parity-basket: path=P:dotnet-project-gone exit_code=2" >&2
  echo "src/Basket.API no longer exists; the committed transcript is the baseline." >&2
  exit 2
fi

WORKDIR="$(mktemp -d)"
RUN_ID="basket-parity-$$"
REDIS_CONTAINER="$RUN_ID-redis"
RABBIT_CONTAINER="$RUN_ID-rabbit"
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
trap cleanup EXIT

free_port() {
  python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1]); s.close()'
}

wait_for_port() {
  local port="$1" label="$2" attempts="${3:-120}"
  for _ in $(seq 1 "$attempts"); do
    if python3 -c "import socket,sys; s=socket.socket(); s.settimeout(0.5); sys.exit(0 if s.connect_ex(('127.0.0.1',$port))==0 else 1)"; then
      return 0
    fi
    sleep 1
  done
  echo "parity-basket: path=P:timeout target=$label exit_code=1" >&2
  return 1
}

REDIS_PORT="$(free_port)"
RABBIT_PORT="$(free_port)"
IDP_PORT="$(free_port)"
SERVICE_PORT="$(free_port)"

echo "parity-basket: mode=$MODE redis=$REDIS_PORT rabbit=$RABBIT_PORT idp=$IDP_PORT service=$SERVICE_PORT"

docker run -d --rm --name "$REDIS_CONTAINER" -p "127.0.0.1:$REDIS_PORT:6379" redis:7 >/dev/null
docker run -d --rm --name "$RABBIT_CONTAINER" -p "127.0.0.1:$RABBIT_PORT:5672" rabbitmq:4 >/dev/null

openssl genrsa -out "$WORKDIR/idp.pem" 2048 2>/dev/null
python3 scripts/parity/idp_stub.py --port "$IDP_PORT" --key "$WORKDIR/idp.pem" --kid parity-key \
  >"$WORKDIR/idp.log" 2>&1 &
PIDS+=("$!")

wait_for_port "$REDIS_PORT" redis
wait_for_port "$IDP_PORT" idp-stub
# The AMQP port accepts TCP well before the broker finishes booting.
echo "parity-basket: waiting for RabbitMQ to finish starting"
for _ in $(seq 1 120); do
  if docker logs "$RABBIT_CONTAINER" 2>&1 | grep -q "Server startup complete"; then
    break
  fi
  sleep 1
done
docker logs "$RABBIT_CONTAINER" 2>&1 | grep -q "Server startup complete" || {
  echo "parity-basket: path=P:rabbit-not-ready exit_code=1" >&2
  exit 1
}

REDIS_URL="redis://127.0.0.1:$REDIS_PORT"
AMQP_URL="amqp://guest:guest@127.0.0.1:$RABBIT_PORT"
IDENTITY_URL="http://127.0.0.1:$IDP_PORT"

echo "parity-basket: building basket-parity"
cargo build --release --manifest-path "$NATIVE_MANIFEST" --bin basket-parity

if [[ "$MODE" == "record-dotnet" ]]; then
  echo "parity-basket: starting .NET src/Basket.API"
  ASPNETCORE_URLS="http://127.0.0.1:$SERVICE_PORT" \
  ConnectionStrings__redis="127.0.0.1:$REDIS_PORT" \
  ConnectionStrings__eventbus="$AMQP_URL" \
  Identity__Url="$IDENTITY_URL" \
  EventBus__SubscriptionClientName="Basket" \
    dotnet run --project src/Basket.API --no-launch-profile >"$WORKDIR/service.log" 2>&1 &
  PIDS+=("$!")
else
  echo "parity-basket: starting the Rust basket-service"
  cargo build --release --manifest-path "$NATIVE_MANIFEST" --bin basket-service
  PORT="$SERVICE_PORT" \
  ConnectionStrings__redis="127.0.0.1:$REDIS_PORT" \
  ConnectionStrings__eventbus="$AMQP_URL" \
  Identity__Url="$IDENTITY_URL" \
  EventBus__SubscriptionClientName="Basket" \
    ./native/target/release/basket-service >"$WORKDIR/service.log" 2>&1 &
  PIDS+=("$!")
fi

if ! wait_for_port "$SERVICE_PORT" basket-service 180; then
  cat "$WORKDIR/service.log" >&2
  exit 1
fi
# Give the integration event consumer time to declare its queue and bind.
sleep 3

PARITY_ARGS=(
  --endpoint "http://127.0.0.1:$SERVICE_PORT"
  --redis "$REDIS_URL"
  --amqp "$AMQP_URL"
  --signing-key "$WORKDIR/idp.pem"
  --key-id parity-key
  --issuer "$IDENTITY_URL"
)

set +e
case "$MODE" in
  record-dotnet)
    ./native/target/release/basket-parity record "${PARITY_ARGS[@]}" --transcript "$TRANSCRIPT"
    ec=$?
    label="P:record-dotnet"
    ;;
  record-rust)
    ./native/target/release/basket-parity record "${PARITY_ARGS[@]}" \
      --transcript scripts/parity/basket-rust.transcript
    ec=$?
    label="P:record-rust"
    ;;
  *)
    ./native/target/release/basket-parity replay "${PARITY_ARGS[@]}" --transcript "$TRANSCRIPT"
    ec=$?
    label="P:replay-rust"
    ;;
esac
set -e

if [[ $ec -ne 0 ]]; then
  echo "--- service log (tail) ---" >&2
  tail -40 "$WORKDIR/service.log" >&2 || true
fi

echo "parity-basket: path=$label exit_code=$ec"
exit "$ec"
