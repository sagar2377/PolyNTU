-- NTU accounts with email login and member/creator roles (ADR 0005).
-- Registered accounts carry a unique normalized NTU email, an argon2id
-- password hash, and a role. Demo and system accounts leave all three NULL,
-- so existing rows and the one-click demo flow are unaffected.
ALTER TABLE accounts
    ADD COLUMN email TEXT UNIQUE,
    ADD COLUMN password_hash TEXT,
    ADD COLUMN role TEXT CHECK (role IN ('member','creator')),
    ADD CHECK (email IS NULL OR (email = lower(email)
        AND email ~ '^[^@\s]+@([a-z0-9-]+\.)*ntu\.edu\.sg$')),
    ADD CHECK ((email IS NULL) = (password_hash IS NULL)),
    ADD CHECK ((email IS NULL) = (role IS NULL));
