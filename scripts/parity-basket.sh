#!/usr/bin/env bash
set -euo pipefail

# Dual-run parity harness for the Basket.API .NET -> Rust migration (plan.md, unit 6).
#
#   ./scripts/parity-basket.sh              replay the recorded .NET transcript against Rust
#   ./scripts/parity-basket.sh --record     re-record the transcript from the .NET service
#   ./scripts/parity-basket.sh --dual       record from .NET and immediately replay against Rust
#
# Both implementations run against a throwaway Redis, a throwaway RabbitMQ and a throwaway OIDC
# issuer, so the comparison covers real token validation, real storage bytes and real integration
# event handling instead of mocks.

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

CRATE_DIR="native/basket_service"
DOTNET_PROJECT="src/Basket.API/Basket.API.csproj"
GOLDEN="scripts/parity/basket-dotnet.transcript"
PARITY_USER="parity-user"
WORK_DIR="$(mktemp -d)"
CONTAINER_PREFIX="basket-parity-$$"

MODE="replay"
case "${1:-}" in
  --record) MODE="record" ;;
  --dual) MODE="dual" ;;
  --replay | "") MODE="replay" ;;
  *)
    echo "usage: $0 [--record|--replay|--dual]" >&2
    exit 2
    ;;
esac

PIDS=()

cleanup() {
  local status=$?
  for pid in "${PIDS[@]:-}"; do
    [[ -n "$pid" ]] && kill "$pid" 2>/dev/null || true
  done
  docker rm -f "$CONTAINER_PREFIX-redis" "$CONTAINER_PREFIX-rabbit" >/dev/null 2>&1 || true
  rm -rf "$WORK_DIR"
  exit "$status"
}
trap cleanup EXIT INT TERM

require() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "parity-basket: $1 is required but not on PATH" >&2
    exit 2
  }
}

free_port() {
  python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1]); s.close()'
}

wait_for_port() {
  local port="$1" label="$2" attempts="${3:-120}"
  for _ in $(seq 1 "$attempts"); do
    if python3 -c "import socket,sys; s=socket.socket(); s.settimeout(0.5); sys.exit(s.connect_ex(('127.0.0.1', $port)))"; then
      return 0
    fi
    sleep 1
  done
  echo "parity-basket: $label did not start on port $port" >&2
  return 1
}

wait_for_rabbit() {
  # The AMQP port accepts TCP well before the broker finishes booting, so wait for the broker to
  # announce it is up instead.
  for _ in $(seq 1 120); do
    if docker logs "$CONTAINER_PREFIX-rabbit" 2>&1 | grep -q "Server startup complete"; then
      return 0
    fi
    sleep 1
  done
  echo "parity-basket: rabbitmq did not become ready" >&2
  docker logs "$CONTAINER_PREFIX-rabbit" 2>&1 | tail -20 >&2
  return 1
}

b64url() { openssl base64 -A | tr '+/' '-_' | tr -d '='; }

mint_jwt() {
  local subject="$1" issuer="$2" lifetime="$3"
  local issued expires header payload signing_input signature
  issued=$(date +%s)
  expires=$((issued + lifetime))
  header=$(printf '{"alg":"RS256","typ":"JWT","kid":"parity-key"}' | b64url)
  payload=$(printf '{"iss":"%s","sub":"%s","aud":"basket","scope":"basket","iat":%s,"nbf":%s,"exp":%s}' \
    "$issuer" "$subject" "$issued" "$issued" "$expires" | b64url)
  signing_input="$header.$payload"
  signature=$(printf '%s' "$signing_input" | openssl dgst -sha256 -sign "$WORK_DIR/oidc-key.pem" -binary | b64url)
  printf '%s.%s' "$signing_input" "$signature"
}

start_oidc_issuer() {
  local port="$1" issuer="http://localhost:$1"
  local modulus n

  openssl genrsa -out "$WORK_DIR/oidc-key.pem" 2048 2>/dev/null
  modulus=$(openssl rsa -in "$WORK_DIR/oidc-key.pem" -noout -modulus | sed 's/^Modulus=//')
  n=$(python3 -c "import base64; print(base64.urlsafe_b64encode(bytes.fromhex('$modulus')).decode().rstrip('='))")

  mkdir -p "$WORK_DIR/oidc/.well-known"
  cat >"$WORK_DIR/oidc/jwks.json" <<JSON
{"keys":[{"kty":"RSA","use":"sig","alg":"RS256","kid":"parity-key","n":"$n","e":"AQAB"}]}
JSON
  cat >"$WORK_DIR/oidc/.well-known/openid-configuration" <<JSON
{
  "issuer": "$issuer",
  "jwks_uri": "$issuer/jwks.json",
  "authorization_endpoint": "$issuer/connect/authorize",
  "token_endpoint": "$issuer/connect/token",
  "response_types_supported": ["code"],
  "subject_types_supported": ["public"],
  "id_token_signing_alg_values_supported": ["RS256"]
}
JSON

  (cd "$WORK_DIR/oidc" && python3 -m http.server "$port" --bind 127.0.0.1 >"$WORK_DIR/oidc.log" 2>&1) &
  PIDS+=($!)
  wait_for_port "$port" "throwaway OIDC issuer" 30
}

run_transcript() {
  local endpoint="$1" output="$2"
  "$CRATE_DIR/target/release/parity-client" \
    --endpoint "$endpoint" \
    --redis "redis://127.0.0.1:$REDIS_PORT" \
    --user "$PARITY_USER" \
    --token "$TOKEN" \
    --expired-token "$EXPIRED_TOKEN" \
    --foreign-token "$FOREIGN_TOKEN" \
    --amqp "amqp://guest:guest@127.0.0.1:$RABBIT_PORT" \
    --output "$output"
}

require docker
require openssl
require python3
require cargo
[[ "$MODE" == "replay" ]] || require dotnet

echo "parity-basket: mode=$MODE"
echo "parity-basket: building the Rust harness driver"
(cd "$CRATE_DIR" && cargo build --release --quiet)

REDIS_PORT=$(free_port)
RABBIT_PORT=$(free_port)
OIDC_PORT=$(free_port)
DOTNET_PORT=$(free_port)
RUST_PORT=$(free_port)

docker run -d --rm --name "$CONTAINER_PREFIX-redis" -p "127.0.0.1:$REDIS_PORT:6379" redis:8.6 >/dev/null
docker run -d --rm --name "$CONTAINER_PREFIX-rabbit" -p "127.0.0.1:$RABBIT_PORT:5672" rabbitmq:4.2 >/dev/null
wait_for_port "$REDIS_PORT" "redis"
wait_for_rabbit

start_oidc_issuer "$OIDC_PORT"
ISSUER="http://localhost:$OIDC_PORT"
TOKEN=$(mint_jwt "$PARITY_USER" "$ISSUER" 3600)
EXPIRED_TOKEN=$(mint_jwt "$PARITY_USER" "$ISSUER" -3600)
FOREIGN_TOKEN=$(mint_jwt "$PARITY_USER" "http://localhost:1/other-issuer" 3600)

AMQP_URL="amqp://guest:guest@localhost:$RABBIT_PORT"
REDIS_CONNECTION="localhost:$REDIS_PORT"

if [[ "$MODE" == "record" || "$MODE" == "dual" ]]; then
  [[ -f "$DOTNET_PROJECT" ]] || {
    echo "parity-basket: $DOTNET_PROJECT no longer exists; the transcript in $GOLDEN is the recorded baseline" >&2
    exit 2
  }

  echo "parity-basket: starting the .NET basket service on port $DOTNET_PORT"
  ASPNETCORE_URLS="http://localhost:$DOTNET_PORT" \
  ASPNETCORE_ENVIRONMENT=Development \
  ConnectionStrings__redis="$REDIS_CONNECTION" \
  ConnectionStrings__eventbus="$AMQP_URL" \
  EventBus__SubscriptionClientName="Basket-parity-dotnet" \
  Identity__Url="$ISSUER" \
    dotnet run --project "$DOTNET_PROJECT" --no-launch-profile >"$WORK_DIR/dotnet.log" 2>&1 &
  PIDS+=($!)
  wait_for_port "$DOTNET_PORT" ".NET basket service" 180

  mkdir -p "$(dirname "$GOLDEN")"
  echo "parity-basket: recording the .NET transcript"
  run_transcript "http://127.0.0.1:$DOTNET_PORT" "$GOLDEN"
fi

if [[ "$MODE" == "replay" || "$MODE" == "dual" ]]; then
  [[ -f "$GOLDEN" ]] || {
    echo "parity-basket: no recorded transcript at $GOLDEN; run $0 --record first" >&2
    exit 2
  }

  echo "parity-basket: starting the Rust basket service on port $RUST_PORT"
  BASKET_LISTEN_ADDR="127.0.0.1:$RUST_PORT" \
  ConnectionStrings__redis="$REDIS_CONNECTION" \
  ConnectionStrings__eventbus="$AMQP_URL" \
  EventBus__SubscriptionClientName="Basket-parity-rust" \
  Identity__Url="$ISSUER" \
  RUST_LOG="${RUST_LOG:-info}" \
    "$CRATE_DIR/target/release/basket-service" >"$WORK_DIR/rust.log" 2>&1 &
  PIDS+=($!)
  wait_for_port "$RUST_PORT" "Rust basket service" 60

  echo "parity-basket: replaying the transcript against Rust"
  run_transcript "http://127.0.0.1:$RUST_PORT" "$WORK_DIR/rust.transcript"

  if diff -u "$GOLDEN" "$WORK_DIR/rust.transcript"; then
    echo "parity-basket: PASS — Rust matches the recorded .NET transcript ($(wc -l <"$GOLDEN") observations)"
  else
    echo "parity-basket: FAIL — Rust diverges from the recorded .NET transcript" >&2
    echo "parity-basket: rust service log:" >&2
    tail -40 "$WORK_DIR/rust.log" >&2
    exit 1
  fi
fi
