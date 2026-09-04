-- Free text that may genuinely be absent becomes nullable.
--
-- These columns were NOT NULL DEFAULT '', which gave every row two ways to say
-- "nothing here": NULL and the empty string. The back office reads them through
-- rustio-admin, whose form layer treats a bare `String` field as required and
-- rejects "" — so a NOT NULL DEFAULT '' column could never be saved empty, and
-- the panel demanded a value for notes nobody had written yet.
--
-- `Option<String>` is the shape the framework already handles correctly: it
-- trims, collapses empty to None, and stores NULL. One way to say "nothing".
--
-- (This only became usable once rustio-admin stopped binding every NULL as a
-- bigint — see docs/rustio-admin-null-bind.patch. Before that fix a NULL here
-- made the row uneditable.)

ALTER TABLE customers  ALTER COLUMN notes         DROP NOT NULL,
                       ALTER COLUMN notes         DROP DEFAULT;
ALTER TABLE domains    ALTER COLUMN registrar     DROP NOT NULL,
                       ALTER COLUMN registrar     DROP DEFAULT,
                       ALTER COLUMN notes         DROP NOT NULL,
                       ALTER COLUMN notes         DROP DEFAULT;
ALTER TABLE sites      ALTER COLUMN repo_url      DROP NOT NULL,
                       ALTER COLUMN repo_url      DROP DEFAULT,
                       ALTER COLUMN live_url      DROP NOT NULL,
                       ALTER COLUMN live_url      DROP DEFAULT,
                       ALTER COLUMN notes         DROP NOT NULL,
                       ALTER COLUMN notes         DROP DEFAULT;
ALTER TABLE deliveries ALTER COLUMN refund_reason DROP NOT NULL,
                       ALTER COLUMN refund_reason DROP DEFAULT,
                       ALTER COLUMN notes         DROP NOT NULL,
                       ALTER COLUMN notes         DROP DEFAULT;

-- Collapse the existing empty strings so there is only one "absent".
UPDATE customers  SET notes         = NULL WHERE notes         = '';
UPDATE domains    SET registrar     = NULL WHERE registrar     = '';
UPDATE domains    SET notes         = NULL WHERE notes         = '';
UPDATE sites      SET repo_url      = NULL WHERE repo_url      = '';
UPDATE sites      SET live_url      = NULL WHERE live_url      = '';
UPDATE sites      SET notes         = NULL WHERE notes         = '';
UPDATE deliveries SET refund_reason = NULL WHERE refund_reason = '';
UPDATE deliveries SET notes         = NULL WHERE notes         = '';
