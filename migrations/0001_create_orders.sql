-- Orders as they arrive from the public form.
--
-- This table is the source of truth. An order is written here before any email
-- is attempted, so a mail outage can never lose one.

CREATE TABLE orders (
    id             BIGSERIAL PRIMARY KEY,

    -- What the customer told us. Validated by hemsidor24-core before it
    -- reaches this table; the CHECKs are a second lock on the same door.
    company        TEXT        NOT NULL CHECK (length(company) BETWEEN 1 AND 120),
    city           TEXT        NOT NULL CHECK (length(city)    BETWEEN 1 AND 80),
    phone          TEXT        NOT NULL CHECK (length(phone)   BETWEEN 1 AND 32),
    email          TEXT        NOT NULL CHECK (length(email)   BETWEEN 1 AND 254),

    -- One service per element, in the order the customer wrote them.
    -- At most six: the scope limit the sales copy promises out loud.
    services       TEXT[]      NOT NULL CHECK (
                       array_length(services, 1) BETWEEN 1 AND 6),

    -- Stable slugs from hemsidor24-core, never the Swedish labels.
    style          TEXT        NOT NULL CHECK (style IN
                       ('klassisk','modern','djarv','student','forening','overraska-mig')),
    package        TEXT        NOT NULL CHECK (package IN ('start','pro')),

    -- Where the order is in its life. Transitions are enforced in core;
    -- this CHECK only keeps unknown values out.
    status         TEXT        NOT NULL DEFAULT 'ny' CHECK (status IN
                       ('ny','utkast_skickat','godkand','publicerad','avbruten')),

    received_at    TIMESTAMPTZ NOT NULL DEFAULT now(),

    -- Spam forensics only, and deliberately not the raw address:
    -- sha256(ip || salt), hex. Retention is 30 days per the privacy policy.
    ip_hash        TEXT,
    user_agent     TEXT,

    -- Whether the studio notification actually went out. False here means an
    -- order arrived that nobody was emailed about — worth a query.
    notified_at    TIMESTAMPTZ
);

-- The admin panel lists newest first, filtered by status.
CREATE INDEX orders_status_received_at_idx ON orders (status, received_at DESC);
CREATE INDEX orders_received_at_idx        ON orders (received_at DESC);
