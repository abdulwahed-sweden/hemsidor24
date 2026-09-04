# hemsidor24

Fixed-price one-page websites for small businesses in Sweden.
Start 2 490 kr, Pro 4 490 kr. No subscription.

This repository is the studio: the public page, the order intake, and the back
office behind it. It is being rebuilt from a single static `index.html` into a
Rust workspace, one reviewable phase at a time.

## Workspace layout

| Crate | Job | Status |
| --- | --- | --- |
| `hemsidor24-core` | Domain types and validation rules. No I/O, no web, no database. | Phase 1 — done |
| `hemsidor24-web` | Axum. Serves the public page, accepts the order form. Server-rendered HTML. | Phase 3 — done |
| `hemsidor24-notify` | Outbound email only. Trait-based so it can be faked in tests. | Phase 3 — done |
| `hemsidor24-admin` | Back office over Postgres. Not visible to customers. | Phase 4 — done |
| `hemsidor24-handover` | Sijill delivery claims. Feature-gated, off by default. | Phase 5 — done, undecided |

The dependency arrows point one way. `core` depends on nothing in this
workspace; everything else may depend on `core`; nothing at all may depend on
`hemsidor24-handover`.

Postgres is the single source of truth for the studio side. SQLite is not used
here — that belongs to the per-customer PRO sites, which are a different
product in a different repository.

## How an order is handled

`POST /bestall` does three things in a fixed order, and the order is the design:

1. **Judge.** Honeypot, then a minimum time on the page, then a per-IP rate
   limit. A caught submission gets the same confirmation a real one does —
   telling a bot why it failed only helps it try again.
2. **Store.** Validated by `hemsidor24-core`, written to Postgres. This is the
   only step allowed to fail the request.
3. **Notify.** The studio gets the order, the customer gets a receipt. Mail
   failures are logged loudly and swallowed, because by then the order is safe.

An order that reached Postgres survives any mail outage. An order that was only
emailed is gone the moment the inbox is tidied. Hence the order.

The stored IP is `sha256(ip || IP_HASH_SALT)`, computed by Postgres. The raw
address never reaches a column, a log line or a backup.

## Handover claims

`hemsidor24-handover` signs the delivery promises — proposal shown, proposal
accepted, revision used, refund issued, ownership transferred — into an
append-only Sijill chain, and renders a receipt from them.

It is **off by default and nothing else depends on it**:

```sh
cargo run -p hemsidor24-handover --features handover --example receipt
```

The protocol crates are pinned by revision rather than tag. A tag is a name its
owner can move; a receipt is exactly the kind of document that has to rebuild to
the same bytes years later, or be shown to have done so.

### What a receipt is worth

**This is single-sided provenance, not a mutual exchange.** Sijill is built for
independent participants who each keep and sign their own records and
countersign what they receive. That is not what happens here. The studio runs a
cell; the customer does not. Every entry is the studio's own account of what the
studio did, signed by the studio's own key.

A receipt establishes that the studio's key signed exactly that text, that the
text has not changed since, and that entries have not been reordered or removed
from the middle. It does **not** establish that any of it is true, that the
customer agrees, when anything actually happened, or that nothing was left off
the end of the chain — truncation is not detectable from a chain alone.

Against a customer who says "I never approved that", this is a contemporaneous,
tamper-evident note the studio wrote to itself. That is better than a mutable
row in a database the studio also controls, and weaker than a countersigned
record. The caveats are printed on every receipt rather than kept here, so they
cannot be separated from the claims they qualify.

Making it mutual would mean the customer holding a key and signing their own
acceptance. That is a product decision, not a technical one: asking a
small-business owner to manage a signing key is a real cost, and the promise of
this product is that nothing about it is annoying.

### Bodies carry references, not names

Sijill's guidance is that a signed body should carry references the issuing cell
can resolve locally, never names — a body naming a person names them for as long
as the record exists. Customers appear as `order_ref`, never as a company name,
email or phone number. The two exceptions are `domain` and `repo_url`, which are
the *subject* of the promise and already public; a receipt saying only "a domain
was transferred" would be useless to the reader it is written for.

## Known limits

The back office is `rustio-admin` 0.33.0.

**Fixed upstream, in 0.33.1.** 0.33.0 bound every `Value::Null` as `None::<i64>`,
so any row holding NULL in a nullable non-`bigint` column could not be updated —
which meant exactly the orders worth acting on (`notified_at IS NULL`, nobody
emailed) were the ones whose status could not be changed. `orm::create` and
`orm::update` now emit the SQL literal `NULL`, which takes its type from the
column being written, instead of binding a typed guess.

`hemsidor24-admin` pins that fix by **exact upstream revision**
(`5eb63a7e12ab279f9f5ccab09e01d7aeb99bc03f`, released as `v0.33.1`) rather than
a tag or a version range: a tag is a name its owner can move, and a range
floats. `orm_contract::a_row_with_null_timestamps_can_be_updated` guards that
this build actually receives the fixed behaviour.
`docs/rustio-admin-null-bind.patch` is kept as provenance only and takes no part
in the build.

How that fix was verified, precisely: rustio-admin's PostgreSQL/testcontainers
integration suite was run **locally** against the fix, and the regression was
shown to fail on the unpatched implementation and pass with it. At the time of
this baseline the upstream GitHub workflow did **not** execute that suite — no
job enables the `integration-test` feature — so the database regression did not
run in upstream CI. Adding that coverage is a separate upstream follow-up and
is not part of this repository.

**Optional free text** was a second symptom of the same bug, not a separate
limitation. The framework already handles `Option<String>` correctly: it trims,
collapses empty to `None`, and stores NULL. The columns were simply declared
`NOT NULL DEFAULT ''`, which gave every row two ways to say "nothing here" and
left the panel demanding a value for notes nobody had written. Migration 0004
makes them nullable and the models use `Option<String>`. No framework change was
needed.

The one shape rustio-admin still cannot express is a `NOT NULL` text column that
accepts the empty string — Django's `blank=True` without `null=True`. Nothing
here needs it, and it is not worth an attribute until something does.

## The rules the code enforces

The sales copy makes promises out loud, and `hemsidor24-core::scope` is where
they are written down: one page, at most six services, one revision included.
Changing those constants changes what the business has promised, which is why
they live in one file with the reasoning next to them.

## Building

```sh
cargo run -p hemsidor24-web      # http://127.0.0.1:3000
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

Queries are checked at compile time against the schema. The generated data in
`.sqlx/` is committed, so a fresh clone builds with no database:

```sh
SQLX_OFFLINE=true cargo build --workspace
```

After changing a query or a migration, regenerate it:

```sh
DATABASE_URL=postgres://... cargo sqlx prepare --workspace -- --all-targets
```

The four database-backed tests skip unless `TEST_DATABASE_URL` is set:

```sh
TEST_DATABASE_URL=postgres://postgres@localhost/hemsidor24_test cargo test
```

The toolchain is pinned in `rust-toolchain.toml`. Copy `.env.example` to `.env`
for local settings; `BIND_ADDR` and `PORT` both have defaults.

`reference/` holds the original design export. It is input material only —
nothing in the build reads from it at runtime.

## Phase status

- [x] **Phase 1** — workspace skeleton, domain types, validation tests
- [x] **Phase 2** — Axum serving the page from Askama templates
- [x] **Phase 3** — the order form: validation, Postgres, email
- [x] **Phase 4** — back office (see *Known limits*)
- [x] **Phase 5** — Sijill handover claims (see *Handover claims*; may be removed)

The design pass comes last, after the phases above. No colours, typography or
copy have been changed.
