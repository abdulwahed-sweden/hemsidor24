#!/bin/sh
# Back up the Hemsidor24 database.
#
#   ./scripts/backup-postgres.sh [destination-directory]
#
# Custom-format dump (-Fc): compressed, and restorable selectively with
# pg_restore, which a plain SQL file is not.
#
# This is NOT a backup of the Sijill cell. See docs/postgres-backup.md.
set -eu

: "${DATABASE_URL:?set DATABASE_URL to the database to back up}"
DEST="${1:-$HOME/Library/Application Support/hemsidor24/backups/postgres}"

# host:port/database, so the log says which database was dumped without ever
# printing the user or password that reached it.
target() {
    rest=${1#*://}
    authority=${rest%%/*}
    path=${rest#*/}
    database=${path%%\?*}
    hostport=${authority##*@}
    case "$hostport" in
        *:*) host=${hostport%:*}; port=${hostport##*:} ;;
        *)   host=$hostport;      port=5432 ;;
    esac
    printf '%s:%s/%s' "$host" "$port" "$database"
}

mkdir -p "$DEST"
chmod 700 "$DEST"
FILE="$DEST/hemsidor24-$(date +%Y%m%d-%H%M%S).dump"

# Never overwrite silently. A backup that quietly replaced another is how one
# bad night's dump erases the good one from the night before.
if [ -e "$FILE" ]; then
    echo "REFUSING: $FILE already exists." >&2
    exit 1
fi

echo "database:    $(target "$DATABASE_URL")"
echo "destination: $FILE"

if ! pg_dump --format=custom --no-owner --no-privileges --file="$FILE" "$DATABASE_URL"; then
    # pg_dump leaves a partial file behind on failure, and a partial dump that
    # looks like a backup is worse than no backup at all.
    rm -f "$FILE"
    echo "FAILED: pg_dump did not complete. No backup was written." >&2
    exit 1
fi
chmod 600 "$FILE"

# Reading the archive's own table of contents proves the file is a well-formed
# dump rather than merely non-empty.
if ! ENTRIES=$(pg_restore --list "$FILE" 2>/dev/null | grep -c '^[0-9]'); then
    echo "FAILED: $FILE is not a readable dump archive." >&2
    exit 1
fi
TABLES=$(pg_restore --list "$FILE" 2>/dev/null | grep -c 'TABLE DATA' || true)

echo
echo "OK  $FILE"
echo "OK  $(wc -c < "$FILE" | tr -d ' ') bytes, $ENTRIES archive entries, $TABLES table(s) with data"
echo
echo "Not yet proven. Verify it restores:"
echo "  ./scripts/verify-postgres-backup.sh \"$FILE\""
