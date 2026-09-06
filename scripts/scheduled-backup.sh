#!/bin/sh
# The unattended wrapper around backup-postgres.sh.
#
#   ./scripts/scheduled-backup.sh
#
# Two things a scheduled backup needs that a manual one does not: it must prune,
# or the disk fills until the backup that matters cannot be written; and it must
# leave a record, because a job that fails quietly every night for a month is
# worse than no job at all.
#
# Installed by scripts/com.hemsidor24.backup.plist. See docs/postgres-backup.md.
set -eu

: "${DATABASE_URL:?set DATABASE_URL}"
HERE=$(cd "$(dirname "$0")" && pwd)
BACKUPS="${HEMSIDOR24_BACKUP_DIR:-$HOME/Library/Application Support/hemsidor24/backups/postgres}"
LOG="$BACKUPS/backup.log"

# Keep every dump for this many days, and never fall below KEEP_MIN however old
# they are — a laptop that was shut for three weeks must not wake up and delete
# its way to nothing.
KEEP_DAYS="${HEMSIDOR24_KEEP_DAYS:-14}"
KEEP_MIN="${HEMSIDOR24_KEEP_MIN:-7}"

mkdir -p "$BACKUPS"; chmod 700 "$BACKUPS"
say() { printf '%s  %s\n' "$(date '+%Y-%m-%d %H:%M:%S')" "$1" >> "$LOG"; }

say "starting"
if ! OUT=$("$HERE/backup-postgres.sh" "$BACKUPS" 2>&1); then
    printf '%s\n' "$OUT" | sed 's/^/    /' >> "$LOG"
    say "FAILED — no backup was written"
    printf '%s\n' "$OUT" >&2
    exit 1
fi
say "$(printf '%s\n' "$OUT" | grep '^OK  /' | sed 's|^OK  ||')"

# Prune. Deliberately narrow: only files this tool writes, matched by the exact
# name shape, in this directory alone, newest kept regardless of age.
CANDIDATES=$(ls -t "$BACKUPS"/hemsidor24-????????-??????.dump 2>/dev/null || true)
TOTAL=$(printf '%s' "$CANDIDATES" | grep -c . || true)
REMOVED=0
if [ "$TOTAL" -gt "$KEEP_MIN" ]; then
    printf '%s\n' "$CANDIDATES" | tail -n +$((KEEP_MIN + 1)) | while IFS= read -r f; do
        [ -n "$f" ] || continue
        # -mtime +N is "older than N days"; the newest KEEP_MIN never reach here.
        if [ -n "$(find "$f" -mtime +"$KEEP_DAYS" 2>/dev/null)" ]; then
            rm -f "$f" && say "pruned $(basename "$f")"
        fi
    done
    REMOVED=$(( TOTAL - $(ls "$BACKUPS"/hemsidor24-????????-??????.dump 2>/dev/null | wc -l | tr -d ' ') ))
fi

KEPT=$(ls "$BACKUPS"/hemsidor24-????????-??????.dump 2>/dev/null | wc -l | tr -d ' ')
say "kept $KEPT backup(s), pruned $REMOVED"

# Verify the dump just written. Deliberately after the backup, never before: the
# dump is the thing that must exist, and a verification failure must not be able
# to prevent one being taken. The same order the application uses when it writes
# to Postgres before it tries to send mail.
#
# Set HEMSIDOR24_SKIP_VERIFY=1 to leave it out — but then nothing is checking
# that these files restore, and an unverified backup is only a hope.
NEW=$(printf '%s\n' "$OUT" | grep '^OK  /' | head -1 | sed 's|^OK  ||')
if [ "${HEMSIDOR24_SKIP_VERIFY:-0}" = "1" ]; then
    say "verification skipped by request"
elif VERIFY_OUT=$("$HERE/verify-postgres-backup.sh" "$NEW" 2>&1); then
    say "verified — restores and matches the source"
else
    printf '%s\n' "$VERIFY_OUT" | sed 's/^/    /' >> "$LOG"
    say "VERIFY FAILED — a dump was written but it did not restore cleanly"
    printf '%s\n' "$VERIFY_OUT" >&2
    exit 1
fi

say "done"
printf '%s\n' "$OUT"
echo "log: $LOG"
