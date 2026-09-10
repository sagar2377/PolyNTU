CREATE TABLE settings (
    singleton BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (singleton),
    clock_offset_ms BIGINT NOT NULL DEFAULT 0 CHECK (clock_offset_ms >= 0),
    demo_mode BOOLEAN NOT NULL DEFAULT FALSE
);
INSERT INTO settings(singleton) VALUES (TRUE);

CREATE TABLE accounts (
    id TEXT PRIMARY KEY,
    display_name TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('issuance','treasury','user','reserve')),
    token_hash TEXT UNIQUE,
    balance_micros BIGINT NOT NULL DEFAULT 0,
    created_ms BIGINT NOT NULL DEFAULT (extract(epoch FROM clock_timestamp()) * 1000)::BIGINT,
    CHECK (kind = 'issuance' OR balance_micros >= 0),
    CHECK ((kind = 'user') = (token_hash IS NOT NULL))
);

-- Each transfer is a balanced pair of entries by construction.
CREATE TABLE ledger_transfers (
    id TEXT PRIMARY KEY,
    from_account TEXT NOT NULL REFERENCES accounts(id),
    to_account TEXT NOT NULL REFERENCES accounts(id),
    amount_micros BIGINT NOT NULL CHECK (amount_micros >= 0),
    kind TEXT NOT NULL CHECK (kind IN ('issuance','grant','subsidy','buy','sell','resolution')),
    reference TEXT NOT NULL UNIQUE,
    created_ms BIGINT NOT NULL,
    CHECK (from_account <> to_account)
);
CREATE INDEX ledger_from ON ledger_transfers(from_account, created_ms);
CREATE INDEX ledger_to ON ledger_transfers(to_account, created_ms);

CREATE FUNCTION apply_transfer() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    PERFORM id FROM accounts WHERE id IN (NEW.from_account, NEW.to_account) ORDER BY id FOR UPDATE;
    UPDATE accounts SET balance_micros = balance_micros - NEW.amount_micros WHERE id = NEW.from_account;
    UPDATE accounts SET balance_micros = balance_micros + NEW.amount_micros WHERE id = NEW.to_account;
    RETURN NEW;
END;
$$;
CREATE TRIGGER apply_transfer AFTER INSERT ON ledger_transfers FOR EACH ROW EXECUTE FUNCTION apply_transfer();

CREATE FUNCTION immutable_record() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN RAISE EXCEPTION 'Audit records are append-only'; END;
$$;
CREATE TRIGGER immutable_ledger BEFORE UPDATE OR DELETE ON ledger_transfers FOR EACH ROW EXECUTE FUNCTION immutable_record();
CREATE VIEW ledger_entries AS
    SELECT id AS transaction_id, from_account AS account_id, -amount_micros AS delta_micros, kind, reference, created_ms FROM ledger_transfers
    UNION ALL
    SELECT id, to_account, amount_micros, kind, reference, created_ms FROM ledger_transfers;

CREATE TABLE templates (
    id TEXT PRIMARY KEY,
    category TEXT NOT NULL CHECK (category IN ('weather','bus','elections','queue_crowd','attendance')),
    title TEXT NOT NULL
);
CREATE TABLE instances (
    id TEXT PRIMARY KEY,
    template_id TEXT NOT NULL REFERENCES templates(id),
    category TEXT NOT NULL,
    title TEXT NOT NULL,
    resolution_criterion TEXT NOT NULL,
    rule JSONB NOT NULL,
    outcomes JSONB NOT NULL,
    source_id TEXT NOT NULL,
    data_mode TEXT NOT NULL CHECK (data_mode IN ('simulated','manual')),
    state TEXT NOT NULL DEFAULT 'open' CHECK (state IN ('open','closed','resolving','resolved','voided')),
    suspended BOOLEAN NOT NULL DEFAULT FALSE,
    close_ms BIGINT NOT NULL,
    observation_start_ms BIGINT NOT NULL,
    observation_end_ms BIGINT NOT NULL,
    finalize_after_ms BIGINT NOT NULL,
    evidence_deadline_ms BIGINT NOT NULL,
    liquidity_units BIGINT NOT NULL CHECK (liquidity_units BETWEEN 10 AND 100000),
    inventory BIGINT[] NOT NULL,
    version BIGINT NOT NULL DEFAULT 0,
    reserve_account_id TEXT NOT NULL UNIQUE REFERENCES accounts(id),
    result JSONB,
    evidence_id TEXT,
    CHECK (cardinality(inventory) BETWEEN 2 AND 8),
    CHECK (0 <= ALL(inventory)),
    CHECK (1000000000 >= ALL(inventory)),
    CHECK (jsonb_array_length(outcomes) = cardinality(inventory)),
    CHECK (close_ms <= observation_start_ms AND observation_start_ms < observation_end_ms
        AND observation_end_ms <= finalize_after_ms AND finalize_after_ms < evidence_deadline_ms),
    UNIQUE(template_id, close_ms)
);
CREATE INDEX instances_browse ON instances(template_id, close_ms DESC);
CREATE INDEX instances_due ON instances(state, close_ms, finalize_after_ms, evidence_deadline_ms)
    WHERE state IN ('open','closed','resolving');

CREATE TABLE positions (
    account_id TEXT NOT NULL REFERENCES accounts(id),
    instance_id TEXT NOT NULL REFERENCES instances(id),
    outcome_index INTEGER NOT NULL CHECK (outcome_index BETWEEN 0 AND 7),
    quantity_millis BIGINT NOT NULL CHECK (quantity_millis BETWEEN 0 AND 1000000000),
    PRIMARY KEY(account_id, instance_id, outcome_index)
);
CREATE INDEX positions_instance ON positions(instance_id, account_id);
CREATE TABLE idempotency (
    account_id TEXT NOT NULL REFERENCES accounts(id),
    key TEXT NOT NULL,
    request_hash TEXT NOT NULL,
    response JSONB,
    PRIMARY KEY(account_id, key)
);
CREATE TABLE trades (
    id TEXT PRIMARY KEY,
    account_id TEXT NOT NULL REFERENCES accounts(id),
    instance_id TEXT NOT NULL REFERENCES instances(id),
    outcome_index INTEGER NOT NULL,
    side TEXT NOT NULL CHECK (side IN ('buy','sell')),
    quantity_millis BIGINT NOT NULL CHECK (quantity_millis > 0),
    amount_micros BIGINT NOT NULL CHECK (amount_micros >= 0),
    instance_version BIGINT NOT NULL,
    engine_version TEXT NOT NULL,
    created_ms BIGINT NOT NULL,
    UNIQUE(instance_id, instance_version)
);
CREATE INDEX trades_account ON trades(account_id, created_ms DESC, id);
CREATE TRIGGER immutable_trades BEFORE UPDATE OR DELETE ON trades FOR EACH ROW EXECUTE FUNCTION immutable_record();

CREATE TABLE evidence (
    id TEXT PRIMARY KEY,
    instance_id TEXT NOT NULL REFERENCES instances(id),
    source_id TEXT NOT NULL,
    event_id TEXT NOT NULL,
    source_revision BIGINT NOT NULL CHECK (source_revision >= 0),
    received_ms BIGINT NOT NULL,
    parser_version TEXT NOT NULL,
    payload JSONB NOT NULL,
    payload_hash TEXT NOT NULL,
    evaluated_result JSONB,
    UNIQUE(instance_id, source_id, event_id),
    UNIQUE(instance_id, source_id, source_revision)
);
ALTER TABLE instances ADD FOREIGN KEY(evidence_id) REFERENCES evidence(id);
CREATE INDEX evidence_instance ON evidence(instance_id, received_ms);
CREATE TRIGGER immutable_evidence BEFORE UPDATE OR DELETE ON evidence FOR EACH ROW EXECUTE FUNCTION immutable_record();

CREATE TABLE settlement_claims (
    instance_id TEXT NOT NULL REFERENCES instances(id),
    account_id TEXT NOT NULL REFERENCES accounts(id),
    credit_micros BIGINT NOT NULL CHECK (credit_micros >= 0),
    created_ms BIGINT NOT NULL,
    PRIMARY KEY(instance_id, account_id)
);
CREATE TRIGGER immutable_claims BEFORE UPDATE OR DELETE ON settlement_claims FOR EACH ROW EXECUTE FUNCTION immutable_record();
CREATE TABLE outbox (
    id BIGSERIAL PRIMARY KEY,
    instance_id TEXT NOT NULL REFERENCES instances(id),
    instance_version BIGINT NOT NULL,
    event_type TEXT NOT NULL,
    created_ms BIGINT NOT NULL
);
CREATE INDEX outbox_instance ON outbox(instance_id, id);
CREATE TABLE admin_audit (
    id BIGSERIAL PRIMARY KEY,
    action TEXT NOT NULL,
    instance_id TEXT,
    detail JSONB NOT NULL,
    created_ms BIGINT NOT NULL
);
CREATE TRIGGER immutable_admin_audit BEFORE UPDATE OR DELETE ON admin_audit FOR EACH ROW EXECUTE FUNCTION immutable_record();
