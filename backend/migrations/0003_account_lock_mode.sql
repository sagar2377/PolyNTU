-- Foreign-key checks on idempotency/positions hold KEY SHARE locks on accounts.
-- Balance-only updates must not upgrade those to FOR UPDATE: concurrent trades
-- by one account on different markets would otherwise deadlock during upgrade.
CREATE OR REPLACE FUNCTION apply_transfer() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    PERFORM id FROM accounts WHERE id IN (NEW.from_account, NEW.to_account) ORDER BY id FOR NO KEY UPDATE;
    UPDATE accounts SET balance_micros = balance_micros - NEW.amount_micros WHERE id = NEW.from_account;
    UPDATE accounts SET balance_micros = balance_micros + NEW.amount_micros WHERE id = NEW.to_account;
    RETURN NEW;
END;
$$;
