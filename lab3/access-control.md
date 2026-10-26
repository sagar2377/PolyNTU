# PolyNTU access control - Lab 3

The authoritative operational description is the [security guide](../docs/developer/security-and-privacy.md). Routes and checks below are taken from [api.rs](../backend/src/api.rs), [auth.rs](../backend/src/auth.rs), [store.rs](../backend/src/store.rs), and [execution.rs](../backend/src/execution.rs); the design view follows the [class responsibilities](../docs/developer/class-diagram.md) and the [architecture write-up](architecture.md).

## Actors and roles

| Actor | Description | Identity establishment |
|---|---|---|
| Visitor | Unauthenticated browser session: browses public market data, registers, or logs in. | None; public routes only. |
| Member | Registered account holding the `member` role: trades, holds positions, files one creator verification request. | Unique lowercase NTU email plus password; a bearer session token after registration or login. |
| Creator | Member permanently approved through verification; publishes series and signs resolutions for creator-authority markets. | Bearer session; the role lives on the account row. Creator-authority series additionally fix the creator's ed25519 public key at creation. |
| Administrator | Provisions accounts, creates instances directly, submits evidence, suspends trading, decides verification, advances the demo clock, runs the worker, and reconciles. | The shared `x-admin-token`, or the bearer session of an account holding the `admin` role. |
| System accounts | `issuance`, `treasury`, and one `reserve:{instance_id}` account per instance; they hold balances and never act. | Fixed identifiers created by bootstrap and instance creation; no token, password, or role, and excluded from session lookups by the `kind='user'` filter. |
| External resolver / provider | Answers the fixed resolver request for resolver-authority series; the platform's own NTU bus adapter and the Python provider behind it are the current examples. | The endpoint URL fixed at series creation; no credential is exchanged, so trust is placed in the endpoint for its own series. |

Unregistered demo and administrator-provisioned accounts are `kind='user'` with no email or role. They share the member row for browsing, trading, and portfolio, but cannot file verification requests or publish series. Demo-mode databases additionally seed one admin-role account (`admin@ntu.edu.sg`, password `admin`) and one creator (`bus@ntu.edu.sg`); the seed tokens' plaintexts are discarded, so the passwords are the only way in.

## Authentication mechanisms

| Mechanism | Credential | What is stored | Lifetime and rotation |
|---|---|---|---|
| Demo account creation (`POST /api/v2/auth/demo`) | None; available only when demo mode is on. | SHA-256 hash of a fresh 256-bit OS-RNG token; the plaintext is returned once. | No expiry. No rotation path: demo accounts cannot log in, so the token never rotates. |
| Registration (`POST /api/v2/auth/register`) | Display name, unique lowercase NTU email, password of at least 12 characters. | argon2id PHC password hash, `member` role, session-token hash. | Token returned once; no expiry. Rotates on the first login. |
| Login (`POST /api/v2/auth/login`) | Email and password. | Replaces the account's single token hash. | Every login rotates the token; the previous token stops resolving immediately, in-process caches included. Unknown email and wrong password return the same 401, and unknown emails still cost one argon2 verification. |
| Bearer session (`Authorization: Bearer`) | 256-bit base64url token. | SHA-256 hash at rest; an in-process cache maps token hash to identity only (no roles; balances read fresh). | No expiry and no scopes; possession grants full account access. |
| Shared admin token (`x-admin-token`) | Configured secret of at least 32 characters, distinct from the quote secret. | Nothing per request; the configured value lives in environment or workspace secrets. | No expiry or rotation. Comparison computes HMAC-SHA256 tags over the fixed message `polyntu-admin` on both sides, so the tag comparison is constant-time. Carries no actor identity. |
| Admin-role session | Bearer session of an account with `role='admin'`. | The role on the account row. | The role is read fresh from the database on every admin request, so a role change takes effect immediately. |
| Creator resolution key | Account password, used only in the browser. | Only the derived public key is published with the series; the private key stays in the browser, cached in local storage under `polyntu.v2.signing-key`. | The browser derives the ed25519 keypair by PBKDF2-HMAC-SHA256 from the password (600,000 iterations, salt `polyntu.resolution.v1:{email}`, 32-byte seed). No password change or recovery path exists; losing the password loses the key and affected markets void at their deadlines. |
| Signed trade quote | Quote token returned by `POST /api/v2/quotes`. | Nothing; verified by recomputation. | HMAC-SHA256 over the base64url-encoded claim; `expires_ms` is `min(now + 15 s, close_ms)`. Replayable until one trade succeeds or state, version, or expiry rejects it; the signing secret must stay stable across restarts. |

## Access control matrix

Values follow the route handlers in [api.rs](../backend/src/api.rs). "Denied" means no credential grants the capability; mode gates are noted separately.

| Capability | Visitor | Member | Creator | Administrator |
|---|---|---|---|---|
| Browse markets, instances, series, history, events (SSE); health and config | allowed | allowed | allowed | allowed |
| Register and login | allowed | allowed | allowed | allowed |
| Demo account creation | allowed (demo mode only) | allowed (demo mode only) | allowed (demo mode only) | allowed (demo mode only) |
| `me`, portfolio, trades | denied | own account only | own account only | own account only (session) |
| Quote and trade | denied | allowed | allowed, not own markets | allowed, not own markets |
| Publish a series (`POST /series`) | denied | denied | allowed | denied |
| Submit a creator resolution (`POST /instances/{id}/resolution`) | denied | denied | own creator-authority markets only | denied |
| File a verification request | denied | allowed (one pending at a time) | denied | denied |
| Read own verification status | denied | own account only | own account only | own account only (null unless one exists) |
| Verification review and decision | denied | denied | denied | allowed |
| Account provisioning (`POST /admin/accounts`) | denied | denied | denied | allowed (either mode) |
| Direct instance creation (`POST /admin/instances`) | denied | denied | denied | allowed |
| Evidence submission | denied | denied | denied | allowed, except creator- or resolver-authority series |
| Suspension | denied | denied | denied | allowed (any open instance) |
| Demo clock advance | denied | denied | denied | allowed (demo mode only) |
| Reconcile | denied | denied | denied | allowed (read-only) |
| Worker tick | denied | denied | denied | allowed |
| NTU bus resolver adapter (`POST /resolvers/ntu-bus`) | allowed | allowed | allowed | allowed |

- The administrator column covers both the shared token and an admin-role session; account-scoped rows apply only to the session form, because the shared token resolves to no account.
- Publishing requires exactly the `creator` role, decided in `Store::create_series`; admin-role accounts are rejected as well. Platform-owned series (no creator) exist only through internal seeding paths such as the demo worker, which publishes its bus series under the seeded creator account.
- The creator's one-time instance is created server-side by series publication; creators never call the direct instance route.
- The self-trading prohibition applies to whichever account the instance records as its creator, of any role, including one named by the administrator on a directly created instance; it is rejected on both the quote and execution paths. Trading additionally requires an `Idempotency-Key` header.
- Instance detail responses include the selected normalized evidence payload; the evidence-privacy rule in the security guide applies.
- The bus adapter route is unauthenticated, but its answer is pending until a bracket's observation window has ended, and only the settlement worker records evidence from it.
- Retired option routes answer 410 for every caller.

## Enforcement points

Every request passes the same chain before any domain logic runs:

```text
request
  -> router layers: HTTP trace, configured CORS, 64 KiB body limit
  -> handler credential extraction: AppState::account / AppState::require_admin
  -> Store-level role, ownership, and state checks inside transactions
  -> PostgreSQL constraints and triggers
```

CORS and the body limit are not authentication: CORS constrains browsers only, and non-browser clients are unaffected. The layer that makes the authoritative decision for each control:

| Control | Authoritative layer | Detail |
|---|---|---|
| Session identity | Handler `AppState::account` with `Store::auth_account_for_token` | SHA-256 hash lookup restricted to `kind='user'`; token-to-account mapping is immutable, so the read-through cache cannot authorize a stale identity. |
| Administrator routes | Handler `AppState::require_admin` | Shared token compared through fixed-message HMAC tags, or a bearer session whose role is read fresh from the database on every request. |
| Creator publication | `Store::create_series` | Role read under `FOR NO KEY UPDATE` row lock inside the publication transaction; the handler requires only a valid session. |
| Creator resolution | `Store::record_creator_resolution` | Series-creator ownership, `creator` authority, closed state, observation and deadline windows, one resolution per instance, and ed25519 verification, all inside one transaction. |
| Administrator evidence exclusion | `Store::record_evidence` | Rejects instances of a series whose authority is not `admin`; directly created instances without a series remain administrator-resolvable. |
| Verification workflow | `Store::create_verification_request` and `decide_verification_request` | Role gate under a row lock; approval updates only rows still holding `role='member'`. |
| Trading authorization | `Store::execute` | Re-binds the quote claim's account to the session account and rechecks state, version, expiry, balances, holdings, and self-trading under the instance lock. |
| Demo-mode gates | Handler (`/auth/demo`) and `Store::advance_demo_clock` | The settings row fixes demo mode at initialization; later mode changes are rejected. |
| Database invariants | PostgreSQL constraints and triggers | Role value checks, unique lowercase NTU email, one pending verification request per account, immutable published definitions including the authority fields, append-only evidence and audit rows, and unique claims and idempotency keys. |

## Resolution authority as access control

A series fixes its resolution authority at creation ([ADR 0007](../docs/decisions/0007-resolution-authority.md)): `admin` (the default, covering platform-owned and directly created instances), `creator` (with the creator's ed25519 public key), or `resolver` (with the endpoint). Migration 0010 checks that creator authority carries a public key on a creator-owned series and that resolver authority carries an endpoint, and the `protect_series` trigger makes all three fields part of the immutable published definition.

- A human resolution must carry an ed25519 signature over the exact string `polyntu.resolution.v1:{instance_id}:{outcome_id}:{nonce}`, checked against the public key fixed at creation; malformed keys and signatures fail closed (`verify_ed25519` in [auth.rs](../backend/src/auth.rs)).
- The administrator evidence route rejects instances of series whose authority is not `admin`, so a compromised administrator credential cannot resolve a creator-signed or resolver-settled market. The exclusion is an application-path control; a database superuser can still drop constraints and insert evidence directly.
- The platform stores only public keys and performs no key derivation server-side, so it can verify a creator's resolution but never forge one.
- Suspension, clock advance, and worker operations remain administrator capabilities regardless of authority; only evidence and resolution are authority-scoped.

## Signed quotes as request integrity

`auth::sign` computes HMAC-SHA256 over the base64url-encoded JSON claim; `auth::verify` recomputes the tag with a constant-time comparison before decoding, and rejects tokens above 4096 characters.

- The claim binds account, instance, outcome, side, quantity, amount, fee, market version, expiry (`min(now + 15 s, close_ms)`), and engine version.
- This prevents altering signed terms, using another account's quote, or executing against a different engine version.
- It does not protect confidentiality: the body is decodable base64url and must contain no secret beyond its signature.
- Quotes are replayable until one succeeds or state, version, or expiry rejects them; the account-scoped idempotency key ensures an intended trade commits once.

## Mapping to design classes

| Design element | Access-control responsibility | Implementation |
|---|---|---|
| `Account.role`, `hasRole` | Distinguishes member, creator, and admin among `kind='user'` accounts; role and registered credentials are absent for unregistered and system accounts. | `accounts.role` with database CHECK values (migrations 0006 and 0008). |
| `AccountControl.authenticate` | Registration validation, password verification, and single-token session rotation. | `register_account` and `login` in [store.rs](../backend/src/store.rs) with [auth.rs](../backend/src/auth.rs) helpers. |
| `AdminControl.authorize` | Gates every privileged route before coordination. | `AppState::require_admin` and the admin handlers in [api.rs](../backend/src/api.rs). |
| `AdminApi` | Boundary that receives authorized administrator requests. | `/api/v2/admin/*` routes in [api.rs](../backend/src/api.rs). |
| `ResolutionAuthority` strategies | `AdminAuthority`, `CreatorSignedAuthority`, and `ExternalResolverAuthority` accept evidence differently under one lifecycle. | Design level: `AuthorityKind` and `match` dispatch over `record_evidence`, `record_creator_resolution`, and `record_resolver_evidence`. |
| `TradingControl` | Binds quotes to accounts and enforces the self-trading prohibition. | `quote` and `execute` in [execution.rs](../backend/src/execution.rs). |
| `StartupControl` | Validates distinct quote and admin secrets of at least 32 characters and rejects a public demo-mode bind. | `AppState::new` in [api.rs](../backend/src/api.rs); startup in [main.rs](../backend/src/main.rs). |

## Known limitations

- Bearer tokens carry no expiry or scopes; possession grants full account access.
- No server-side revocation or logout exists: sign-out clears browser storage only, a fresh login rotates the token, and unused tokens never expire.
- The shared admin token carries no actor identity, expiry, rotation, or multi-party approval; audit rows cannot identify which human used it.
- No rate limiting, lockout, or denial-of-service mitigation exists; the login timing equalization is not a rate limit.
- The resolution key derives from the account password, so the published public key permits offline password guessing at one 600,000-iteration PBKDF2 run per guess, and the cached private key is readable by same-origin scripts.
- Application processes use one database URL rather than separate least-privilege migration and runtime roles.
- Browser local storage holds the bearer token, one pending trade, and the cached signing key; any script executing in the same origin can read them.
- No campus SSO, MFA, or password recovery exists.

## References

- [Security and privacy](../docs/developer/security-and-privacy.md): authoritative control description.
- [Architecture write-up](architecture.md): sibling Lab 3 contribution.
- [Class responsibilities](../docs/developer/class-diagram.md): control classes and the ResolutionAuthority strategies.
- [API implementation map](../docs/developer/api-reference.md): router layers and authentication flow.
- [ADR 0005](../docs/decisions/0005-ntu-accounts-and-creator-roles.md): NTU accounts and creator roles.
- [ADR 0007](../docs/decisions/0007-resolution-authority.md): resolution authority.
- [api.rs](../backend/src/api.rs), [auth.rs](../backend/src/auth.rs), [store.rs](../backend/src/store.rs), [execution.rs](../backend/src/execution.rs): route, credential, and check implementation.
