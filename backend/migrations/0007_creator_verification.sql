-- Creator verification workflow (ADR 0005): a member files one request, the
-- administrator approves or rejects it with a recorded reason, and approval
-- permanently grants the creator role on the account.
CREATE TABLE verification_requests (
    id TEXT PRIMARY KEY,
    account_id TEXT NOT NULL REFERENCES accounts(id),
    status TEXT NOT NULL CHECK (status IN ('pending','approved','rejected')),
    reason TEXT,
    created_ms BIGINT NOT NULL,
    decided_ms BIGINT
);
-- At most one pending request per account; re-application after a rejection
-- stays possible.
CREATE UNIQUE INDEX verification_requests_pending
    ON verification_requests(account_id) WHERE status = 'pending';
CREATE INDEX verification_requests_review
    ON verification_requests(status, created_ms);
