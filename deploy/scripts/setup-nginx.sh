#!/usr/bin/env bash
# Deprecated: host Nginx install was removed. Edge Nginx runs inside Compose.
echo "setup-nginx.sh is deprecated." >&2
echo "Use ./deploy/scripts/install.sh (Compose edge service) or edit deploy/compose/nginx/templates/." >&2
exit 1
