-- Market series (ADR 0006): creator-owned definitions that spawn bracket
-- instances on a recurrence rule. A one-time market is a series with exactly
-- one instance. The platform itself may own a series (NULL creator), like
-- the rolling demo bus.
CREATE TABLE market_series (
    id TEXT PRIMARY KEY,
    creator_account_id TEXT REFERENCES accounts(id),
    title TEXT NOT NULL,
    resolution_criterion TEXT NOT NULL,
    rule JSONB NOT NULL,
    source_id TEXT NOT NULL,
    data_mode TEXT NOT NULL CHECK (data_mode IN ('simulated','manual')),
    liquidity_units BIGINT NOT NULL CHECK (liquidity_units BETWEEN 10 AND 100000),
    -- Fee-free markets (welfare series like bus timing) collect no fee and
    -- pay no creator share; the flag is fixed at creation.
    fee_charged BOOLEAN NOT NULL DEFAULT TRUE,
    recurrence TEXT NOT NULL CHECK (recurrence IN ('once','recurring')),
    interval_ms BIGINT CHECK (interval_ms BETWEEN 60000 AND 86400000),
    active_start_minute INTEGER CHECK (active_start_minute BETWEEN 0 AND 1438),
    active_end_minute INTEGER CHECK (active_end_minute BETWEEN 1 AND 1439),
    max_concurrency BIGINT NOT NULL DEFAULT 1 CHECK (max_concurrency BETWEEN 1 AND 50),
    end_ms BIGINT,
    anchor_ms BIGINT NOT NULL,
    state TEXT NOT NULL DEFAULT 'active' CHECK (state IN ('active','ended')),
    created_ms BIGINT NOT NULL,
    CHECK ((recurrence = 'once') = (interval_ms IS NULL)),
    CHECK (recurrence = 'once' OR (active_start_minute IS NOT NULL
        AND active_end_minute IS NOT NULL AND active_start_minute < active_end_minute
        AND max_concurrency >= 1))
);
CREATE INDEX market_series_active ON market_series(state, created_ms);

ALTER TABLE instances ADD COLUMN series_id TEXT REFERENCES market_series(id);
ALTER TABLE instances ADD COLUMN bracket_start_ms BIGINT;
ALTER TABLE instances ADD COLUMN fee_charged BOOLEAN NOT NULL DEFAULT TRUE;
CREATE INDEX instances_series ON instances(series_id, close_ms);

-- Published series definitions are immutable; only the lifecycle state moves.
CREATE FUNCTION protect_series() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF ROW(NEW.creator_account_id,NEW.title,NEW.resolution_criterion,NEW.rule,NEW.source_id,
           NEW.data_mode,NEW.liquidity_units,NEW.fee_charged,NEW.recurrence,NEW.interval_ms,NEW.active_start_minute,
           NEW.active_end_minute,NEW.max_concurrency,NEW.end_ms,NEW.anchor_ms)
       IS DISTINCT FROM
       ROW(OLD.creator_account_id,OLD.title,OLD.resolution_criterion,OLD.rule,OLD.source_id,
           OLD.data_mode,OLD.liquidity_units,OLD.fee_charged,OLD.recurrence,OLD.interval_ms,OLD.active_start_minute,
           OLD.active_end_minute,OLD.max_concurrency,OLD.end_ms,OLD.anchor_ms) THEN
        RAISE EXCEPTION 'Published series definitions are immutable';
    END IF;
    IF NEW.state NOT IN ('active','ended') OR (OLD.state = 'ended' AND NEW.state <> 'ended') THEN
        RAISE EXCEPTION 'Invalid series lifecycle transition';
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER protect_series BEFORE UPDATE ON market_series
    FOR EACH ROW EXECUTE FUNCTION protect_series();

-- Bracket membership and the fee policy are part of the published definition.
CREATE OR REPLACE FUNCTION protect_instance() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF ROW(NEW.template_id,NEW.category,NEW.title,NEW.resolution_criterion,NEW.rule,NEW.outcomes,
           NEW.source_id,NEW.data_mode,NEW.close_ms,NEW.observation_start_ms,NEW.observation_end_ms,
           NEW.finalize_after_ms,NEW.evidence_deadline_ms,NEW.liquidity_units,NEW.reserve_account_id,NEW.fee_charged,NEW.creator_account_id,NEW.series_id,NEW.bracket_start_ms)
       IS DISTINCT FROM
       ROW(OLD.template_id,OLD.category,OLD.title,OLD.resolution_criterion,OLD.rule,OLD.outcomes,
           OLD.source_id,OLD.data_mode,OLD.close_ms,OLD.observation_start_ms,OLD.observation_end_ms,
           OLD.finalize_after_ms,OLD.evidence_deadline_ms,OLD.liquidity_units,OLD.reserve_account_id,OLD.fee_charged,OLD.creator_account_id,OLD.series_id,OLD.bracket_start_ms) THEN
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
