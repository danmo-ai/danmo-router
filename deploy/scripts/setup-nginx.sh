#!/usr/bin/env bash
# Interactive helper: render deploy/examples/nginx/*.conf from templates and install under nginx.
# Does NOT ship nginx/certbot binaries. Run on the target host (often as root).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEPLOY_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
EXAMPLES="${DEPLOY_DIR}/examples/nginx"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

info() { echo -e "${GREEN}==>${NC} $*"; }
warn() { echo -e "${YELLOW}==>${NC} $*"; }
err() { echo -e "${RED}==>${NC} $*" >&2; }

if [[ ! -d "${EXAMPLES}" ]]; then
  err "Missing ${EXAMPLES}. Run from release tree (dist/deploy/scripts/) or git deploy/scripts/."
  exit 1
fi

if [[ "$(id -u)" -ne 0 ]]; then
  err "Run as root (or sudo $0)."
  exit 1
fi

prompt() {
  local var_name="$1"
  local text="$2"
  local default="${3:-}"
  local input
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
  local hint="Y/n"
  [[ "${default}" == "n" ]] && hint="y/N"
  local input
  read -r -p "${text} [${hint}]: " input
  input="${input:-${default}}"
  [[ "${input}" =~ ^[Yy] ]]
}

info "danmo-router Nginx 配置向导（仅生成/安装站点配置，不替代云 LB/CDN 方案）"
echo ""

prompt DANMO_ROOT "安装根目录" "/opt/danmo"
prompt NGINX_DIR "Nginx 配置目录" "/etc/nginx/conf.d"
prompt API_DOMAIN "转发 API 域名 (relay)" "api.example.com"
prompt ADMIN_DOMAIN "管理 Portal 域名" "admin.example.com"
prompt USER_DOMAIN "用户 Portal 域名" "user.example.com"

INSTALL_RELAY=y
INSTALL_ADMIN=y
INSTALL_USER=y
if ! yesno "本机是否安装 relay (API) 配置?" "y"; then INSTALL_RELAY=n; fi
if ! yesno "本机是否安装 admin Portal 配置?" "y"; then INSTALL_ADMIN=n; fi
if ! yesno "本机是否安装 user Portal 配置?" "y"; then INSTALL_USER=n; fi

if [[ ! -d "${DANMO_ROOT}/apps/admin" && "${INSTALL_ADMIN}" == "y" ]]; then
  warn "未找到 ${DANMO_ROOT}/apps/admin，请先 make package 并解压 Portal 静态文件。"
fi
if [[ ! -d "${DANMO_ROOT}/apps/user" && "${INSTALL_USER}" == "y" ]]; then
  warn "未找到 ${DANMO_ROOT}/apps/user，请先解压用户 Portal。"
fi

if ! command -v nginx >/dev/null 2>&1; then
  if yesno "未检测到 nginx，是否尝试用 apt 安装? (仅 Debian/Ubuntu)" "n"; then
    apt-get update && apt-get install -y nginx
  else
    err "请先安装 nginx 后再执行 nginx -t / reload。"
    exit 1
  fi
fi

mkdir -p "${NGINX_DIR}"
mkdir -p /var/www/certbot

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

[[ "${INSTALL_RELAY}" == "y" ]] && render "${EXAMPLES}/danmo-relay.conf" "${NGINX_DIR}/danmo-relay.conf"
[[ "${INSTALL_ADMIN}" == "y" ]] && render "${EXAMPLES}/danmo-portal-admin.conf" "${NGINX_DIR}/danmo-portal-admin.conf"
[[ "${INSTALL_USER}" == "y" ]] && render "${EXAMPLES}/danmo-portal-user.conf" "${NGINX_DIR}/danmo-portal-user.conf"

check_cert() {
  local domain="$1"
  local cert="/etc/letsencrypt/live/${domain}/fullchain.pem"
  local key="/etc/letsencrypt/live/${domain}/privkey.pem"
  [[ -f "${cert}" && -f "${key}" ]]
}

missing=0
[[ "${INSTALL_RELAY}" == "y" ]] && ! check_cert "${API_DOMAIN}" && missing=1
[[ "${INSTALL_ADMIN}" == "y" ]] && ! check_cert "${ADMIN_DOMAIN}" && missing=1
[[ "${INSTALL_USER}" == "y" ]] && ! check_cert "${USER_DOMAIN}" && missing=1

if [[ "${missing}" -eq 1 ]]; then
  warn "部分 Let’s Encrypt 证书路径尚不存在（配置中默认 /etc/letsencrypt/live/<域名>/）。"
  echo "  可先申请证书，例如："
  [[ "${INSTALL_RELAY}" == "y" ]] && echo "    certbot certonly --webroot -w /var/www/certbot -d ${API_DOMAIN}"
  [[ "${INSTALL_ADMIN}" == "y" ]] && echo "    certbot certonly --webroot -w /var/www/certbot -d ${ADMIN_DOMAIN}"
  [[ "${INSTALL_USER}" == "y" ]] && echo "    certbot certonly --webroot -w /var/www/certbot -d ${USER_DOMAIN}"
  echo "  或使用 certbot --nginx -d <域名>（会修改 server 块）。"
  echo "  云证书请手工改 ssl_certificate 路径后再 reload。"
  if ! yesno "证书未齐，仍尝试 nginx -t? (通常会失败)" "n"; then
    info "已写入配置，请补证书后执行: nginx -t && systemctl reload nginx"
    exit 0
  fi
fi

if nginx -t; then
  if yesno "nginx -t 通过，是否 reload nginx?" "y"; then
    systemctl reload nginx
    info "Done."
  else
    info "配置已安装，请自行 reload。"
  fi
else
  err "nginx -t 失败，请修正证书或配置。"
  exit 1
fi

echo ""
info "别忘了 .env: CORS_ORIGINS=https://${ADMIN_DOMAIN},https://${USER_DOMAIN}"
info "PUBLIC_RELAY_URL=https://${API_DOMAIN}/v1"
