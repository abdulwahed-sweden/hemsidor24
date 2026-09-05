-- Which hosting account was handed over.
--
-- `deliveries.hosting_transferred` has been a flag since 0003, and nothing
-- recorded *what* was transferred. The public page promises "domän och
-- webbhotell i ditt namn", and the domain has always been identifiable through
-- `sites.domain_id` while the hosting account was not identifiable at all.
--
-- That matters more now than it did: publishing signs a handoff claim naming
-- each artefact that passed to the customer, and an artefact with no identifier
-- cannot honestly be named in one. Without this column the hosting half of the
-- promise could be marked done but never evidenced.
--
-- Free text on purpose. A hosting account is whatever the provider calls it —
-- an account number, a username, a customer reference — and inventing a shape
-- for it would be guessing at providers we have not used yet.

ALTER TABLE sites ADD COLUMN hosting_ref TEXT;
