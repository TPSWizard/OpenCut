#!/bin/sh
# Starts the self-hosted ConzaCut: gateway (the only port), web app and API
# worker in one wrangler session, with shares and other state under /data.
set -eu
cd /app/infra/self-host

# The API worker's secrets and settings, from the container's environment.
# Unset ones are left out, and the matching feature answers 503.
vars=api/.dev.vars
: > "$vars"
chmod 600 "$vars"
for name in SHARE_UPLOAD_KEY FREESOUND_API_KEY PIXABAY_API_KEY CORS_ORIGIN; do
  eval "value=\${$name:-}"
  if [ -n "$value" ]; then
    # .dev.vars is dotenv, which has no escapes: quote with whichever quote
    # mark the value does not contain.
    case "$value" in
      *"'"*"\""* | *"\""*"'"*)
        echo "$name contains both ' and \"; use a value without one of them" >&2
        exit 1 ;;
      *"'"*) quote='"' ;;
      *) quote="'" ;;
    esac
    case "$value" in
      *"
"*) echo "$name must be on one line" >&2; exit 1 ;;
    esac
    printf '%s=%s%s%s\n' "$name" "$quote" "$value" "$quote" >> "$vars"
  fi
done

# Never expose wrangler's local storage explorer or trace viewer.
export X_LOCAL_EXPLORER=false X_LOCAL_OBSERVABILITY=false WRANGLER_SEND_METRICS=false

exec /app/apps/api/node_modules/.bin/wrangler dev \
  -c gateway.jsonc \
  -c /app/apps/web/dist/server/wrangler.json \
  -c api/wrangler.jsonc \
  --ip 0.0.0.0 --port "${PORT:-8080}" \
  --persist-to /data \
  --show-interactive-dev-session=false \
  --log-level "${LOG_LEVEL:-info}"
