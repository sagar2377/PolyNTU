# ADR 0005: Register accounts with NTU email and creator roles

Status: **proposed (planned, not yet implemented)**

## Context

An account today is just a display name (2 to 60 characters) plus a one-shot random bearer token created through the API or the demo route; only the token's SHA-256 hash is stored. There is no email, no login, and no roles. The administrator is a single shared token that creates every market instance, records all evidence, suspends, grants, and reconciles.

The product plan requires signup and login restricted to NTU affiliates, a welcome gift of 10,000 units (today's grant is 1,000 units), and a member-to-creator verification workflow, because markets will be created by external parties.

## Decision

Registration will take a display name, an NTU email, and a password; login will issue a session token under the existing bearer model.

- The email must match `^[^@\s]+@([a-z0-9-]+\.)*ntu\.edu\.sg$` case-insensitively (so `billy@ntu.edu.sg` and `billy@scse.ntu.edu.sg` pass) and must be unique. Passwords will be stored as argon2id hashes with a 12-character minimum.
- Login will verify the password and issue a bearer session token: plaintext returned once, only the SHA-256 hash at rest.
- The welcome gift will be 10,000 units (10,000,000,000 micros) from the treasury, and a migration will raise total issuance from 1M to 1B units so the budget is not exhausted after 100 accounts.
- Roles: member and creator. A member will file a verification request; the administrator will approve or reject it with a recorded reason; approval is permanent, and a creator can create unlimited markets without re-verifying.
- The administrator remains a single shared token, unchanged in scope for now.
- Login failures will return one generic message for unknown email and wrong password alike, so accounts cannot be enumerated.

## Alternatives considered

- **Keep the token-only model**: no login and no affiliation check, and a lost token locks the account out with no recovery.
- **Campus SSO**: deferred; no provider access in an academic project.
- **Emailed magic links**: no mail infrastructure to send them.

## Consequences and limits

- Real password reset and email-based recovery remain deferred.
- The demo one-click account stays for development and CI.
- argon2 adds registration cost, which is negligible at this scale.
- The 1B issuance is still finite, and reconciliation continues to check the net of all balances.

## Evidence

Planned artifacts (none exist yet):

- a new auth module for password hashing and login
- a migration adding email, password hash, role, and the issuance change
- the verification request table and its admin route

Current code this decision will touch:

- `backend/src/auth.rs`
- `backend/src/store.rs` (`create_account`)
- `backend/migrations/`
