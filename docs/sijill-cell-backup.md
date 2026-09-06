# Backing up the Sijill cell

Operator note. Applies only to deployments built with `--features handover`.

## Where the cell lives

```
~/Library/Application Support/hemsidor24/cell        mode 0700
```

CellId `135bdf571611b47cbadd3b73246109393f9eccbdd6e449d55691faeacbe4f45d`,
created 2026-09-06. Record that value: it is public, and it is what proves a
restore brought back the same cell rather than a new one.

Chosen for what it avoids as much as what it offers. It is outside the
repository, so it cannot be committed. It is under `~/Library`, which is
outside the iCloud container — `~/Desktop` and `~/Documents` would sync an
unprotected signing key to Apple if Desktop & Documents sync were ever turned
on, and a Dropbox or Drive folder would do the same today. It survives rebuilds
and `cargo clean`, which `/tmp` and any scratch directory do not.

**The identity is portable, so this path is not a commitment.** When a
deployment target is chosen, the cell moves there by *restore* — the procedure
below, proven to preserve the CellId. Do not create a second cell on the new
host; that would start an unrelated chain.

## Not yet safe for real handovers

Two gaps on the machine holding this cell, both outside what the repository can
fix:

1. **FileVault is off.** `cell.key` is an unprotected signing seed sitting on an
   unencrypted disk. Anyone with the file, or with the powered-off machine, can
   sign as this cell.
2. **No off-machine backup.** No Time Machine destination is configured. The
   copy under `backups/` is on the same disk: it survives an accidental `rm` of
   the cell directory and nothing else.

Until both are fixed, treat this cell as **staging**. It has signed nothing, so
today it costs nothing to discard and recreate — which is exactly why this is
the moment to fix them, and not after the first customer handover.

Before this cell signs a real handover:

- [ ] FileVault enabled on the machine holding it
- [ ] A backup taken to separate physical storage
- [ ] That off-machine backup restore-tested (procedure below)

## What this covers, and what it does not

`HANDOVER_CELL_DIR` holds the studio's signing identity and its append-only
chain of handoff claims.

**A PostgreSQL backup does not back up the cell.** They are separate durability
problems and need separate procedures:

| | Postgres | Sijill cell |
| --- | --- | --- |
| Holds | orders, customers, statuses, refunds | signing identity, signed claims |
| A lost item is | a lost record | unrecoverable — the key cannot be regenerated |
| Restoring most of it | is a partial recovery | is **not a restore** |

A replacement key is a different cell. Claims signed by the old one remain
valid, but nothing signed afterwards joins the same chain.

## Rules

1. **Back up the whole directory as one unit** — `cell.key`, `log.sijill` and
   `log.sijill.horizon` together, in one operation.
2. **Partial restoration is not acceptable.** See the warning below; it does not
   fail the way you would expect.
3. **`cell.key` is secret signing material.** It is the unprotected signing
   seed: anyone holding it can issue claims that verify as this cell. Protect
   backup copies to the same standard as any other production credential —
   restricted access, encrypted at rest, never in the repository, never in a
   shared drive or ticket attachment.
4. **Take the backup with the application stopped.** A cell directory takes one
   holder at a time.
5. **Prove the restore before relying on it.** An untested backup is not a
   backup.

## The dangerous failure

A directory containing only `cell.key` **opens successfully**, reports the
**correct cell id**, and presents an **empty chain**. No error, nothing in the
log.

So an operator who restores a backup and checks only the id will conclude the
restore worked when every claim is gone. **Always check the claim count too.**

(The reverse — a log with no key — fails outright, which is the safer failure.)

## Procedure

```text
1. create the cell deliberately     HANDOVER_CELL_INIT=1, once, on purpose
2. record the CellId                from the startup log
3. stop the application             the cell takes one holder at a time
4. back up HANDOVER_CELL_DIR        the whole directory, one unit
5. restore into a clean directory   not over the top of an existing one
6. open the restored cell
7. confirm the CellId is identical
8. confirm the claim count matches  ← the step that catches a partial restore
9. confirm it can still append
```

Steps 5–9 are exercised as tests:

```sh
cargo test -p hemsidor24-handover --features handover --test backup_restore
```

Those tests prove a full round trip preserves the cell id, keeps every claim
verifiable, and leaves a cell that can still extend its own chain — and that a
half-restored directory does not announce itself.

## Before production

- [x] A durable location for `HANDOVER_CELL_DIR` has been chosen
- [x] The cell has been created deliberately and its CellId recorded
- [x] A backup exists, taken with the application stopped (same disk only)
- [ ] That backup is on **separate physical storage**
- [ ] The backup is encrypted at rest (FileVault is currently off)
- [x] A restore has been performed into a clean directory
- [x] The restored CellId matched

Note: this cell has signed nothing yet, so the claim-count check has nothing to
compare. Until it has claims, verify a restore by CellId **and** by confirming
all three files are present — a key-only restore would otherwise look correct.
