# ADR 0005: Register accounts with NTU email and creator roles

Status: **accepted and implemented**

## Context

An account today is just a display name (2 to 60 characters) plus a one-shot random bearer token created through the API or the demo route; only the token's SHA-256 hash is stored. There is no email, no login, and no roles. The administrator is a single shared token that creates every market instance, records all evidence, suspends, grants, and reconciles.

The product plan requires signup and login restricted to NTU affiliates, a welcome gift of 10,000 units (today's grant is 1,000 units), and a member-to-creator verification workflow, because markets will be created by external parties.

## Decision

Registration takes a display name, an NTU email, and a password; login issues a session token under the existing bearer model.

- The email must match `^[^@\s]+@([a-z0-9-]+\.)*ntu\.edu\.sg$` case-insensitively (so `billy@ntu.edu.sg` and `billy@scse.ntu.edu.sg` pass), is normalized to lowercase, and must be unique. Passwords are stored as argon2id hashes with a 12-character minimum.
- Login verifies the password and rotates the account's single bearer session token: plaintext returned once, only the SHA-256 hash at rest, and every previous token invalidated.
- The welcome gift is 10,000 units (10,000,000,000 micros) from the treasury for registered accounts, and total issuance rises from 1M to 1B units so the budget is not exhausted after 100 accounts.
- Roles: member, creator, and admin. A member files a verification request; the administrator approves or rejects it with a recorded reason; approval is permanent, a rejected member may re-apply, and a creator can create unlimited markets without re-verifying.
- The administrator keeps the shared token, unchanged in scope. Admin routes additionally accept the bearer session of an admin-role account, and demo-mode databases seed exactly one such administrator (`admin@ntu.edu.sg`, password `admin`).
- Login failures return one generic message for unknown email and wrong password alike, so accounts cannot be enumerated.

## Alternatives considered

- **Keep the token-only model**: no login and no affiliation check, and a lost token locks the account out with no recovery.
- **Campus SSO**: deferred; no provider access in an academic project.
- **Emailed magic links**: no mail infrastructure to send them.

## Consequences and limits

- Real password reset and email-based recovery remain deferred.
- The demo one-click account stays for development and CI.
- argon2 adds registration cost, which is negligible at this scale.
- An account holds at most one live session: logging in on a second device invalidates the first browser's token on its next request.
- The seeded demo administrator exists only in demo-mode databases and uses a documented weak password, so it must never reach a non-demo deployment.
- The 1B issuance is still finite, and reconciliation continues to check the net of all balances.

## Evidence

Implemented artifacts:

- `backend/migrations/0006_ntu_accounts.sql`: unique NTU email, argon2 password hash, and role columns with presence and lowercase-pattern checks.
- `backend/migrations/0007_creator_verification.sql`: the `verification_requests` table and its one-pending-per-account partial unique index.
- `backend/migrations/0008_admin_role.sql`: the admin role.
- `backend/src/auth.rs`: `valid_ntu_email`, `hash_password`, `verify_password`.
- `backend/src/store.rs`: `register_account`, `login` with token rotation and cache eviction, `LOGIN_TIMING_HASH`, the verification request/decision functions, `account_is_admin`, and the `issuance-expansion` bootstrap transfer.
- `backend/src/api.rs`: `/auth/register`, `/auth/login`, the member and administrator verification routes, and `require_admin` accepting an admin-role bearer session.
- `frontend/src/App.jsx` and `frontend/src/api.js`: the account entry panel (registration, login, demo accounts) and the creator verification panel.
- `backend/tests/integration.rs`: registration success/rejection/duplicate, HTTP register, login rotation with single-session invalidation, indistinguishable login failures, HTTP login, verification approval/rejection/reapplication, HTTP verification gating, and the seeded demo administrator.

Honest deviation from this record's original wording: the issuance raise is not "a migration". It is a second idempotent bootstrap transfer (`issuance-expansion`, appended by `Store::initialize`) because migrations run before the issuance account exists on a fresh database; the bootstrap transfer achieves the same effect idempotently. The admin role (migration 0008) and admin-route bearer sessions also extend the original "member and creator" roles and "single shared token" wording; the shared token itself is unchanged in scope.
