#!/bin/bash
# Run both Hemsidor24 binaries in one container.
#
# They are two processes because they are two servers: the public site on 3000
# and the back office on 3001, which binds separately and shares no route with
# it. That separation is the product's, not the demo's, so this runs both rather
# than pretending otherwise.
set -euo pipefail

# The public binary owns the migrations, so it goes first and the back office
# waits for it. Otherwise the first admin page load races the schema.
hemsidor24-web &
WEB=$!

for _ in $(seq 1 60); do
    if curl -fsS http://127.0.0.1:3000/health >/dev/null 2>&1; then break; fi
    if ! kill -0 "$WEB" 2>/dev/null; then
        echo "entrypoint: the public site exited during startup" >&2
        exit 1
    fi
    sleep 1
done

hemsidor24-admin &
ADMIN=$!

echo "entrypoint: public site on :3000, back office on :3001/admin/"

# If either dies the container must die too, rather than serve half a demo.
wait -n "$WEB" "$ADMIN"
STATUS=$?
echo "entrypoint: a server exited (status $STATUS), stopping the other" >&2
kill "$WEB" "$ADMIN" 2>/dev/null || true
exit "$STATUS"
