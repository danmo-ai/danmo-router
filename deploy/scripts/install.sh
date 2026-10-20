#!/usr/bin/env bash
# First-time deploy: .env, systemd, Nginx, start processes.
# SQL schema is applied by the binary on startup (incremental via schema_migrations).
# Usage:
#   sudo ./install.sh
#   sudo ./install.sh --role=user --prefix=/opt/danmo-user --non-interactive
#   sudo ./install.sh --nginx-only
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEPLOY_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
# Release tarball root is parent of deploy/; git checkout may use repo root.
PACKAGE_ROOT="$(cd "${DEPLOY_DIR}/.." && pwd)"
EXAMPLES_NGINX="${DEPLOY_DIR}/examples/nginx"
EXAMPLES_SYSTEMD="${DEPLOY_DIR}/examples/systemd"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

info() { echo -e "${GREEN}==>${NC} $*"; }
warn() { echo -e "${YELLOW}==>${NC} $*"; }
err() { echo -e "${RED}==>${NC} $*" >&2; }

ROLE=all
PREFIX=/opt/danmo
NGINX_DIR=/etc/nginx/conf.d
API_DOMAIN=api.example.com
ADMIN_DOMAIN=admin.example.com
USER_DOMAIN=user.example.com
NON_INTERACTIVE=0
NGINX_ONLY=0
SKIP_NGINX=0
SKIP_SYSTEMD=0
START_SERVICES=1
# Written by install; read by upgrade.sh (see write_install_conf).
INSTALL_STATE_DIR=/etc/danmo

usage() {
  cat <<EOF
Usage: sudo $0 [options]

  --role=all|relay|admin|user   Components on this host (default: all)
  --prefix=DIR                  Install root (default: /opt/danmo)
  --nginx-dir=DIR               Nginx conf.d (default: /etc/nginx/conf.d)
  --api-domain=HOST
  --admin-domain=HOST
  --user-domain=HOST
  --nginx-only                  Only render/install Nginx site configs
  --skip-nginx                  Skip Nginx
  --skip-systemd                Skip systemd units
  --no-start                    Do not systemctl enable --now
  --non-interactive             No prompts (use flags / defaults)
  -h, --help

Writes install state to:
  \${PREFIX}/danmo-install.conf
  ${INSTALL_STATE_DIR}/installs.d/<role>.conf
  ${INSTALL_STATE_DIR}/install.conf   (default for upgrade.sh)
EOF
}

for arg in "$@"; do
  case "$arg" in
    --role=*) ROLE="${arg#*=}" ;;
    --prefix=*) PREFIX="${arg#*=}" ;;
    --nginx-dir=*) NGINX_DIR="${arg#*=}" ;;
    --api-domain=*) API_DOMAIN="${arg#*=}" ;;
    --admin-domain=*) ADMIN_DOMAIN="${arg#*=}" ;;
    --user-domain=*) USER_DOMAIN="${arg#*=}" ;;
    --nginx-only) NGINX_ONLY=1 ;;
    --skip-nginx) SKIP_NGINX=1 ;;
    --skip-systemd) SKIP_SYSTEMD=1 ;;
    --no-start) START_SERVICES=0 ;;
    --non-interactive) NON_INTERACTIVE=1 ;;
    -h|--help) usage; exit 0 ;;
    *) err "Unknown option: $arg"; usage; exit 1 ;;
  esac
done

case "$ROLE" in
  all|relay|admin|user) ;;
  *) err "Invalid --role=$ROLE"; exit 1 ;;
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

prompt() {
  local var_name="$1"
  local text="$2"
  local default="${3:-}"
  local input
  if [[ "${NON_INTERACTIVE}" -eq 1 ]]; then
    printf -v "${var_name}" '%s' "${default}"
    return
  fi
  if [[ -n "${default}" ]]; then
    read -r -p "${text} [${default}]: " input
    printf -v "${var_name}" '%s' "${input:-${default}}"
  else
    read -r -p "${text}: " input
    printf -v "${var_name}" '%s' "${input}"
  fi
}

yesno() {
  local text="$1"
  local default="${2:-y}"
  if [[ "${NON_INTERACTIVE}" -eq 1 ]]; then
    [[ "${default}" =~ ^[Yy] ]]
    return
  fi
  local hint="Y/n"
  [[ "${default}" == "n" ]] && hint="y/N"
  local input
  read -r -p "${text} [${hint}]: " input
  input="${input:-${default}}"
  [[ "${input}" =~ ^[Yy] ]]
}

if [[ "$(id -u)" -ne 0 ]]; then
  err "Run as root (sudo $0)."
  exit 1
fi

if [[ ! -d "${EXAMPLES_NGINX}" ]]; then
  err "Missing ${EXAMPLES_NGINX}"
  exit 1
fi

info "danmo-router 首次发布 (install.sh)  role=${ROLE}"
echo ""

if [[ "${NON_INTERACTIVE}" -eq 0 ]]; then
  prompt PREFIX "安装根目录" "${PREFIX}"
  prompt NGINX_DIR "Nginx 配置目录" "${NGINX_DIR}"
  prompt API_DOMAIN "转发 API 域名" "${API_DOMAIN}"
  prompt ADMIN_DOMAIN "管理 Portal 域名" "${ADMIN_DOMAIN}"
  prompt USER_DOMAIN "用户 Portal 域名" "${USER_DOMAIN}"
  if [[ "${ROLE}" == "all" ]]; then
    INSTALL_RELAY=1; INSTALL_ADMIN=1; INSTALL_USER=1
    if ! yesno "本机安装 relay?" "y"; then INSTALL_RELAY=0; fi
    if ! yesno "本机安装 admin Portal + API?" "y"; then INSTALL_ADMIN=0; fi
    if ! yesno "本机安装 user Portal + API?" "y"; then INSTALL_USER=0; fi
  fi
fi

install_env() {
  mkdir -p "${PREFIX}"
  local env_src="${PACKAGE_ROOT}/.env.example"
  [[ -f "${env_src}" ]] || env_src="${DEPLOY_DIR}/../.env.example"
  if [[ ! -f "${PREFIX}/.env" ]]; then
    if [[ -f "${env_src}" ]]; then
      cp "${env_src}" "${PREFIX}/.env"
      info "Created ${PREFIX}/.env from .env.example — edit ENCRYPTION_KEY / passwords"
    else
      warn "No .env.example found; create ${PREFIX}/.env manually"
    fi
  else
    info "Keeping existing ${PREFIX}/.env"
  fi
}

install_binaries() {
  mkdir -p "${PREFIX}/bin"
  local bin_src="${PACKAGE_ROOT}"
  if [[ -d "${PACKAGE_ROOT}/bin" ]]; then
    bin_src="${PACKAGE_ROOT}/bin"
  fi
  if [[ "${INSTALL_RELAY}" -eq 1 ]]; then
    if [[ -f "${bin_src}/danmo-relay" ]]; then
      cp -f "${bin_src}/danmo-relay" "${PREFIX}/bin/"
      chmod +x "${PREFIX}/bin/danmo-relay"
    elif [[ -f "${PACKAGE_ROOT}/packages/danmo-relay/bin/danmo-relay" ]]; then
      cp -f "${PACKAGE_ROOT}/packages/danmo-relay/bin/danmo-relay" "${PREFIX}/bin/"
      chmod +x "${PREFIX}/bin/danmo-relay"
    else
      warn "danmo-relay binary not found under ${bin_src}"
    fi
  fi
  if [[ "${INSTALL_ADMIN}" -eq 1 ]]; then
    if [[ -f "${bin_src}/danmo-admin" ]]; then
      cp -f "${bin_src}/danmo-admin" "${PREFIX}/bin/"
      chmod +x "${PREFIX}/bin/danmo-admin"
    elif [[ -f "${PACKAGE_ROOT}/packages/danmo-admin-portal/bin/danmo-admin" ]]; then
      cp -f "${PACKAGE_ROOT}/packages/danmo-admin-portal/bin/danmo-admin" "${PREFIX}/bin/"
      chmod +x "${PREFIX}/bin/danmo-admin"
    else
      warn "danmo-admin binary not found"
    fi
    mkdir -p "${PREFIX}/apps/admin"
    if [[ -d "${PACKAGE_ROOT}/apps/admin" ]]; then
      cp -R "${PACKAGE_ROOT}/apps/admin/." "${PREFIX}/apps/admin/"
    elif [[ -d "${PACKAGE_ROOT}/packages/danmo-admin-portal/apps/admin" ]]; then
      cp -R "${PACKAGE_ROOT}/packages/danmo-admin-portal/apps/admin/." "${PREFIX}/apps/admin/"
    else
      warn "admin Portal static not found"
    fi
  fi
  if [[ "${INSTALL_USER}" -eq 1 ]]; then
    if [[ -f "${bin_src}/danmo-user" ]]; then
      cp -f "${bin_src}/danmo-user" "${PREFIX}/bin/"
      chmod +x "${PREFIX}/bin/danmo-user"
    elif [[ -f "${PACKAGE_ROOT}/packages/danmo-user-portal/bin/danmo-user" ]]; then
      cp -f "${PACKAGE_ROOT}/packages/danmo-user-portal/bin/danmo-user" "${PREFIX}/bin/"
      chmod +x "${PREFIX}/bin/danmo-user"
    else
      warn "danmo-user binary not found"
    fi
    mkdir -p "${PREFIX}/apps/user"
    if [[ -d "${PACKAGE_ROOT}/apps/user" ]]; then
      cp -R "${PACKAGE_ROOT}/apps/user/." "${PREFIX}/apps/user/"
    elif [[ -d "${PACKAGE_ROOT}/packages/danmo-user-portal/apps/user" ]]; then
      cp -R "${PACKAGE_ROOT}/packages/danmo-user-portal/apps/user/." "${PREFIX}/apps/user/"
    else
      warn "user Portal static not found"
    fi
  fi
}

install_systemd() {
  [[ -d "${EXAMPLES_SYSTEMD}" ]] || { err "Missing ${EXAMPLES_SYSTEMD}"; exit 1; }
  render_unit() {
    local name="$1"
    local port="$2"
    local src="${EXAMPLES_SYSTEMD}/${name}.service"
    local dest="/etc/systemd/system/${name}.service"
    sed \
      -e "s|/opt/danmo|${PREFIX}|g" \
      -e "s|Environment=PORT=.*|Environment=PORT=${port}|g" \
      "${src}" > "${dest}"
    info "Installed ${dest}"
  }
  [[ "${INSTALL_RELAY}" -eq 1 ]] && render_unit danmo-relay 3000
  [[ "${INSTALL_ADMIN}" -eq 1 ]] && render_unit danmo-admin 3001
  [[ "${INSTALL_USER}" -eq 1 ]] && render_unit danmo-user 3002
  systemctl daemon-reload
}

install_nginx() {
  if ! command -v nginx >/dev/null 2>&1; then
    if [[ "${NON_INTERACTIVE}" -eq 1 ]]; then
      warn "nginx not installed; skipping apt (non-interactive)"
      return 0
    fi
    if yesno "未检测到 nginx，是否尝试用 apt 安装? (仅 Debian/Ubuntu)" "n"; then
      apt-get update && apt-get install -y nginx
    else
      err "请先安装 nginx"
      exit 1
    fi
  fi
  mkdir -p "${NGINX_DIR}" /var/www/certbot
  local DANMO_ROOT="${PREFIX}"
  render() {
    local src="$1"
    local dest="$2"
    sed \
      -e "s|api.example.com|${API_DOMAIN}|g" \
      -e "s|admin.example.com|${ADMIN_DOMAIN}|g" \
      -e "s|user.example.com|${USER_DOMAIN}|g" \
      -e "s|/opt/danmo|${DANMO_ROOT}|g" \
      "${src}" > "${dest}"
    info "Wrote ${dest}"
  }
  [[ "${INSTALL_RELAY}" -eq 1 ]] && render "${EXAMPLES_NGINX}/danmo-relay.conf" "${NGINX_DIR}/danmo-relay.conf"
  [[ "${INSTALL_ADMIN}" -eq 1 ]] && render "${EXAMPLES_NGINX}/danmo-portal-admin.conf" "${NGINX_DIR}/danmo-portal-admin.conf"
  [[ "${INSTALL_USER}" -eq 1 ]] && render "${EXAMPLES_NGINX}/danmo-portal-user.conf" "${NGINX_DIR}/danmo-portal-user.conf"

  check_cert() {
    local domain="$1"
    [[ -f "/etc/letsencrypt/live/${domain}/fullchain.pem" && -f "/etc/letsencrypt/live/${domain}/privkey.pem" ]]
  }
  local missing=0
  [[ "${INSTALL_RELAY}" -eq 1 ]] && ! check_cert "${API_DOMAIN}" && missing=1
  [[ "${INSTALL_ADMIN}" -eq 1 ]] && ! check_cert "${ADMIN_DOMAIN}" && missing=1
  [[ "${INSTALL_USER}" -eq 1 ]] && ! check_cert "${USER_DOMAIN}" && missing=1
  if [[ "${missing}" -eq 1 ]]; then
    warn "部分 Let’s Encrypt 证书路径尚不存在。"
    echo "  示例: certbot certonly --webroot -w /var/www/certbot -d ${API_DOMAIN}"
    echo "  云证书请改 ssl_certificate 路径后再 reload。"
    if [[ "${NON_INTERACTIVE}" -eq 1 ]] || ! yesno "证书未齐，仍尝试 nginx -t?" "n"; then
      info "Nginx 配置已写入；补证书后: nginx -t && systemctl reload nginx"
      return 0
    fi
  fi
  if nginx -t; then
    if [[ "${NON_INTERACTIVE}" -eq 1 ]] || yesno "nginx -t 通过，是否 reload?" "y"; then
      systemctl reload nginx
      info "nginx reloaded"
    fi
  else
    err "nginx -t failed"
    exit 1
  fi
}

start_units() {
  [[ "${INSTALL_RELAY}" -eq 1 ]] && systemctl enable --now danmo-relay
  [[ "${INSTALL_ADMIN}" -eq 1 ]] && systemctl enable --now danmo-admin
  [[ "${INSTALL_USER}" -eq 1 ]] && systemctl enable --now danmo-user
  info "Services started. Schema migrations run inside the process (sqlite/; MySQL TBD)."
}

write_install_conf() {
  local conf_body
  conf_body=$(cat <<EOF
# Generated by danmo-router install.sh — consumed by upgrade.sh
# Do not put secrets here (.env stays at PREFIX/.env).
PREFIX=${PREFIX}
ROLE=${ROLE}
NGINX_DIR=${NGINX_DIR}
API_DOMAIN=${API_DOMAIN}
ADMIN_DOMAIN=${ADMIN_DOMAIN}
USER_DOMAIN=${USER_DOMAIN}
INSTALL_RELAY=${INSTALL_RELAY}
INSTALL_ADMIN=${INSTALL_ADMIN}
INSTALL_USER=${INSTALL_USER}
INSTALLED_AT=$(date -u +%Y-%m-%dT%H:%M:%SZ)
EOF
)
  mkdir -p "${PREFIX}" "${INSTALL_STATE_DIR}/installs.d"
  printf '%s\n' "${conf_body}" > "${PREFIX}/danmo-install.conf"
  chmod 644 "${PREFIX}/danmo-install.conf"
  # Per-role record (multiple prefixes on one host: relay + user, etc.)
  local role_key="${ROLE}"
  printf '%s\n' "${conf_body}" > "${INSTALL_STATE_DIR}/installs.d/${role_key}.conf"
  chmod 644 "${INSTALL_STATE_DIR}/installs.d/${role_key}.conf"
  # Default for upgrade.sh when --prefix / --role omitted
  printf '%s\n' "${conf_body}" > "${INSTALL_STATE_DIR}/install.conf"
  chmod 644 "${INSTALL_STATE_DIR}/install.conf"
  info "Wrote install state: ${PREFIX}/danmo-install.conf"
  info "Wrote ${INSTALL_STATE_DIR}/install.conf (upgrade default) and installs.d/${role_key}.conf"
}

if [[ "${NGINX_ONLY}" -eq 1 ]]; then
  install_nginx
  write_install_conf
  echo ""
  info "CORS_ORIGINS=https://${ADMIN_DOMAIN},https://${USER_DOMAIN}"
  info "PUBLIC_RELAY_URL=https://${API_DOMAIN}/v1"
  exit 0
fi

install_env
install_binaries
[[ "${SKIP_SYSTEMD}" -eq 0 ]] && install_systemd
[[ "${SKIP_NGINX}" -eq 0 ]] && install_nginx
[[ "${START_SERVICES}" -eq 1 && "${SKIP_SYSTEMD}" -eq 0 ]] && start_units
write_install_conf

echo ""
info "Done. Edit ${PREFIX}/.env then restart units if needed."
info "Upgrade later: sudo upgrade.sh --from-dir=<new-release>  (reads ${INSTALL_STATE_DIR}/install.conf)"
info "CORS_ORIGINS=https://${ADMIN_DOMAIN},https://${USER_DOMAIN}"
info "PUBLIC_RELAY_URL=https://${API_DOMAIN}/v1"
info "DATABASE_URL: sqlite://… now; mysql://… when MySQL support lands (migrations/mysql/)."
