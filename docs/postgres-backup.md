# Backing up PostgreSQL

Operator note. Companion to [`sijill-cell-backup.md`](sijill-cell-backup.md).

## These are two backups, not one

```
PostgreSQL  ≠  Sijill cell
```

A complete Hemsidor24 recovery needs **both**:

| | PostgreSQL | `HANDOVER_CELL_DIR` |
| --- | --- | --- |
| Restores | commercial and application state — orders, customers, sites, deliveries, statuses, refunds, back-office users | the studio's signing identity and its append-only handoff history |
| Tool | `pg_dump` / `pg_restore` | copy the directory whole |
| If lost | records are gone, but nothing is unrecoverable in principle | the key cannot be regenerated; a replacement is a different cell |

**Neither substitutes for the other.** A perfect database restore leaves you
unable to sign a handover. A perfect cell restore leaves you not knowing who the
customers were.

## Taking a backup

```sh
./scripts/backup-postgres.sh [destination-directory]
```

Reads `DATABASE_URL`. Writes a timestamped PostgreSQL **custom-format** dump
(`-Fc`) — compressed, and restorable selectively with `pg_restore`, which a
plain SQL file is not. Default destination is
`~/Library/Application Support/hemsidor24/backups/postgres`.

It refuses to overwrite an existing file, deletes the partial dump if `pg_dump`
fails rather than leaving something that looks like a backup, and prints the
database as `host:port/name` so credentials never reach the output.

## Proving it

A backup is not a backup until it restores.

```sh
./scripts/verify-postgres-backup.sh <dump-file>
```

Creates a throwaway database, restores into it, compares it against the source,
and drops it again. It never writes to the source and never touches a database
it did not create. Checks:

- row counts match for `orders`, `customers`, `sites`, `deliveries`
- every migration is present, by version and not merely by count
- the principal tables exist
- data reads back through a join — structure alone is not a restore

Exits non-zero if any check fails.

## Before production

- [ ] A backup schedule exists (this is a manual command; nothing runs it for you)
- [ ] Backups are written to storage separate from the database host
- [ ] A restore has been verified from the storage that will actually be used
- [ ] The Sijill cell is backed up too — see the companion note
