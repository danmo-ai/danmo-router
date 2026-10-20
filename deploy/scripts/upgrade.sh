#!/usr/bin/env bash
# Upgrade in place: stop → replace binaries/Portal → start.
# Incremental SQL is applied by the process via schema_migrations (do not re-run full DDL here).
#
# Install directory: not hard-coded. Resolution order:
#   1. --prefix= / --role= / --config= on the CLI
#   2. --config=FILE or /etc/danmo/install.conf (written by install.sh)
#   3. /etc/danmo/installs.d/<role>.conf when --role= is set
#   4. PREFIX/danmo-install.conf when --prefix= is set
#   5. fallback PREFIX=/opt/danmo ROLE=all
#
# Usage (from inside an extracted release):
#   sudo ./deploy/scripts/upgrade.sh
#   sudo ./deploy/scripts/upgrade.sh --prefix=/opt/danmo-user --role=user
#   sudo ./deploy/scripts/upgrade.sh --from-dir=/other/extracted-tree
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEPLOY_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
# Release tarball root is parent of deploy/ (same layout as install.sh).
PACKAGE_ROOT="$(cd "${DEPLOY_DIR}/.." && pwd)"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

info() { echo -e "${GREEN}==>${NC} $*"; }
warn() { echo -e "${YELLOW}==>${NC} $*"; }
err() { echo -e "${RED}==>${NC} $*" >&2; }

ROLE=""
PREFIX=""
FROM_DIR=""
CONFIG_FILE=""
SKIP_NGINX=1
BACKUP_DIR=""
INSTALL_STATE_DIR=/etc/danmo
CLI_PREFIX=""
CLI_ROLE=""
API_DOMAIN=""
ADMIN_DOMAIN=""
USER_DOMAIN=""

usage() {
  cat <<EOF
Usage: sudo $0 [options]

  --from-dir=DIR                New release tree (default: this script's package root:
                                  ${PACKAGE_ROOT})
  --prefix=DIR                  Install root (default: from install.conf, else /opt/danmo)
  --role=all|relay|admin|user   Components (default: from install.conf, else all)
  --config=FILE                 Install state file (default: ${INSTALL_STATE_DIR}/install.conf)
  --with-nginx                  Also re-run install.sh --nginx-only (rare)
  --backup-dir=DIR              Backup previous bin/apps (default: PREFIX/.upgrade-backup-TIMESTAMP)
  -h, --help

Typical: wget + tar, then cd into the tree and run:
  sudo ./deploy/scripts/upgrade.sh

install.sh writes:
  \${PREFIX}/danmo-install.conf
  ${INSTALL_STATE_DIR}/install.conf
  ${INSTALL_STATE_DIR}/installs.d/<role>.conf
EOF
}

for arg in "$@"; do
  case "$arg" in
    --from-dir=*) FROM_DIR="${arg#*=}" ;;
    --prefix=*) CLI_PREFIX="${arg#*=}" ;;
    --role=*) CLI_ROLE="${arg#*=}" ;;
    --config=*) CONFIG_FILE="${arg#*=}" ;;
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

# Default: the extracted release that contains this script (…/deploy/scripts/upgrade.sh).
if [[ -z "${FROM_DIR}" ]]; then
  FROM_DIR="${PACKAGE_ROOT}"
  info "Using --from-dir=${FROM_DIR} (package root of this script)"
fi

if [[ ! -d "${FROM_DIR}" ]]; then
  err "--from-dir is not a directory: ${FROM_DIR}"
  exit 1
fi

# Sanity: look like a release tree (binaries at root or under bin/ or packages/).
if [[ ! -f "${FROM_DIR}/danmo-relay" && ! -f "${FROM_DIR}/bin/danmo-relay" \
   && ! -f "${FROM_DIR}/packages/danmo-relay/bin/danmo-relay" \
   && ! -f "${FROM_DIR}/danmo-admin" && ! -f "${FROM_DIR}/bin/danmo-admin" \
   && ! -f "${FROM_DIR}/danmo-user" && ! -f "${FROM_DIR}/bin/danmo-user" ]]; then
  err "No danmo-* binaries under ${FROM_DIR}; pass --from-dir= to the extracted release root."
  exit 1
fi

load_conf() {
  local f="$1"
  [[ -f "$f" ]] || return 1
  # shellcheck disable=SC1090
  set -a
  # shellcheck disable=SC1090
  source "$f"
  set +a
  info "Loaded install state from $f"
  return 0
}

set_components_from_role() {
  INSTALL_RELAY=0
  INSTALL_ADMIN=0
  INSTALL_USER=0
  case "$ROLE" in
    all) INSTALL_RELAY=1; INSTALL_ADMIN=1; INSTALL_USER=1 ;;
    relay) INSTALL_RELAY=1 ;;
    admin) INSTALL_ADMIN=1 ;;
    user) INSTALL_USER=1 ;;
    *) err "Invalid role=${ROLE}"; exit 1 ;;
  esac
}

# Defaults, then install.conf, then CLI overrides.
PREFIX=/opt/danmo
ROLE=all
INSTALL_RELAY=1
INSTALL_ADMIN=1
INSTALL_USER=1

CONF_LOADED=0
if [[ -n "${CONFIG_FILE}" ]]; then
  load_conf "${CONFIG_FILE}" || { err "Config not found: ${CONFIG_FILE}"; exit 1; }
  CONF_LOADED=1
elif [[ -n "${CLI_PREFIX}" && -f "${CLI_PREFIX}/danmo-install.conf" ]]; then
  load_conf "${CLI_PREFIX}/danmo-install.conf" && CONF_LOADED=1
elif [[ -n "${CLI_ROLE}" && -f "${INSTALL_STATE_DIR}/installs.d/${CLI_ROLE}.conf" ]]; then
  load_conf "${INSTALL_STATE_DIR}/installs.d/${CLI_ROLE}.conf" && CONF_LOADED=1
elif [[ -f "${INSTALL_STATE_DIR}/install.conf" ]]; then
  load_conf "${INSTALL_STATE_DIR}/install.conf" && CONF_LOADED=1
else
  warn "No ${INSTALL_STATE_DIR}/install.conf — using PREFIX=/opt/danmo ROLE=all."
  warn "Pass --prefix= / --role= or re-run install.sh to record state."
fi

[[ -n "${CLI_PREFIX}" ]] && PREFIX="${CLI_PREFIX}"
[[ -n "${CLI_ROLE}" ]] && ROLE="${CLI_ROLE}"
PREFIX="${PREFIX:-/opt/danmo}"
ROLE="${ROLE:-all}"

# If user forced --role on CLI, derive components from that role.
# Otherwise keep INSTALL_* from conf when present (partial "all" installs).
if [[ -n "${CLI_ROLE}" ]]; then
  set_components_from_role
elif [[ "${CONF_LOADED}" -eq 0 ]]; then
  set_components_from_role
elif [[ "${INSTALL_RELAY}" != "0" && "${INSTALL_RELAY}" != "1" ]]; then
  set_components_from_role
fi

FROM_DIR="$(cd "${FROM_DIR}" && pwd)"
BIN_SRC="${FROM_DIR}"
[[ -d "${FROM_DIR}/bin" ]] && BIN_SRC="${FROM_DIR}/bin"

BACKUP_DIR="${BACKUP_DIR:-${PREFIX}/.upgrade-backup-$(date +%Y%m%d%H%M%S)}"
mkdir -p "${BACKUP_DIR}/bin" "${PREFIX}/bin"

stop_one() {
  local u="$1"
  if systemctl is-active --quiet "${u}" 2>/dev/null; then
    systemctl stop "${u}" || true
    info "Stopped ${u}"
  elif systemctl is-enabled --quiet "${u}" 2>/dev/null; then
    systemctl stop "${u}" || true
    info "Stopped ${u}"
  fi
}

start_one() {
  local u="$1"
  if [[ -f "/etc/systemd/system/${u}.service" ]] || systemctl cat "${u}.service" &>/dev/null; then
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

# Refresh install.conf timestamps / keep domains after upgrade
touch_install_conf() {
  if [[ -f "${PREFIX}/danmo-install.conf" ]]; then
    if grep -q '^INSTALLED_AT=' "${PREFIX}/danmo-install.conf" 2>/dev/null; then
      sed -i "s|^INSTALLED_AT=.*|INSTALLED_AT=$(date -u +%Y-%m-%dT%H:%M:%SZ)|" "${PREFIX}/danmo-install.conf" 2>/dev/null \
        || true
    fi
  fi
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

touch_install_conf

if [[ "${SKIP_NGINX}" -eq 0 ]]; then
  nginx_args=(--nginx-only --prefix="${PREFIX}" --role="${ROLE}" --non-interactive)
  [[ -n "${API_DOMAIN}" ]] && nginx_args+=(--api-domain="${API_DOMAIN}")
  [[ -n "${ADMIN_DOMAIN}" ]] && nginx_args+=(--admin-domain="${ADMIN_DOMAIN}")
  [[ -n "${USER_DOMAIN}" ]] && nginx_args+=(--user-domain="${USER_DOMAIN}")
  "${SCRIPT_DIR}/install.sh" "${nginx_args[@]}"
fi

info "Done. Check logs for 'applied migration N' (sqlite now; mysql/ when enabled)."
info "journalctl -u danmo-relay -u danmo-admin -u danmo-user -n 50 --no-pager"
