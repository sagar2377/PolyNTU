# ADR 0007: Resolution authority: signed creator resolution and contracted external resolvers

Status: **accepted and implemented**

## Context

Evidence and resolution today flow only through the administrator route, and the administrator can resolve anything ([ADR 0003](0003-evidence-resolution.md)). Once creators own market series ([ADR 0006](0006-market-series-and-recurrence.md)) this is wrong twice over: the platform must not be able to resolve, or forge resolution of, a creator's market, and obviously-resolvable markets like bus arrival should settle from an automated source without any human.

The plan therefore fixes a resolution authority per series at creation: the creator (human) or an external resolver endpoint (automatic).

## Decision

Every series will record a resolution authority fixed at creation: creator-human or external-resolver-automatic.

- Human authority: at series creation the creator's browser will generate an ed25519 keypair. The public key will be stored with the series and will join the immutable-definition protection; the private key never leaves the client. Every human resolution will carry a signature over (instance id, outcome, nonce), and the server will verify it against the stored public key before recording anything. The administrator evidence route will reject creator-owned series outright, so the administrator cannot resolve them even with full API access, and a valid signature cannot be produced by anyone holding only server credentials.
- Automatic authority: the series will store a resolver endpoint and a fixed request contract (instance identifier, bracket window, rule parameters). At the finalize window the settlement worker will call the endpoint; the response must name exactly one of the published options; retries will run with backoff through the finalize deadline. Unreachable, malformed, or out-of-options responses count as missing evidence, and the instance voids per the published policy ([ADR 0003](0003-evidence-resolution.md)).
- A lost private key makes human resolution impossible; the instance voids at the deadline. This is the accepted cost of excluding the platform from resolution.

## Alternatives considered

- **A bearer-token gate alone**: weaker; it stops only the API path, and direct database writes bypass it.
- **Server-held signing keys**: defeats the purpose; the server could forge a resolution.
- **A hash-chained resolution log**: tamper evidence without prevention; deferred.

## Consequences and limits

- Creators carry key custody in browser storage with no recovery path except the void policy.
- Signature verification adds one cheap check per human resolution.
- Automatic resolvers must implement the contract, so each real source (the bus timing API) needs a small adapter.
- The exclusion holds against a compromised application credential but not against a determined database superuser, who can always drop constraints.

## Evidence

Implemented artifacts:

- `backend/migrations/0010_resolution_authority.sql`: the `resolution_authority` column (`admin`, `creator`, or `resolver`, default `admin`), `resolution_public_key`, and `resolver_endpoint`; the checks that creator authority carries a public key on a creator-owned series and that resolver authority carries an endpoint; and the `protect_series` extension that makes all three part of the immutable published definition.
- `backend/src/market.rs`: `ResolutionSpec` (`creator` with a base64 32-byte ed25519 public key, or `resolver` with an https endpoint; plain http is accepted only on loopback, where local adapters run during development and tests), validated in `NewSeries::validate`.
- `backend/src/auth.rs`: `resolution_message`, the exact signed string `polyntu.resolution.v1:{instance_id}:{outcome_id}:{nonce}`, and `verify_ed25519`, which checks the signature with ed25519-dalek and fails closed on malformed keys or signatures.
- `backend/src/resolution.rs`: `record_creator_resolution` (only the series creator, on a closed market whose observation window has ended, before the deadline, one resolution per instance, recorded as evidence with source `creator-signature` and parser `creator-signature-v1`), `record_resolver_evidence` (a validated resolver answer recorded as evidence with source `external-resolver`), and the administrator exclusion inside `record_evidence`.
- `backend/src/resolver.rs`: the contract module: the fixed `ResolverRequest` (instance, series, bracket and observation window, rule), response parsing against the published options, and `call` with a 5-second timeout and a 64 KiB response cap.
- `backend/src/worker.rs`: the due query admits resolver-authority instances from their finalize window, and the call loop records valid answers while pending, malformed, and unreachable sources retry on every one-second tick until the published evidence deadline voids the instance.
- `backend/src/api.rs`: `POST /api/v2/instances/{id}/resolution`, the bearer-authenticated route a creator's signed resolution is submitted through.
- `frontend/src/api.js` with `frontend/src/pages/CreateMarket.jsx` and `frontend/src/pages/SeriesPage.jsx`: WebCrypto ed25519 key generation in the browser, publication of the public key only, the private key stored in local storage under `polyntu.v2.series-keys` keyed by series ID, and the per-bracket resolve control that signs and submits.
- `backend/tests/integration.rs`: authority fixed at creation with administrator evidence blocked, creator resolution with wrong-nonce, non-creator, administrator, and replay rejections plus settlement, resolver settlement from a live local test server, and resolver voiding on invalid and unreachable answers.

Honest notes extending this record's original wording:

- Retries run on every worker tick (one second) through the published evidence deadline, not with backoff through the finalize deadline as originally worded.
- The browser holds the private key in local storage with no recovery except the void policy: clearing browser storage or losing the profile loses the key, and the instance voids at its deadline.
- The administrator exclusion is an application-path control. It holds against compromised application credentials, but a database superuser can always drop constraints and insert evidence directly.

## Amendment (20 September 2026)

The creator's signing key is no longer a random browser-generated keypair. It is now derived from the creator's account password, so holding the password is holding the key and any browser where the creator signs in can resolve.

- Derivation contract: PBKDF2-HMAC-SHA256 with the account password as the password input, the UTF-8 salt `polyntu.resolution.v1:{email}` (the email lowercase as registered), 600,000 iterations, and a 32-byte output used as the ed25519 seed. `backend/src/auth.rs` (`resolution_key_seed`, `RESOLUTION_KEY_ITERATIONS`) and `frontend/src/api.js` (`deriveResolutionKeyPair`) implement the same contract, and the unit test `resolution_key_seed_matches_the_browser_derivation` pins the Rust and browser derivations to one known-answer vector (seed and public key), rejecting wrong passwords and wrong emails.
- The resulting public key is published with the series exactly as before. The server verification path is completely unchanged: ed25519 over `polyntu.resolution.v1:{instance_id}:{outcome_id}:{nonce}` against the public key fixed at creation, with the administrator exclusion untouched. The server never sees the password and performs no derivation.
- The per-series `polyntu.v2.series-keys` local storage key is replaced by a single per-account cache `polyntu.v2.signing-key`, filled after login and registration, so it is only a cache. The create form prompts for the password when the cache is missing; the series page prompts when the cache is missing or its public key does not match the series' published key, and verifies the derived public key before storing it. The password is used only in the browser.

Honest notes:

- The published public key now permits offline password guessing: each guess costs one PBKDF2 run. The 600,000 iterations and the 12-character minimum make guessing expensive, but the scheme is strictly weaker than a random key no password determines.
- Losing the password now loses the key, and no password recovery exists; affected markets void at their published deadlines.
- Series published before this amendment keep the public keys they were published with; those keys were browser-generated and no password derives them, so their custody remains as originally documented.

## Amendment (20 September 2026): the platform hosts the NTU Bus API adapter

The demo bus series now resolves through the external-resolver contract instead of administrator evidence, and the adapter the consequences section anticipated (the bus timing API needs a small adapter) is an endpoint the platform serves itself.

- `POST /api/v2/resolvers/ntu-bus` (`backend/src/resolver.rs`, `ntu_bus_answer`) accepts the fixed resolver request and answers simulated bus brackets from the deterministic feed once their observation window has ended, so it reveals nothing before the simulator itself would; anything else answers pending. The live NTU Bus API feed is deferred work.
- Demo seeding publishes the bus series with resolver authority and an endpoint built from the server's own `POLYNTU_BIND` loopback address. Databases seeded before this change keep their old bus series but ended, and a fresh resolver-authority series takes over, because published definitions are immutable.
- Resolver-authority instances never fall back to the simulated-evidence path the demo uses for administrator-authority markets, so a broken resolver surfaces as retries and a deadline void instead of being masked by automatic evidence.
- `record_resolver_evidence` takes a replay flag: a valid answer for a simulated instance is recorded even when a demo clock jump skipped the ask window past the published deadline, because the answer is deterministic and mirrors the simulated-evidence replay path; answers for manual instances keep the hard deadline.
- The resolve controls and password prompt moved from the series page to every bracket's market page when the series page was merged into it; the signing path itself is unchanged.
