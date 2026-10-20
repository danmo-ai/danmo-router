#!/usr/bin/env bash
# Upgrade in place: stop → replace binaries/Portal → start.
# Incremental SQL is applied by the process via schema_migrations (do not re-run full DDL here).
# Usage:
#   sudo ./upgrade.sh --from-dir=/path/to/extracted-release --prefix=/opt/danmo --role=all
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

info() { echo -e "${GREEN}==>${NC} $*"; }
warn() { echo -e "${YELLOW}==>${NC} $*"; }
err() { echo -e "${RED}==>${NC} $*" >&2; }

ROLE=all
PREFIX=/opt/danmo
FROM_DIR=""
SKIP_NGINX=1
BACKUP_DIR=""

usage() {
  cat <<EOF
Usage: sudo $0 --from-dir=DIR [options]

  --from-dir=DIR                Extracted release tree (contains binaries and/or apps/)
  --prefix=DIR                  Install root (default: /opt/danmo)
  --role=all|relay|admin|user   Components on this host (default: all)
  --with-nginx                  Also re-run install.sh --nginx-only (rare)
  --backup-dir=DIR              Backup previous bin/apps (default: PREFIX/.upgrade-backup-TIMESTAMP)
  -h, --help
EOF
}

for arg in "$@"; do
  case "$arg" in
    --from-dir=*) FROM_DIR="${arg#*=}" ;;
    --prefix=*) PREFIX="${arg#*=}" ;;
    --role=*) ROLE="${arg#*=}" ;;
    --with-nginx) SKIP_NGINX=0 ;;
    --backup-dir=*) BACKUP_DIR="${arg#*=}" ;;
    -h|--help) usage; exit 0 ;;
    *) err "Unknown option: $arg"; usage; exit 1 ;;
  esac
done

if [[ "$(id -u)" -ne 0 ]]; then
  err "Run as root (sudo $0)."
  exit 1
fi

if [[ -z "${FROM_DIR}" || ! -d "${FROM_DIR}" ]]; then
  err "--from-dir must point to an extracted release directory"
  exit 1
fi

case "$ROLE" in
  all|relay|admin|user) ;;
  *) err "Invalid --role"; exit 1 ;;
esac

INSTALL_RELAY=0
INSTALL_ADMIN=0
INSTALL_USER=0
case "$ROLE" in
  all) INSTALL_RELAY=1; INSTALL_ADMIN=1; INSTALL_USER=1 ;;
  relay) INSTALL_RELAY=1 ;;
  admin) INSTALL_ADMIN=1 ;;
  user) INSTALL_USER=1 ;;
esac

FROM_DIR="$(cd "${FROM_DIR}" && pwd)"
BIN_SRC="${FROM_DIR}"
[[ -d "${FROM_DIR}/bin" ]] && BIN_SRC="${FROM_DIR}/bin"

BACKUP_DIR="${BACKUP_DIR:-${PREFIX}/.upgrade-backup-$(date +%Y%m%d%H%M%S)}"
mkdir -p "${BACKUP_DIR}/bin" "${PREFIX}/bin"

stop_one() {
  local u="$1"
  if systemctl list-unit-files "${u}.service" &>/dev/null && systemctl is-enabled --quiet "${u}" 2>/dev/null; then
    systemctl stop "${u}" || true
    info "Stopped ${u}"
  elif systemctl is-active --quiet "${u}" 2>/dev/null; then
    systemctl stop "${u}" || true
    info "Stopped ${u}"
  fi
}

start_one() {
  local u="$1"
  if [[ -f "/etc/systemd/system/${u}.service" ]] || systemctl list-unit-files "${u}.service" &>/dev/null; then
    systemctl start "${u}"
    info "Started ${u}"
  else
    warn "No systemd unit for ${u}; start manually: ${PREFIX}/bin/${u}"
  fi
}

copy_bin() {
  local name="$1"
  local src=""
  if [[ -f "${BIN_SRC}/${name}" ]]; then
    src="${BIN_SRC}/${name}"
  elif [[ -f "${FROM_DIR}/packages/danmo-relay/bin/${name}" && "${name}" == "danmo-relay" ]]; then
    src="${FROM_DIR}/packages/danmo-relay/bin/${name}"
  elif [[ -f "${FROM_DIR}/packages/danmo-admin-portal/bin/${name}" && "${name}" == "danmo-admin" ]]; then
    src="${FROM_DIR}/packages/danmo-admin-portal/bin/${name}"
  elif [[ -f "${FROM_DIR}/packages/danmo-user-portal/bin/${name}" && "${name}" == "danmo-user" ]]; then
    src="${FROM_DIR}/packages/danmo-user-portal/bin/${name}"
  fi
  if [[ -z "${src}" ]]; then
    err "Missing ${name} in ${FROM_DIR}"
    exit 1
  fi
  if [[ -f "${PREFIX}/bin/${name}" ]]; then
    cp -f "${PREFIX}/bin/${name}" "${BACKUP_DIR}/bin/"
  fi
  cp -f "${src}" "${PREFIX}/bin/${name}"
  chmod +x "${PREFIX}/bin/${name}"
  info "Updated ${PREFIX}/bin/${name}"
}

copy_apps() {
  local which="$1"
  local dest="${PREFIX}/apps/${which}"
  local src=""
  if [[ -d "${FROM_DIR}/apps/${which}" ]]; then
    src="${FROM_DIR}/apps/${which}"
  elif [[ "${which}" == "admin" && -d "${FROM_DIR}/packages/danmo-admin-portal/apps/admin" ]]; then
    src="${FROM_DIR}/packages/danmo-admin-portal/apps/admin"
  elif [[ "${which}" == "user" && -d "${FROM_DIR}/packages/danmo-user-portal/apps/user" ]]; then
    src="${FROM_DIR}/packages/danmo-user-portal/apps/user"
  fi
  if [[ -z "${src}" ]]; then
    warn "No apps/${which} in release; skip Portal update"
    return
  fi
  if [[ -d "${dest}" ]]; then
    mkdir -p "${BACKUP_DIR}/apps"
    rm -rf "${BACKUP_DIR}/apps/${which}"
    cp -a "${dest}" "${BACKUP_DIR}/apps/${which}"
  fi
  mkdir -p "${dest}"
  cp -R "${src}/." "${dest}/"
  info "Updated ${dest}"
}

info "Upgrade role=${ROLE} prefix=${PREFIX} from=${FROM_DIR}"
info "Backup → ${BACKUP_DIR}"

[[ "${INSTALL_RELAY}" -eq 1 ]] && stop_one danmo-relay
[[ "${INSTALL_ADMIN}" -eq 1 ]] && stop_one danmo-admin
[[ "${INSTALL_USER}" -eq 1 ]] && stop_one danmo-user

[[ "${INSTALL_RELAY}" -eq 1 ]] && copy_bin danmo-relay
[[ "${INSTALL_ADMIN}" -eq 1 ]] && copy_bin danmo-admin && copy_apps admin
[[ "${INSTALL_USER}" -eq 1 ]] && copy_bin danmo-user && copy_apps user

[[ "${INSTALL_RELAY}" -eq 1 ]] && start_one danmo-relay
[[ "${INSTALL_ADMIN}" -eq 1 ]] && start_one danmo-admin
[[ "${INSTALL_USER}" -eq 1 ]] && start_one danmo-user

if [[ "${SKIP_NGINX}" -eq 0 ]]; then
  "${SCRIPT_DIR}/install.sh" --nginx-only --prefix="${PREFIX}" --role="${ROLE}" --non-interactive
fi

info "Done. Check logs for 'applied migration N' (sqlite now; mysql/ when enabled)."
info "journalctl -u danmo-relay -u danmo-admin -u danmo-user -n 50 --no-pager"
