CREATE FUNCTION protect_instance() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF ROW(NEW.template_id,NEW.category,NEW.title,NEW.resolution_criterion,NEW.rule,NEW.outcomes,
           NEW.source_id,NEW.data_mode,NEW.close_ms,NEW.observation_start_ms,NEW.observation_end_ms,
           NEW.finalize_after_ms,NEW.evidence_deadline_ms,NEW.liquidity_units,NEW.reserve_account_id)
       IS DISTINCT FROM
       ROW(OLD.template_id,OLD.category,OLD.title,OLD.resolution_criterion,OLD.rule,OLD.outcomes,
           OLD.source_id,OLD.data_mode,OLD.close_ms,OLD.observation_start_ms,OLD.observation_end_ms,
           OLD.finalize_after_ms,OLD.evidence_deadline_ms,OLD.liquidity_units,OLD.reserve_account_id) THEN
        RAISE EXCEPTION 'Published market rules are immutable';
    END IF;
    IF NEW.version <> OLD.version + 1 THEN RAISE EXCEPTION 'Market version must advance exactly once'; END IF;
    IF OLD.state IN ('resolved','voided') THEN RAISE EXCEPTION 'Finalized markets are immutable'; END IF;
    IF OLD.result IS NOT NULL AND NEW.result IS DISTINCT FROM OLD.result THEN RAISE EXCEPTION 'Final outcome is immutable'; END IF;
    IF OLD.state <> 'open' AND NEW.inventory IS DISTINCT FROM OLD.inventory THEN RAISE EXCEPTION 'Trading is closed'; END IF;
    IF NEW.state <> OLD.state AND NOT (
        (OLD.state='open' AND NEW.state='closed') OR
        (OLD.state='closed' AND NEW.state='resolving' AND NEW.result IS NOT NULL) OR
        (OLD.state='resolving' AND NEW.state IN ('resolved','voided'))
    ) THEN RAISE EXCEPTION 'Invalid lifecycle transition'; END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER protect_instance BEFORE UPDATE ON instances FOR EACH ROW EXECUTE FUNCTION protect_instance();

ALTER TABLE ledger_transfers DROP CONSTRAINT ledger_transfers_kind_check;
ALTER TABLE ledger_transfers ADD CHECK (kind IN ('issuance','grant','subsidy','buy','sell','resolution','release'));
