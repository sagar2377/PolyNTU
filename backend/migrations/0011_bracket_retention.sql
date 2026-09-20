-- Bracket retention: recurring series accumulate hundreds of settled
-- brackets per day, so terminal brackets whose evidence deadline passed more
-- than 24 hours ago are purged with their whole subtree. One-time markets
-- are kept indefinitely. The purge runs only inside a transaction that sets
-- polyntu.purge = 'on'; every other update and delete of these append-only
-- records keeps raising.

CREATE OR REPLACE FUNCTION immutable_record() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'DELETE' AND coalesce(current_setting('polyntu.purge', true), '') = 'on' THEN
        RETURN OLD;
    END IF;
    RAISE EXCEPTION 'Audit records are append-only';
END;
$$;

-- instances.evidence_id and evidence.instance_id reference each other, so
-- the purge deletes both tables in one transaction. Deferring only this
-- constraint (immediate by default) makes that possible without changing any
-- other check.
ALTER TABLE instances ALTER CONSTRAINT instances_evidence_id_fkey
    DEFERRABLE INITIALLY IMMEDIATE;

CREATE INDEX instances_purge ON instances(evidence_deadline_ms)
    WHERE state IN ('resolved','voided');
