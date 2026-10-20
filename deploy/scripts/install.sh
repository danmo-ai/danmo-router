#!/usr/bin/env bash
# First-time deploy via Docker Compose (edge Nginx + relay/admin/user).
# SQL schema is applied by the binaries on startup (schema_migrations).
#
# Usage (from repo root or a checkout that contains compose.yml):
#   ./deploy/scripts/install.sh
#   ./deploy/scripts/install.sh --role=relay
#   ./deploy/scripts/install.sh --non-interactive
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEPLOY_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
# Repo root (parent of deploy/) — where compose.yml lives.
ROOT="$(cd "${DEPLOY_DIR}/.." && pwd)"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

info() { echo -e "${GREEN}==>${NC} $*"; }
warn() { echo -e "${YELLOW}==>${NC} $*"; }
err() { echo -e "${RED}==>${NC} $*" >&2; }

ROLE=all
NON_INTERACTIVE=0
BUILD=1
PULL=0
API_DOMAIN=api.localhost
ADMIN_DOMAIN=admin.localhost
USER_DOMAIN=user.localhost
ENABLE_TLS=0
COMPOSE_FILE="${ROOT}/compose.yml"

usage() {
  cat <<EOF
Usage: $0 [options]

  --role=all|relay|admin|user   Compose profiles to start (default: all)
  --api-domain=HOST             Default: api.localhost
  --admin-domain=HOST           Default: admin.localhost
  --user-domain=HOST            Default: user.localhost
  --enable-tls                  Edge terminates TLS (certs in ./certs)
  --pull                        docker compose pull (use published GHCR images)
  --no-build                    Skip --build (with --pull, or when images exist)
  --non-interactive             No prompts
  -h, --help

Writes/updates ${ROOT}/.env then runs: docker compose --profile … up -d
Only the edge service publishes host ports 80/443; backends stay internal.
EOF
}

for arg in "$@"; do
  case "$arg" in
    --role=*) ROLE="${arg#*=}" ;;
    --api-domain=*) API_DOMAIN="${arg#*=}" ;;
    --admin-domain=*) ADMIN_DOMAIN="${arg#*=}" ;;
    --user-domain=*) USER_DOMAIN="${arg#*=}" ;;
    --enable-tls) ENABLE_TLS=1 ;;
    --pull) PULL=1 ;;
    --no-build) BUILD=0 ;;
    --non-interactive) NON_INTERACTIVE=1 ;;
    -h|--help) usage; exit 0 ;;
    *) err "Unknown option: $arg"; usage; exit 1 ;;
  esac
done

case "$ROLE" in
  all|relay|admin|user) ;;
  *) err "Invalid --role=$ROLE"; exit 1 ;;
esac

prompt() {
  local var_name="$1"
  local text="$2"
  local default="${3:-}"
  local value
  if [[ "${NON_INTERACTIVE}" -eq 1 ]]; then
    printf -v "$var_name" '%s' "$default"
    return
  fi
  if [[ -n "$default" ]]; then
    read -r -p "${text} [${default}]: " value || true
    value="${value:-$default}"
  else
    read -r -p "${text}: " value || true
  fi
  printf -v "$var_name" '%s' "$value"
}

require_docker() {
  if ! command -v docker >/dev/null 2>&1; then
    err "docker not found. Install Docker Engine + Compose plugin."
    exit 1
  fi
  if ! docker compose version >/dev/null 2>&1; then
    err "docker compose plugin not found."
    exit 1
  fi
}

random_hex64() {
  if command -v openssl >/dev/null 2>&1; then
    openssl rand -hex 32
  else
    head -c 32 /dev/urandom | od -An -tx1 | tr -d ' \n'
  fi
}

ensure_env() {
  local env_file="${ROOT}/.env"
  if [[ ! -f "$env_file" ]]; then
    if [[ -f "${ROOT}/.env.example" ]]; then
      cp "${ROOT}/.env.example" "$env_file"
      info "Created ${env_file} from .env.example"
    else
      err "Missing ${env_file} and .env.example"
      exit 1
    fi
  fi

  prompt API_DOMAIN "API domain" "$API_DOMAIN"
  prompt ADMIN_DOMAIN "Admin portal domain" "$ADMIN_DOMAIN"
  prompt USER_DOMAIN "User portal domain" "$USER_DOMAIN"

  # Upsert selected keys in .env without wiping user customizations.
  upsert_env() {
    local key="$1" val="$2" file="$3"
    if grep -qE "^${key}=" "$file" 2>/dev/null; then
      # portable-ish in-place replace
      local tmp
      tmp="$(mktemp)"
      awk -v k="$key" -v v="$val" 'BEGIN{done=0} $0 ~ "^"k"=" {print k"="v; done=1; next} {print} END{if(!done) print k"="v}' \
        "$file" >"$tmp"
      mv "$tmp" "$file"
    else
      printf '%s=%s\n' "$key" "$val" >>"$file"
    fi
  }

  upsert_env COMPOSE_PROFILES "$ROLE" "$env_file"
  upsert_env API_DOMAIN "$API_DOMAIN" "$env_file"
  upsert_env ADMIN_DOMAIN "$ADMIN_DOMAIN" "$env_file"
  upsert_env USER_DOMAIN "$USER_DOMAIN" "$env_file"
  upsert_env ENABLE_TLS "$ENABLE_TLS" "$env_file"
  upsert_env CORS_ORIGINS "http://${ADMIN_DOMAIN},http://${USER_DOMAIN},https://${ADMIN_DOMAIN},https://${USER_DOMAIN}" "$env_file"
  if [[ "$ENABLE_TLS" == "1" ]]; then
    upsert_env PUBLIC_RELAY_URL "https://${API_DOMAIN}/v1" "$env_file"
  else
    upsert_env PUBLIC_RELAY_URL "http://${API_DOMAIN}/v1" "$env_file"
  fi
  upsert_env DATABASE_URL "sqlite:///data/danmo.db?mode=rwc" "$env_file"

  local key
  key="$(grep -E '^ENCRYPTION_KEY=' "$env_file" | head -1 | cut -d= -f2- || true)"
  if [[ -z "$key" || "$key" == "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef" ]]; then
    if [[ "${NON_INTERACTIVE}" -eq 1 ]]; then
      key="$(random_hex64)"
      upsert_env ENCRYPTION_KEY "$key" "$env_file"
      warn "Generated ENCRYPTION_KEY (non-interactive). Store it safely."
    else
      prompt key "ENCRYPTION_KEY (64 hex; empty=generate)" ""
      if [[ -z "$key" ]]; then
        key="$(random_hex64)"
      fi
      upsert_env ENCRYPTION_KEY "$key" "$env_file"
    fi
  fi

  mkdir -p "${ROOT}/certs"
}

compose_up() {
  local -a profiles=()
  case "$ROLE" in
    all) profiles=(--profile all) ;;
    relay) profiles=(--profile relay) ;;
    admin) profiles=(--profile admin) ;;
    user) profiles=(--profile user) ;;
  esac

  local -a args=( -f "$COMPOSE_FILE" "${profiles[@]}" )
  if [[ "$PULL" -eq 1 ]]; then
    info "Pulling images…"
    docker compose "${args[@]}" pull
  fi

  local -a up_args=( up -d )
  if [[ "$BUILD" -eq 1 && "$PULL" -eq 0 ]]; then
    up_args+=( --build )
  fi

  info "Starting compose (role=${ROLE})…"
  docker compose "${args[@]}" "${up_args[@]}"
  info "Done. Edge publishes host ports EDGE_HTTP_PORT (default 80) and EDGE_HTTPS_PORT (default 443)."
  info "Backends are not published on the host — only edge Nginx is reachable."
  info "Health: curl -H \"Host: ${API_DOMAIN}\" http://127.0.0.1/healthz"
}

require_docker
cd "$ROOT"
[[ -f "$COMPOSE_FILE" ]] || { err "Missing ${COMPOSE_FILE}"; exit 1; }
ensure_env
compose_up
