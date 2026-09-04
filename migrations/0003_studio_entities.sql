-- The rest of the studio's world, as the back office needs to see it.
--
-- An order is what arrives. Everything below is what happens afterwards: the
-- customer it belongs to, the domain registered in their name, the site built
-- for them, and the delivery that hands it all over.
--
-- Every table carries a BIGSERIAL `id`, because rustio-admin's `Model` trait
-- requires `fn id(&self) -> i64`.
--
-- Every nullable moment is TIMESTAMPTZ rather than DATE. rustio-admin's field
-- mapping has no nullable-date kind (only optional string, timestamp and i64),
-- and "not yet delivered" has to be expressible. It also matches `received_at`
-- and `notified_at` on orders, so the whole schema tells time one way.

CREATE TABLE customers (
    id           BIGSERIAL PRIMARY KEY,
    company      TEXT        NOT NULL CHECK (length(company) BETWEEN 1 AND 120),
    city         TEXT        NOT NULL CHECK (length(city)    BETWEEN 1 AND 80),
    phone        TEXT        NOT NULL CHECK (length(phone)   BETWEEN 1 AND 32),
    email        TEXT        NOT NULL CHECK (length(email)   BETWEEN 1 AND 254),
    org_nr       TEXT,
    notes        TEXT        NOT NULL DEFAULT '',
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX customers_company_idx ON customers (company);

-- An order becomes a customer once it is accepted. Nullable because an order
-- that is never accepted never gets one.
ALTER TABLE orders ADD COLUMN customer_id BIGINT REFERENCES customers (id);
CREATE INDEX orders_customer_id_idx ON orders (customer_id);

-- Domains are registered in the customer's name and paid by them directly.
-- We only ever administer them, which is why `registrar` and the dates matter
-- more than anything about us.
CREATE TABLE domains (
    id           BIGSERIAL PRIMARY KEY,
    customer_id  BIGINT      REFERENCES customers (id),
    name         TEXT        NOT NULL UNIQUE CHECK (length(name) BETWEEN 3 AND 253),
    registrar    TEXT        NOT NULL DEFAULT '',
    -- Whose name the contract is in. The promise is "ditt namn", so anything
    -- other than 'customer' is a deviation worth being able to query for.
    registered_to TEXT       NOT NULL DEFAULT 'customer'
                             CHECK (registered_to IN ('customer', 'studio')),
    registered_at TIMESTAMPTZ,
    expires_at    TIMESTAMPTZ,
    notes        TEXT        NOT NULL DEFAULT '',
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX domains_customer_id_idx ON domains (customer_id);
CREATE INDEX domains_expires_at_idx  ON domains (expires_at);

-- The site we build. One per order, in practice.
CREATE TABLE sites (
    id           BIGSERIAL PRIMARY KEY,
    customer_id  BIGINT      REFERENCES customers (id),
    order_id     BIGINT      REFERENCES orders (id),
    domain_id    BIGINT      REFERENCES domains (id),
    -- The repository the customer owns. Part of what is handed over.
    repo_url     TEXT        NOT NULL DEFAULT '',
    live_url     TEXT        NOT NULL DEFAULT '',
    -- Mirrors hemsidor24-core::Package. Slugs, never labels.
    package      TEXT        NOT NULL CHECK (package IN ('start', 'pro')),
    -- One revision is included; anything past that is billable.
    revisions_used INTEGER   NOT NULL DEFAULT 0 CHECK (revisions_used >= 0),
    published_at TIMESTAMPTZ,
    notes        TEXT        NOT NULL DEFAULT '',
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX sites_customer_id_idx ON sites (customer_id);
CREATE INDEX sites_order_id_idx    ON sites (order_id);

-- The handover itself: the moment the promises come due.
--
-- These columns are deliberately the same facts phase 5 would sign as Sijill
-- claims. If that phase stays, this table is what it draws from; if it goes,
-- this table is still the record.
CREATE TABLE deliveries (
    id            BIGSERIAL PRIMARY KEY,
    site_id       BIGINT      REFERENCES sites (id),
    customer_id   BIGINT      REFERENCES customers (id),
    offered_at    TIMESTAMPTZ,
    accepted_at   TIMESTAMPTZ,
    -- Ownership transfer, itemised, because these are the promises that get
    -- disputed later.
    domain_transferred    BOOLEAN NOT NULL DEFAULT false,
    hosting_transferred   BOOLEAN NOT NULL DEFAULT false,
    source_transferred    BOOLEAN NOT NULL DEFAULT false,
    refunded_at   TIMESTAMPTZ,
    refund_reason TEXT        NOT NULL DEFAULT '',
    notes         TEXT        NOT NULL DEFAULT '',
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX deliveries_site_id_idx     ON deliveries (site_id);
CREATE INDEX deliveries_customer_id_idx ON deliveries (customer_id);
