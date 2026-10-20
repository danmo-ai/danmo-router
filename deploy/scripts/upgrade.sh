#!/usr/bin/env bash
# Upgrade a Compose deployment: pull (or rebuild) images, recreate containers.
# Incremental SQL is applied by processes via schema_migrations.
#
# Usage (from repo root):
#   ./deploy/scripts/upgrade.sh
#   ./deploy/scripts/upgrade.sh --build
#   ./deploy/scripts/upgrade.sh --role=relay
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEPLOY_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
ROOT="$(cd "${DEPLOY_DIR}/.." && pwd)"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

info() { echo -e "${GREEN}==>${NC} $*"; }
warn() { echo -e "${YELLOW}==>${NC} $*"; }
err() { echo -e "${RED}==>${NC} $*" >&2; }

ROLE=""
BUILD=0
BACKUP=1
COMPOSE_FILE="${ROOT}/compose.yml"

usage() {
  cat <<EOF
Usage: $0 [options]

  --role=all|relay|admin|user   Override COMPOSE_PROFILES from .env
  --build                       Rebuild images from this checkout instead of pull
  --no-backup                   Skip SQLite file copy hint / volume snapshot note
  -h, --help

Default: docker compose pull && up -d (images from GHCR / configured tags).
Back up the danmo-data volume (SQLite) before major upgrades.
EOF
}

for arg in "$@"; do
  case "$arg" in
    --role=*) ROLE="${arg#*=}" ;;
    --build) BUILD=1 ;;
    --no-backup) BACKUP=0 ;;
    -h|--help) usage; exit 0 ;;
    *) err "Unknown option: $arg"; usage; exit 1 ;;
  esac
done

if [[ -n "$ROLE" ]]; then
  case "$ROLE" in
    all|relay|admin|user) ;;
    *) err "Invalid --role=$ROLE"; exit 1 ;;
  esac
fi

if ! command -v docker >/dev/null 2>&1 || ! docker compose version >/dev/null 2>&1; then
  err "docker compose is required"
  exit 1
fi

cd "$ROOT"
[[ -f "$COMPOSE_FILE" ]] || { err "Missing ${COMPOSE_FILE}"; exit 1; }
[[ -f "${ROOT}/.env" ]] || warn "No .env yet — copy .env.example or run install.sh first"

if [[ "$BACKUP" -eq 1 ]]; then
  info "Before upgrading production, back up the Compose volume (SQLite):"
  info "  docker compose -f ${COMPOSE_FILE} run --rm --no-deps -v danmo_danmo-data:/data:ro alpine \\"
  info "    tar -c -f - -C /data . > danmo-data-backup-\$(date +%F).tar"
  info "Keep ENCRYPTION_KEY from .env with the backup."
fi

PROFILE="${ROLE}"
if [[ -z "$PROFILE" && -f "${ROOT}/.env" ]]; then
  PROFILE="$(grep -E '^COMPOSE_PROFILES=' "${ROOT}/.env" | head -1 | cut -d= -f2- || true)"
fi
PROFILE="${PROFILE:-all}"

local_profiles=()
case "$PROFILE" in
  *all*|all) local_profiles=(--profile all) ;;
  *)
    IFS=',' read -r -a parts <<<"$PROFILE"
    for p in "${parts[@]}"; do
      p="$(echo "$p" | tr -d '[:space:]')"
      [[ -n "$p" ]] && local_profiles+=(--profile "$p")
    done
    ;;
esac

args=( -f "$COMPOSE_FILE" "${local_profiles[@]}" )

if [[ "$BUILD" -eq 1 ]]; then
  info "Rebuilding and recreating containers…"
  docker compose "${args[@]}" up -d --build --remove-orphans
else
  info "Pulling images…"
  docker compose "${args[@]}" pull
  info "Recreating containers…"
  docker compose "${args[@]}" up -d --remove-orphans
fi

info "Upgrade complete. Schema migrations run on process start."
info "Check: docker compose ${local_profiles[*]} ps"
