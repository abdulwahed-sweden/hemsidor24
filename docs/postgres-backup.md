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

## Running it nightly

```sh
sed -e "s|__REPO__|$PWD|" -e "s|__HOME__|$HOME|" \
    scripts/com.hemsidor24.backup.plist > ~/Library/LaunchAgents/com.hemsidor24.backup.plist
launchctl bootstrap gui/$UID ~/Library/LaunchAgents/com.hemsidor24.backup.plist
```

A macOS user agent, 03:20 daily; launchd runs it on wake if the machine was
asleep. Run it on demand with
`launchctl kickstart gui/$UID/com.hemsidor24.backup`, and remove it with
`launchctl bootout gui/$UID/com.hemsidor24.backup`.

`scripts/scheduled-backup.sh` is the wrapper, and does three things the manual
command does not:

- **Prunes.** Keeps 14 days, and never drops below the newest 7 however old they
  are — a laptop shut for a month must not wake up and delete its way to
  nothing. It only ever removes files matching its own dump-name shape in its
  own directory.
- **Logs**, to `backup.log` beside the dumps. A job that fails quietly every
  night for a month is worse than no job.
- **Verifies each dump**, after writing it, never before. The dump is the thing
  that must exist, so a verification failure cannot prevent one being taken —
  the same order the application uses when it writes to Postgres before it tries
  to send mail. `HEMSIDOR24_SKIP_VERIFY=1` turns it off, at the cost of nothing
  checking that these files restore.

Tune with `HEMSIDOR24_KEEP_DAYS`, `HEMSIDOR24_KEEP_MIN`, `HEMSIDOR24_BACKUP_DIR`.

On a Linux host, schedule the same script with a systemd timer or cron instead;
nothing in it is macOS-specific.

**The cell is not on this schedule.** Its backup needs the back office stopped
and external storage attached, so it stays a deliberate operator step.

## Before production

- [x] A backup schedule exists (launchd, daily, verifying)
- [ ] Backups are written to storage separate from the database host
- [ ] A restore has been verified from the storage that will actually be used
- [ ] The Sijill cell is backed up too — see the companion note
