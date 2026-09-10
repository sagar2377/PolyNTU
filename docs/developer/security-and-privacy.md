# Security and privacy

PolyNTU's current controls are designed for a loopback academic demonstration and a future authenticated institutional service. They are not a claim of production security certification.

## Trust boundaries

```text
untrusted browser input
    -> Axum parsing, size limits, authentication, domain validation
        -> service transactions and signed quote verification
            -> PostgreSQL constraints/triggers

administrator client
    -> shared administrator token
        -> account/instance/evidence/clock/worker/reconciliation operations

future provider
    -> adapter outside this repository
        -> normalized public-safe evidence
```

The browser, account holder, administrator input, and future evidence provider can all submit incorrect or malicious data. Database contents and logs are sensitive operational assets even though units are simulated.

## Secrets

| Secret | Purpose | Storage in current development flow |
|---|---|---|
| Account bearer token | Select one private user account | Plaintext browser local storage; SHA-256 hash in PostgreSQL |
| Quote secret | Authenticate quote claims | Environment; generated in `.local/dev-secrets.json` for workspace demo |
| Administrator token | Protect administrator endpoints | Environment; generated separately in `.local/dev-secrets.json` |
| Simulation secret | Prevent public prediction of demo outcomes | Plaintext private PostgreSQL settings row |
| Database password | Protect non-local PostgreSQL | Environment/secret manager; local portable cluster uses loopback trust |

The application checks only that quote/admin strings contain at least 32 characters and differ. Operators must generate high-entropy values, restrict file/environment access, and never commit them.

## Account authentication

Account provisioning generates 256 random bits with the operating-system RNG and returns the base64url token once. Database lookup hashes the presented value with SHA-256.

Security properties and limits:

- a database reader does not directly receive bearer tokens;
- bearer tokens have no expiry, scopes, rotation, or revocation endpoint;
- possession grants full account access;
- there is no campus identity binding, MFA, or recovery;
- display names are not unique identities; and
- sign-out deletes browser storage but does not invalidate the token.

Do not deploy account tokens through email, logs, screenshots, or public issue reports.

## Administrator authentication

One shared token protects all `/admin/*` routes. Comparison uses HMAC verification of a fixed message so tag comparison is constant-time.

There is no administrator identity, per-action role, separate resolver, expiry, rotation, or multi-party approval. Audit rows cannot identify which human used the shared token.

Audit coverage includes instance creation, evidence events/rejections, suspension, result finalization, simulated evidence, and demo clock advancement. Administrator account creation is represented by the account and grant ledger transfer but does not add an `admin_audit` row. Reconciliation is read-only; calling worker tick itself is not audited, although resulting state transitions may be.

## Signed quotes

HMAC-SHA256 authenticates the encoded JSON claim. Claims bind account, instance, outcome, side, quantity, amount, market version, expiry, and engine version.

This prevents a user from changing signed fields, using another account's quote, or executing against a different engine/version. It does not encrypt the payload: clients can decode its contents, and it must contain no secret other than its signature.

Quotes are replayable until one succeeds or state/version/expiry rejects them. Account-scoped idempotency ensures an intended trade commits once. Keep the quote signing secret stable across ordinary restarts.

## Request and network controls

- JSON bodies are limited to 64 KiB.
- Identifier, text, array, quantity, balance, liquidity, revision, and time ranges are bounded in service validation/schema.
- CORS allows only configured origins and a small method/header set.
- Demo mode refuses a non-loopback backend bind.
- Docker publishes port 8000 to host loopback by default.
- Database statements have a 15-second timeout and locks a 2-second timeout.

CORS is not authentication and does not protect non-browser clients. The repository does not configure TLS, reverse-proxy controls, request-rate limiting, IP restrictions, security headers, Content Security Policy, or denial-of-service mitigation.

## Database controls

PostgreSQL constraints and triggers enforce nonnegative non-issuance balances, bounded inventories, valid lifecycle transitions, immutable published fields/results, append-only ledger/trade/evidence/claim/audit records, unique trades per version, and unique claims/idempotency/event identities.

Application processes currently use one database URL rather than separate least-privilege migration/runtime roles. The portable Windows cluster uses trust authentication on loopback. Production should use password/certificate authentication, encrypted transport as appropriate, restricted network access, separate backup credentials, and least-privilege roles.

## Evidence privacy

The public detail endpoint returns the selected normalized evidence payload, including its `reference`, and the frontend displays it. This is the most important current privacy rule:

> Only submit evidence that is safe to publish to every visitor.

For attendance and crowd data, submit aggregate counts only. Never include student names, IDs, emails, device identifiers, raw check-ins, access tokens, signed provider URLs, internal case notes, or private payloads.

Future adapters should retain raw provider data in a separately controlled store and submit a public immutable record ID/reference. The current backend does not provide field-level redaction or separate private/public evidence columns.

## Browser storage

The React demo stores the bearer token and one pending trade in local storage. Any script executing in the same origin can read them. A cross-site scripting vulnerability would therefore expose account access and possibly an uncertain request.

Before public deployment:

- choose an institutional authentication/session design;
- add an appropriate Content Security Policy and security headers;
- avoid long-lived bearer tokens in local storage;
- define session expiry, revocation, recovery, and device/logout behaviour; and
- perform dependency, frontend input/output, and deployment review.

React escapes normal text interpolation, including titles, evidence JSON, and error strings. Continue avoiding raw HTML injection APIs.

## Threat and control summary

| Threat | Current control | Remaining limitation |
|---|---|---|
| Modify quote amount/account | HMAC-bound claims and recomputation | Secret rotation unavailable |
| Duplicate uncertain trade | Account-scoped body-hash idempotency | Rows have no retention policy |
| Spend twice concurrently | Account locks and nonnegative DB constraint | Production contention not fully saturated |
| Trade after close | Database time rechecked under instance lock | Clock depends on database/operator integrity |
| Rewrite result/evidence | Immutable triggers and append-only revisions | Shared admin token controls ingestion |
| Double settlement | Unique claim key and transactional batches | Operational monitoring is manual |
| Infer future demo outcome | Private persisted seed plus instance ID | Database readers can access the secret |
| Leak attendee data | Aggregate evidence contract and documentation | No automatic reference redaction |
| Public abuse | Loopback demo bind | No rate limiting/public hardening |

## Logging

Tracing records request/service errors. Internal/database error strings are logged but hidden from HTTP clients. Do not add request-body, authorization-header, quote-token, secret, or evidence-payload logging. Use instance, event, evidence, trade, and audit IDs for correlation.

## Production readiness checklist

Before changing from local/institutional evaluation to a public service:

1. define legal/product approval for the market categories and simulated-unit use;
2. implement campus SSO/session recovery and administrator/resolver roles;
3. introduce secret storage and tested rotation procedures;
4. restrict database roles/network and enable tested backups;
5. add TLS, proxy/security headers, CSP, and rate limits;
6. define evidence privacy, retention, and provider verification;
7. add audit access/retention and actor identity;
8. add observability and incident response;
9. run security review and dependency scanning;
10. run load, recovery, and manual accessibility/browser testing in the target environment.

