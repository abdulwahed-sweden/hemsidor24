-- Store the services as the customer typed them: one per line, in a TEXT
-- column, instead of a TEXT[] array.
--
-- Why: the back office reads this table through rustio-admin, whose ORM maps
-- Postgres types onto a fixed `Value` enum with no array variant. A column it
-- cannot read is a column the studio cannot see, and "what did they actually
-- order" is the most useful thing on the page.
--
-- Nothing is lost. `hemsidor24-core::Services` parses exactly this shape —
-- one service per line, blank lines forgiven — and it is the same text the
-- customer typed into the textarea. The 1..=6 rule was never really enforced
-- here anyway: it lives in core, where it is tested, and the array CHECK was
-- a duplicate of it.

-- The array CHECK from 0001 has to go first: it names `services` and would
-- block the type change. Postgres auto-named it `orders_services_check`.
ALTER TABLE orders DROP CONSTRAINT IF EXISTS orders_services_check;

ALTER TABLE orders
    ALTER COLUMN services TYPE TEXT
    USING array_to_string(services, E'\n');

ALTER TABLE orders
    ADD CONSTRAINT orders_services_not_empty
    CHECK (length(btrim(services)) > 0);
