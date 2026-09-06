# Backing up the Sijill cell

Operator note. Applies only to deployments built with `--features handover`.

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

- [ ] A durable location for `HANDOVER_CELL_DIR` has been chosen
- [ ] The cell has been created deliberately and its CellId recorded
- [ ] A backup exists, taken with the application stopped
- [ ] The backup is access-controlled and encrypted at rest
- [ ] A restore has been performed into a clean directory
- [ ] The restored CellId matched, and the claim count matched
