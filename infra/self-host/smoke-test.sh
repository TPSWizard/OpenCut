#!/usr/bin/env bash
# Checks a running self-hosted ConzaCut: the editor, the API under /api, a
# share round trip, and that wrangler's local explorer is not exposed.
#   bash smoke-test.sh http://127.0.0.1:8080 <SHARE_UPLOAD_KEY>
set -euo pipefail
base=${1:-http://127.0.0.1:8080}
key=${2:-}
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

field() { sed -n "s/.*\"$1\":\"\([^\"]*\)\".*/\1/p"; }
fail() { echo "FAIL: $*" >&2; exit 1; }
status() { curl -s -o /dev/null -w '%{http_code}' "$@"; }

[ "$(status "$base/")" = 200 ] || fail "home page"
curl -s "$base/" | grep -qi "<html" || fail "home page is not HTML"
[ "$(status "$base/projects")" = 200 ] || fail "projects page"
[ "$(status "$base/sw.js")" = 200 ] || fail "service worker"
curl -s "$base/api/health" | grep -q '"healthy":true' || fail "API health"
explorer=$(status "$base/cdn-cgi/local/explorer/api/r2/buckets")
[ "$explorer" != 200 ] || fail "wrangler's local explorer is exposed"
echo "pages, API and closed explorer ($explorer): ok"

if [ -z "$key" ]; then
  [ "$(status -X POST "$base/api/share/start" -H 'Content-Type: application/json' -d '{"size":1}')" = 503 ] \
    || fail "sharing without a passcode should answer 503"
  echo "sharing is off (no passcode given): ok"
  exit 0
fi

[ "$(status -X POST "$base/api/share/start" -H 'X-Share-Key: wrong' -H 'Content-Type: application/json' -d '{"size":1}')" = 401 ] \
  || fail "wrong passcode should answer 401"
head -c 4096 /dev/urandom > "$work/bundle"
start=$(curl -fsS -X POST "$base/api/share/start" -H "X-Share-Key: $key" -H 'Content-Type: application/json' -d '{"size":4096,"name":"Smoke test"}')
id=$(printf '%s' "$start" | field id)
upload=$(printf '%s' "$start" | field uploadId)
part=$(curl -fsS -X PUT "$base/api/share/part?id=$id&uploadId=$upload&part=1" -H "X-Share-Key: $key" --data-binary @"$work/bundle")
etag=$(printf '%s' "$part" | field etag)
done=$(curl -fsS -X POST "$base/api/share/complete" -H "X-Share-Key: $key" -H 'Content-Type: application/json' \
  -H 'X-Forwarded-Proto: https' -H 'X-Forwarded-Host: cut.example.test' \
  -d "{\"id\":\"$id\",\"uploadId\":\"$upload\",\"parts\":[{\"partNumber\":1,\"etag\":\"$etag\"}]}")
url=$(printf '%s' "$done" | field url)
[ "$url" = "https://cut.example.test/api/share/$id" ] || fail "share URL $url"
curl -fsS -o "$work/back" "$base/api/share/$id"
cmp -s "$work/bundle" "$work/back" || fail "shared bytes differ"
echo "share round trip ($id): ok"
