#!/bin/bash
# Render danmo nginx site configs from templates (runs via nginx image /docker-entrypoint.d/).
set -euo pipefail

SRC=/etc/nginx/templates-src
DST=/etc/nginx/conf.d

API_DOMAIN="${API_DOMAIN:-api.localhost}"
ADMIN_DOMAIN="${ADMIN_DOMAIN:-admin.localhost}"
USER_DOMAIN="${USER_DOMAIN:-user.localhost}"
RELAY_UPSTREAM="${RELAY_UPSTREAM:-relay:3000}"
ADMIN_UPSTREAM="${ADMIN_UPSTREAM:-admin:3001}"
USER_UPSTREAM="${USER_UPSTREAM:-user:3002}"
ENABLE_TLS="${ENABLE_TLS:-0}"
SSL_CERT="${SSL_CERT:-/etc/nginx/certs/fullchain.pem}"
SSL_KEY="${SSL_KEY:-/etc/nginx/certs/privkey.pem}"

export API_DOMAIN ADMIN_DOMAIN USER_DOMAIN RELAY_UPSTREAM ADMIN_UPSTREAM USER_UPSTREAM SSL_CERT SSL_KEY

mkdir -p "$DST"
rm -f "$DST"/danmo-*.conf

render() {
  local name="$1"
  envsubst '${API_DOMAIN} ${ADMIN_DOMAIN} ${USER_DOMAIN} ${RELAY_UPSTREAM} ${ADMIN_UPSTREAM} ${USER_UPSTREAM} ${SSL_CERT} ${SSL_KEY}' \
    <"${SRC}/${name}.conf.template" >"${DST}/${name}.conf"
}

if [[ "${ENABLE_TLS}" == "1" || "${ENABLE_TLS}" == "true" ]]; then
  if [[ ! -f "${SSL_CERT}" || ! -f "${SSL_KEY}" ]]; then
    echo "danmo-edge: ENABLE_TLS=1 but cert/key missing (${SSL_CERT}, ${SSL_KEY})" >&2
    exit 1
  fi
  render danmo-relay-tls
  render danmo-portal-admin-tls
  render danmo-portal-user-tls
else
  render danmo-relay-http
  render danmo-portal-admin-http
  render danmo-portal-user-http
fi

echo "danmo-edge: rendered nginx conf (TLS=${ENABLE_TLS}) for ${API_DOMAIN}, ${ADMIN_DOMAIN}, ${USER_DOMAIN}"
