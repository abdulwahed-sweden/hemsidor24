#!/bin/sh
# Back up the Sijill cell to a destination, and prove the copy restores.
#
#   ./scripts/backup-cell.sh /Volumes/<your disk>/hemsidor24-cell-backup
#
# An untested backup is not a backup, so this does not stop at copying: it
# reopens the copy in a scratch directory and checks the CellId and the claim
# count against the original. Run it from the repository root, with the back
# office stopped — a cell directory takes one holder at a time.
set -eu

SRC="${HANDOVER_CELL_DIR:-$HOME/Library/Application Support/hemsidor24/cell}"
DEST="${1:-}"

if [ -z "$DEST" ]; then
    echo "usage: $0 <destination-directory>" >&2
    echo "  source cell: $SRC" >&2
    exit 2
fi

# The destination must not be somewhere that republishes the key. cell.key is
# an unprotected signing seed: a cloud-synced folder hands it to a third party,
# and the repository would publish it permanently on the first commit.
case "$DEST" in
    *"Mobile Documents"*|*CloudStorage*|*Dropbox*|*"Google Drive"*|*OneDrive*)
        echo "REFUSING: $DEST is inside a cloud-synced folder." >&2
        echo "cell.key is an unprotected signing seed and must not be synced." >&2
        exit 1 ;;
    /tmp/*|/private/tmp/*|/var/folders/*)
        echo "REFUSING: $DEST is temporary storage, not a backup." >&2
        exit 1 ;;
esac
case "$(cd "$(dirname "$DEST")" 2>/dev/null && pwd || echo "$DEST")" in
    "$(pwd)"*) echo "REFUSING: $DEST is inside the repository." >&2; exit 1 ;;
esac

for f in cell.key log.sijill log.sijill.horizon; do
    [ -f "$SRC/$f" ] || { echo "REFUSING: $SRC/$f is missing. A partial backup is not a backup." >&2; exit 1; }
done

echo "source:      $SRC"
echo "destination: $DEST"

mkdir -p "$DEST"
chmod 700 "$DEST"
cp -p "$SRC"/cell.key "$SRC"/log.sijill "$SRC"/log.sijill.horizon "$DEST"/
chmod 600 "$DEST/cell.key"

echo "verifying the copy is byte-for-byte identical..."
for f in cell.key log.sijill log.sijill.horizon; do
    a=$(shasum -a 256 "$SRC/$f"  | cut -d' ' -f1)
    b=$(shasum -a 256 "$DEST/$f" | cut -d' ' -f1)
    [ "$a" = "$b" ] || { echo "FAILED: $f differs between source and backup" >&2; exit 1; }
done

echo "restore-testing the backup into a scratch directory..."
SCRATCH=$(mktemp -d)
trap 'rm -rf "$SCRATCH"' EXIT
cp -p "$DEST"/* "$SCRATCH"/

inspect() { HANDOVER_CELL_DIR="$1" cargo run -q -p hemsidor24-handover \
                --features handover --example inspect 2>/dev/null; }

before=$(inspect "$SRC")
after=$(inspect "$SCRATCH")

id_before=$(printf '%s\n' "$before" | head -1)
id_after=$(printf '%s\n' "$after"  | head -1)
n_before=$(printf '%s\n' "$before" | grep -c '^  \[' || true)
n_after=$(printf '%s\n' "$after"   | grep -c '^  \[' || true)

[ -n "$id_after" ] || { echo "FAILED: the restored copy could not be opened" >&2; exit 1; }
[ "$id_before" = "$id_after" ] || {
    echo "FAILED: CellId changed" >&2
    echo "  original: $id_before" >&2
    echo "  restored: $id_after" >&2
    exit 1; }
[ "$n_before" = "$n_after" ] || {
    echo "FAILED: claim count changed ($n_before -> $n_after)." >&2
    echo "A key-only restore opens cleanly and shows an empty chain." >&2
    exit 1; }

echo
echo "OK  $id_after"
echo "OK  $n_after claim(s) present after restore"
echo "OK  backup verified at $DEST"
echo
echo "This copy is only as durable as the disk it is on. Keep it on separate"
echo "physical storage, and keep that storage encrypted."
