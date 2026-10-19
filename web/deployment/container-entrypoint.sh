#!/bin/sh

set -eu

backend_base_url=${BACKEND_BASE_URL:?BACKEND_BASE_URL is required}
case "$backend_base_url" in
  http://*|https://*) ;;
  *)
    echo "error: BACKEND_BASE_URL must be an absolute HTTP or HTTPS URL" >&2
    exit 1
    ;;
esac
case "$backend_base_url" in
  *[!A-Za-z0-9:/._~-]*)
    echo "error: BACKEND_BASE_URL contains unsupported characters" >&2
    exit 1
    ;;
esac

printf 'window.__HEALTH_BASE_URL__ = "%s";\n' "$backend_base_url" > /tmp/config.js
exec nginx -g 'daemon off;'
