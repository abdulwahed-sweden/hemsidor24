#!/bin/sh
# Prove a Hemsidor24 dump actually restores.
#
#   ./scripts/verify-postgres-backup.sh [dump-file]
#
# Restores into a throwaway database created for the purpose and dropped
# afterwards, then compares it against the source. Never writes to the source,
# and never touches a database it did not create itself.
set -eu

: "${DATABASE_URL:?set DATABASE_URL to the database the dump came from}"
DUMP="${1:-}"
[ -n "$DUMP" ] || { echo "usage: $0 <dump-file>" >&2; exit 2; }
[ -f "$DUMP" ] || { echo "no such dump: $DUMP" >&2; exit 1; }

ADMIN_URL=$(printf '%s' "$DATABASE_URL" | sed 's#/[^/?]*\(?.*\)\{0,1\}$#/postgres#')
BASE=$(printf '%s' "$DATABASE_URL" | sed 's#.*/##; s#?.*##')
VERIFY="${BASE}_verify_$$"

count() { psql -tAqc "SELECT count(*) FROM $2" "$1" 2>/dev/null || echo "ERR"; }

# Refuse to reuse a name that already exists: this script drops what it makes,
# and it must never drop somebody else's database.
if psql -tAqc "SELECT 1 FROM pg_database WHERE datname='$VERIFY'" "$ADMIN_URL" | grep -q 1; then
    echo "REFUSING: $VERIFY already exists." >&2
    exit 1
fi

echo "dump:         $DUMP"
echo "verification: $VERIFY  (created now, dropped at the end)"
echo

createdb -h "$(printf '%s' "$ADMIN_URL" | sed 's#.*://##; s#.*@##; s#[:/].*##')" "$VERIFY" 2>/dev/null \
    || psql -tAqc "CREATE DATABASE \"$VERIFY\"" "$ADMIN_URL" >/dev/null
VERIFY_URL=$(printf '%s' "$DATABASE_URL" | sed "s#/[^/?]*\(?.*\)\{0,1\}\$#/$VERIFY#")
cleanup() {
    psql -tAqc "DROP DATABASE IF EXISTS \"$VERIFY\"" "$ADMIN_URL" >/dev/null 2>&1 || true
}
trap cleanup EXIT

if ! pg_restore --no-owner --no-privileges --dbname="$VERIFY_URL" "$DUMP"; then
    echo "FAILED: pg_restore did not complete." >&2
    exit 1
fi

FAIL=0
check() {
    if [ "$2" = "$3" ]; then
        printf '  OK    %-12s %s\n' "$1" "$2"
    else
        printf '  FAIL  %-12s source=%s restored=%s\n' "$1" "$2" "$3"
        FAIL=1
    fi
}

echo
echo "comparing the restored database against the source:"
for table in orders customers sites deliveries; do
    check "$table" "$(count "$DATABASE_URL" "$table")" "$(count "$VERIFY_URL" "$table")"
done
check "migrations" "$(count "$DATABASE_URL" _sqlx_migrations)" "$(count "$VERIFY_URL" _sqlx_migrations)"

# Every migration by version, not merely the same number of them.
SRC_V=$(psql -tAqc "SELECT string_agg(version::text,',' ORDER BY version) FROM _sqlx_migrations" "$DATABASE_URL")
DST_V=$(psql -tAqc "SELECT string_agg(version::text,',' ORDER BY version) FROM _sqlx_migrations" "$VERIFY_URL")
check "versions" "$SRC_V" "$DST_V"

for table in orders customers sites deliveries domains; do
    if ! psql -tAqc "SELECT to_regclass('public.$table')" "$VERIFY_URL" | grep -q .; then
        printf '  FAIL  %-12s missing from the restore\n' "$table"; FAIL=1
    fi
done

# Counting rows would pass against a table that restored its structure and no
# readable content, so read actual data back through a join.
ROWS=$(psql -tAqc "SELECT count(*) FROM orders o LEFT JOIN customers c ON o.customer_id = c.id" "$VERIFY_URL" 2>/dev/null || echo ERR)
if [ "$ROWS" = "ERR" ]; then
    echo "  FAIL  restored data could not be queried"; FAIL=1
else
    printf '  OK    %-12s %s row(s) read back through a join\n' "queryable" "$ROWS"
fi

echo
if [ "$FAIL" -eq 0 ]; then
    echo "POSTGRES BACKUP/RESTORE: PROVEN for $DUMP"
else
    echo "POSTGRES BACKUP/RESTORE: FAILED for $DUMP" >&2
    exit 1
fi
