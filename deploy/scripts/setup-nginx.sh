#!/usr/bin/env bash
# Deprecated compatibility entry → install.sh --nginx-only
# Prefer: sudo ./install.sh   (full first deploy) or sudo ./install.sh --nginx-only
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
echo "note: setup-nginx.sh is deprecated; forwarding to install.sh --nginx-only" >&2
exec "${SCRIPT_DIR}/install.sh" --nginx-only "$@"
