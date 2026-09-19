# ADR 0007: Resolution authority: signed creator resolution and contracted external resolvers

Status: **proposed (planned, not yet implemented)**

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

Planned artifacts (none exist yet):

- key generation in the frontend
- signature verification in the resolution path
- the administrator evidence route exclusion
- the resolver contract module

Current code this decision will touch:

- `backend/src/resolution.rs`
- `backend/src/api.rs` (administrator evidence route)
- the `protect_instance` trigger in `backend/migrations/`
