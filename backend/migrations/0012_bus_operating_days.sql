-- Recurring series can restrict which days of the week spawn brackets,
-- as ISO day numbers (1 = Monday to 7 = Sunday); the default runs every
-- day. The campus bus demo needs this: Green runs weekdays only, Brown
-- weekends only, Blue and Red daily (NTU's internal campus shuttle page).
ALTER TABLE market_series ADD COLUMN active_days JSONB NOT NULL DEFAULT '[1,2,3,4,5,6,7]'::jsonb
    CHECK (jsonb_typeof(active_days) = 'array' AND active_days <> '[]'::jsonb);

-- The operating days are part of the immutable published definition.
CREATE OR REPLACE FUNCTION protect_series() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF ROW(NEW.creator_account_id,NEW.title,NEW.resolution_criterion,NEW.rule,NEW.source_id,
           NEW.data_mode,NEW.liquidity_units,NEW.fee_charged,NEW.recurrence,NEW.interval_ms,NEW.active_start_minute,
           NEW.active_end_minute,NEW.active_days,NEW.max_concurrency,NEW.end_ms,NEW.anchor_ms,
           NEW.resolution_authority,NEW.resolution_public_key,NEW.resolver_endpoint)
       IS DISTINCT FROM
       ROW(OLD.creator_account_id,OLD.title,OLD.resolution_criterion,OLD.rule,OLD.source_id,
           OLD.data_mode,OLD.liquidity_units,OLD.fee_charged,OLD.recurrence,OLD.interval_ms,OLD.active_start_minute,
           OLD.active_end_minute,OLD.active_days,OLD.max_concurrency,OLD.end_ms,OLD.anchor_ms,
           OLD.resolution_authority,OLD.resolution_public_key,OLD.resolver_endpoint) THEN
        RAISE EXCEPTION 'Published series definitions are immutable';
    END IF;
    IF NEW.state NOT IN ('active','ended') OR (OLD.state = 'ended' AND NEW.state <> 'ended') THEN
        RAISE EXCEPTION 'Invalid series lifecycle transition';
    END IF;
    RETURN NEW;
END;
$$;
