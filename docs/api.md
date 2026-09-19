# PolyNTU HTTP API v2

Status: **current external contract**  
Default origin: `http://127.0.0.1:8000`  
API prefix: `/api/v2`

This document describes the HTTP behaviour exposed to clients. For handler-to-service mappings, see the [API implementation reference](developer/api-reference.md).

## Conventions

- Request and response bodies are JSON unless the endpoint is an SSE stream.
- JSON request bodies are limited to 64 KiB.
- Financial values that may exceed JavaScript's safe integer precision are returned as decimal integer strings.
- Times are UTC Unix milliseconds. The frontend displays them in Asia/Singapore time.
- List `limit` values are clamped to 1–100 and `offset` to 0–100,000.
- Unknown v2 paths return the standard not-found error.
- Axum extractor failures, such as malformed JSON, may use Axum's rejection text instead of the application error envelope.

## Authentication

| Credential | Header | Used by |
|---|---|---|
| Account bearer token | `Authorization: Bearer <token>` | `/me`, private portfolio/history, quotes, trades, and creator verification requests |
| Administrator token | `X-Admin-Token: <token>` | Account provisioning, verification review, instance/evidence/suspension/clock/worker/reconciliation administration |
| None | (none) | Registration, login, configuration, markets, instances, health, and public instance events |

Account tokens are returned once: at registration, at login, or when a demo/administrator-provisioned account is created. Login rotates the token; an account holds at most one live bearer token, and each login invalidates every previous token. Inputs never provide their own account ID; the server derives ownership from the bearer token.

Administrator routes accept either the configured shared `X-Admin-Token` or the bearer session of an account holding the `admin` role. Demo-mode databases seed exactly one such account, `admin@ntu.edu.sg` with password `admin`, so it can sign in through login; its seed token was discarded, so the password is the only way in.

## Error envelope

Application errors use:

```json
{
  "error": {
    "code": "conflict",
    "message": "Market price changed. Request a new quote"
  }
}
```

| HTTP status | Code | Meaning |
|---:|---|---|
| 400 | `invalid_request` | Shape or domain validation failed. |
| 401 | `unauthorized` | Account token or signed quote authentication failed. |
| 401 | `invalid_credentials` | Login email/password pair does not match an account; the same message covers unknown email and wrong password. |
| 403 | `forbidden` | Administrator access or mode permission is missing. |
| 404 | `not_found` | Instance or route does not exist. |
| 409 | `conflict` | Valid request conflicts with current state, version, time, holdings, balance, revision, or idempotency history. |
| 410 | `options_retired` | A specifically retained legacy option route was requested. |
| 500 | `internal_error` | Database/internal failure. Retry an uncertain trade with the same body and idempotency key. |

Exact messages are useful to humans but are not a stable machine-enumerated error taxonomy. Clients should primarily branch on HTTP status and preserve uncertain trade requests.

## Endpoint summary

| Method and path | Authentication | Behaviour |
|---|---|---|
| `GET /health` | None | Checks PostgreSQL and identifies the Rust engine. Outside `/api/v2`. |
| `GET /api/v2/config` | None | Demo mode, authoritative time, unit scales, and quote lifetime. |
| `POST /api/v2/auth/demo` | None; demo mode only | Creates a funded demo account and returns its token once. |
| `POST /api/v2/auth/register` | None | Registers an NTU-email account with a password and returns its token once. |
| `POST /api/v2/auth/login` | None | Verifies email and password, rotates the session token, and returns it once. |
| `GET /api/v2/me` | Account | Current account identity, email/role, and available balance. |
| `GET /api/v2/markets` | None | Up to 100 templates ordered by category and ID. |
| `GET /api/v2/instances` | None | Paginated instances across templates. |
| `GET /api/v2/markets/{id}/instances` | None | Paginated instances for one template ID. |
| `GET /api/v2/instances/{id}` | None | Current instance snapshot and selected evidence. |
| `GET /api/v2/instances/{id}/events` | None | Resumable public SSE events. |
| `POST /api/v2/quotes` | Account | Read-only signed price preview. |
| `POST /api/v2/trades` | Account plus idempotency header | Executes or retrieves one exact quoted trade. |
| `GET /api/v2/me/portfolio` | Account | Paginated positions and settlement credits. |
| `GET /api/v2/me/trades` | Account | Paginated private trade history. |
| `POST /api/v2/verification-requests` | Account | A member files one pending creator verification request. |
| `GET /api/v2/verification-requests` | Account | The requester's own latest verification request, or null. |
| `POST /api/v2/admin/accounts` | Administrator | Creates a funded account in either mode; primarily needed outside public demo enrollment. |
| `POST /api/v2/admin/instances` | Administrator | Validates, funds, and immediately opens an immutable instance. |
| `GET /api/v2/admin/verification-requests` | Administrator | Verification review list, optionally filtered by status. |
| `POST /api/v2/admin/verification-requests/{id}/decision` | Administrator | Approves or rejects a pending verification request. |
| `POST /api/v2/admin/instances/{id}/evidence` | Administrator | Appends a manual evidence revision. |
| `POST /api/v2/admin/instances/{id}/suspension` | Administrator | Suspends or resumes trading on an open instance. |
| `POST /api/v2/admin/clock/advance` | Administrator; demo mode only | Advances demo time and runs a worker cycle. |
| `POST /api/v2/admin/worker/tick` | Administrator | Runs one close/evidence/settlement/scheduling cycle. |
| `GET /api/v2/admin/reconcile` | Administrator | Checks ledger, inventory, reserve, and total-unit invariants. |

## Health and configuration

### `GET /health`

```json
{
  "status": "ok",
  "backend": "rust",
  "engine": "lmsr-decimal-v1"
}
```

A database connection/query failure becomes `500 internal_error`.

### `GET /api/v2/config`

```json
{
  "demo_mode": true,
  "server_time_ms": 1788912000000,
  "credit_scale": 1000000,
  "share_scale": 1000,
  "quote_ttl_ms": 15000
}
```

Use `server_time_ms` for programmatic scheduling and countdown adjustment, especially when demo time has advanced.

## Accounts

### Create a demo or administrator-provisioned account

`POST /api/v2/auth/demo` and `POST /api/v2/admin/accounts` share this body:

```json
{
  "display_name": "Alex"
}
```

The trimmed name must be 2–60 characters and contain no control characters. The demo endpoint is disabled unless `POLYNTU_DEMO_MODE=true`. The administrator endpoint works in either mode.

Successful response:

```json
{
  "token": "returned-only-now",
  "account": {
    "id": "account-uuid",
    "display_name": "Alex",
    "email": null,
    "role": null,
    "balance_micros": "1000000000"
  }
}
```

The grant is 1,000 simulated units transferred from the finite treasury. Demo and administrator-provisioned accounts carry no email, password, or role, so they cannot use login; the returned token is their only credential.

### Register an NTU account

`POST /api/v2/auth/register`:

```json
{
  "display_name": "Alex",
  "email": "Alex@SCSE.NTU.EDU.SG",
  "password": "at-least-12-characters"
}
```

- The trimmed display name must be 2–60 characters and contain no control characters.
- The email is trimmed and normalized to lowercase, must be an NTU address (`name@ntu.edu.sg` or `name@unit.ntu.edu.sg`; further unit labels are accepted), and must be unique.
- The password must contain at least 12 characters. It is stored only as an argon2id hash.

Successful response:

```json
{
  "token": "returned-only-now",
  "account": {
    "id": "account-uuid",
    "display_name": "Alex",
    "email": "alex@scse.ntu.edu.sg",
    "role": "member",
    "balance_micros": "10000000000"
  }
}
```

The new account receives the `member` role and the 10,000-unit welcome gift (10,000,000,000 micros) from the treasury.

Errors: 400 `invalid_request` for a bad display name, a non-NTU email, or a short password; 409 `conflict` when the email is already registered or the treasury gift budget is exhausted.

### Log in with email and password

`POST /api/v2/auth/login`:

```json
{
  "email": "alex@scse.ntu.edu.sg",
  "password": "at-least-12-characters"
}
```

The email is normalized to lowercase exactly as at registration. The successful response repeats the register shape with the account's current balance.

Login rotates the account's single session token: the new plaintext token is returned once and every previous token stops working immediately. An account holds at most one live bearer token.

Errors: unknown email and wrong password both return 401 `invalid_credentials` with the same message. Unknown-email attempts still perform one argon2 verification, so response timing cannot enumerate registered addresses. Demo and administrator-provisioned accounts have no email and cannot log in.

### `GET /api/v2/me`

```json
{
  "id": "account-uuid",
  "display_name": "Alex",
  "email": "alex@scse.ntu.edu.sg",
  "role": "member",
  "balance_micros": "994875052"
}
```

`email` and `role` are null for demo and administrator-provisioned accounts. For registered accounts, `role` is `member`, `creator`, or `admin`.

## Markets and instances

### Template response

`GET /api/v2/markets` returns objects with `id`, `category`, and `title`. Templates organize recurring instances; they do not own inventory or settlement.

### Instance snapshot

Instance list and detail endpoints share the following core fields:

| Field | Meaning |
|---|---|
| `id` | Instance UUID. |
| `template_id` | Stable grouping/scheduling key. |
| `category` | `weather`, `bus`, `elections`, `queue_crowd`, or `attendance`. |
| `title` | Published human-readable question title. |
| `resolution_criterion` | Published explanation of how the result is chosen. |
| `rule` | Typed immutable rule JSON. |
| `source_id` | Required evidence source identifier. |
| `data_mode` | `simulated` or `manual`. |
| `state` | Effective `open`, `closed`, `resolving`, `resolved`, or `voided`. |
| `suspended` / `tradable` | Administrative flag and computed ability to trade now. |
| `outcomes` | Fixed IDs/labels plus current probability and outstanding millishares. |
| `version` | Changes on every instance update; quotes bind to it. |
| timing fields | `close_ms`, `observation_start_ms`, `observation_end_ms`, `finalize_after_ms`, and `evidence_deadline_ms`. |
| `liquidity_units` | Fixed LMSR parameter `b`. |
| `result` | `null`, `{"kind":"winner","outcome":0}`, or `{"kind":"void","reason":"..."}`. |
| `creator_account_id` | Optional participant account credited with the creator's half of the settled trading-fee pot. |
| `evidence_id` | Selected highest-revision evidence record, when present. |
| `server_time_ms` | Authoritative time used to build the view. |
| `void_policy` | Human-readable uniform fractional redemption policy. |

Each outcome has:

```json
{
  "id": "yes",
  "label": "Yes",
  "probability": 0.5,
  "outstanding_millis": 0
}
```

The detail endpoint additionally includes `evidence` with `source_id`, `event_id`, `received_ms`, and the complete normalized `payload`. That payload is public. Administrators must not submit credentials, personal identifiers, private URLs, or other sensitive references.

### Pagination

```http
GET /api/v2/instances?limit=100&offset=0
GET /api/v2/markets/bus-blue/instances?limit=100&offset=0
```

The server does not return a total count or next-page token. A client infers that it reached the last page when fewer than `limit` rows are returned.

## Quotes

### `POST /api/v2/quotes`

```json
{
  "instance_id": "instance-uuid",
  "outcome_id": "yes",
  "side": "buy",
  "quantity_millis": 10000
}
```

- `side` is `buy` or `sell`.
- Quantity is 1–100,000 millishares: 0.001–100 shares.
- Sales are quoteable without an ownership read, but execution rejects quantities the account does not own.
- A quote is a pure preview and does not reserve inventory, create an idempotency record, or alter the clock.

Example response:

```json
{
  "quote_token": "base64url-payload.base64url-hmac",
  "instance_id": "instance-uuid",
  "outcome_id": "yes",
  "side": "buy",
  "quantity_millis": 10000,
  "amount_micros": "5137761",
  "fee_micros": "12813",
  "version": 0,
  "expires_ms": 1788912015000,
  "server_time_ms": 1788912000000,
  "average_price": 0.5137761,
  "price_before": 0.5,
  "price_after": 0.5249791875
}
```

`amount_micros` is the all-in debit (buy) or credit (sell): the LMSR amount plus a 25-basis-point trading fee, reported separately as `fee_micros` and rounded against the trader. `average_price` divides the all-in amount by the quantity. The exact fields depend on inventory and liquidity. The token embeds the authoritative rounded amount and fee and expires after at most 15 seconds.

## Trades and idempotency

### `POST /api/v2/trades`

```http
POST /api/v2/trades
Authorization: Bearer <account token>
Idempotency-Key: 60353dca-f5d5-43df-b559-a148cc943857
Content-Type: application/json
```

```json
{
  "quote_token": "signed quote",
  "limit_micros": "5124948"
}
```

The idempotency key must contain 1–120 ASCII letters, digits, hyphens, or underscores. It is scoped to the authenticated account.

- For a buy, `limit_micros` is the maximum debit.
- For a sale, it is the minimum credit.
- Amounts are all-in: they include the 25-basis-point trading fee.
- Using the quoted amount confirms exactly the previewed financial result.
- Reusing a key with the identical body returns its stored response.
- Reusing it with another body returns 409.
- Expired/stale quotes, insufficient units, insufficient shares, suspended/closed markets, or reserve violations return 409 before effects commit.

Successful response:

```json
{
  "trade_id": "trade-uuid",
  "instance_id": "instance-uuid",
  "outcome_id": "yes",
  "side": "buy",
  "quantity_millis": 10000,
  "amount_micros": "5137761",
  "fee_micros": "12813",
  "balance_micros": "994862239",
  "owned_millis": 10000,
  "version": 1,
  "created_ms": 1788912001000
}
```

After a connection interruption or unreadable response, retry the exact body with the same key. Do not generate a replacement key until the original request has a definitive result.

## Portfolio and trade history

### `GET /api/v2/me/portfolio?limit=100&offset=0`

The response contains:

- `account`: current identity and available balance;
- `positions`: positive positions, with outcome label, original share count, current state/result, and `settled`; and
- `settlements`: one credit per settled account/instance.

The same `limit` and `offset` are applied independently to the positions query and settlements query inside this one response. They do not have separate offsets. Historical positions retain their quantity after redemption; credits are kept in the settlement list so one account credit is not repeated for every outcome position.

### `GET /api/v2/me/trades?limit=100&offset=0`

Returns private trades ordered newest first. Each entry contains trade/instance IDs, title, outcome label, side, quantity, amount string, and creation time. This endpoint has pagination independent from the portfolio request.

## Creator verification

### `POST /api/v2/verification-requests`

An authenticated member files one pending creator verification request:

```json
{
  "id": "request-uuid",
  "account_id": "account-uuid",
  "status": "pending",
  "created_ms": 1788912000000
}
```

- Only a `member` may apply. Creators, administrators, and demo accounts (which have no email) are each rejected with a specific 400 `invalid_request` message.
- At most one pending request may exist per account; a second returns 409 `conflict`.
- A member whose previous request was rejected may apply again.

### `GET /api/v2/verification-requests`

Returns the requester's own latest request, or `null` when none exists:

```json
{
  "id": "request-uuid",
  "status": "rejected",
  "reason": "Insufficient public activity record",
  "created_ms": 1788912000000,
  "decided_ms": 1788915600000
}
```

`reason` and `decided_ms` are null while the request is pending.

### `GET /api/v2/admin/verification-requests?status=pending`

Administrator route. Returns up to 200 requests ordered newest first, each including the requester's `display_name` and `email` alongside the fields above. The optional `status` filter must be `pending`, `approved`, or `rejected`; anything else is 400 `invalid_request`.

### `POST /api/v2/admin/verification-requests/{id}/decision`

```json
{
  "approve": false,
  "reason": "Insufficient public activity record"
}
```

- A rejection requires a nonblank `reason` (400 `invalid_request` otherwise); a reason is optional for approval.
- Only a pending request can be decided; any other ID returns 404 `not_found`.
- Approval permanently sets the requester's role to `creator`. If the requester is no longer an eligible member, approval returns 409 `conflict` and the request stays pending.
- Every decision appends an administrator audit row with action `verification_decision`.

Successful response:

```json
{
  "id": "request-uuid",
  "account_id": "account-uuid",
  "status": "rejected"
}
```

## Instance administration

### Create an instance

`POST /api/v2/admin/instances` accepts the `NewInstance` shape:

```json
{
  "template_id": "weather-rain",
  "title": "Rainfall at the campus station",
  "resolution_criterion": "Yes if complete recorded rainfall reaches 0.2 mm during the published interval.",
  "rule": {
    "kind": "weather",
    "station_id": "station-S",
    "threshold_milli_mm": 200
  },
  "source_id": "campus-weather-observer",
  "data_mode": "manual",
  "close_ms": 1788832800000,
  "observation_start_ms": 1788832800000,
  "observation_end_ms": 1788836400000,
  "finalize_after_ms": 1788836460000,
  "evidence_deadline_ms": 1788837000000,
  "liquidity_units": 100,
  "creator_account_id": "optional-participant-account-uuid"
}
```

Validation includes:

- title 5–240 characters and resolution criterion 30–4,000 characters;
- bounded nonempty template/source identifiers;
- valid typed rule and derived category/outcomes;
- future `close <= observation start < observation end <= finalize < deadline`;
- evidence deadline within one year of creation;
- liquidity 10–100,000;
- `creator_account_id`, when present, must be an existing participant account and is immutable after publication; and
- simulated mode only in demo mode with source `polyntu-simulator-v1`.

Creation immediately funds and opens the instance. There is no draft/edit endpoint.

### Rule JSON

| Shape | JSON fields |
|---|---|
| Weather | `{"kind":"weather","station_id":"station-S","threshold_milli_mm":200}` |
| Bus | `{"kind":"bus","route_id":"NTU-blue","direction":"clockwise","stop_id":"north-spine"}` |
| Election | `{"kind":"election","candidates":["A","B","C"],"is_fictional":true}` |
| Queue count | `{"kind":"count","metric":"queue_length","location_id":"canteen-queue","threshold":20}` |
| Occupancy | `{"kind":"count","metric":"crowd_occupancy","location_id":"library-zone-A","threshold":100}` |
| Attendance | `{"kind":"count","metric":"unique_attendance","location_id":"event-001","threshold":150}` |

Unknown fields are rejected.

### Suspension

`POST /api/v2/admin/instances/{id}/suspension`:

```json
{
  "suspended": true,
  "reason": "Source investigation"
}
```

The reason must be 5–1,000 characters. Only an `open` instance may change suspension state. A successful response is `{"ok":true}`.

## Evidence

### `POST /api/v2/admin/instances/{id}/evidence`

Common envelope:

```json
{
  "source_id": "campus-weather-observer",
  "event_id": "station-reading-revision-2",
  "source_revision": 2,
  "window_start_ms": 1788832800000,
  "window_end_ms": 1788836400000,
  "observation": {
    "kind": "weather",
    "station_id": "station-S",
    "total_milli_mm": 400,
    "complete": true
  },
  "reference": "Public archived source record identifier"
}
```

The event ID is at most 120 characters, revision is 0–1,000,000, reference is nonblank and at most 2,000 characters, encoded input is at most 64 KiB, and source/window must exactly match the published instance.

Observation variants:

```json
{"kind":"weather","station_id":"station-S","total_milli_mm":400,"complete":true}
```

```json
{"kind":"bus","route_id":"NTU-blue","direction":"clockwise","stop_id":"north-spine","arrivals_ms":[1788833000000],"complete":true}
```

```json
{"kind":"election","winner":"Candidate A","is_final":true,"is_fictional":true}
```

```json
{"kind":"count","metric":"unique_attendance","location_id":"event-001","value":162,"complete":true}
```

Weather/count values must be nonnegative. Bus arrays are limited to 10,000 timestamps; only entries in `[window_start_ms, window_end_ms)` determine Yes. Election winners must exactly match a published candidate. `complete:false` or `is_final:false` waits rather than implying No.

Successful new evidence returns its ID, `duplicate:false`, and the evaluated result or `null`. Repeating identical event content returns the same ID with `duplicate:true`.

## Demo clock, worker, and reconciliation

### Advance clock

```json
{"minutes": 60}
```

One request accepts 1–10,080 minutes, the cumulative offset is limited to ten years, and demo mode is required. The response contains `server_time_ms` and the number of accounts settled by the immediately following worker cycle.

### Tick worker

`POST /api/v2/admin/worker/tick` returns `{"settled_accounts":N}`. The number counts accounts processed during that cycle, not markets finalized.

### Reconcile

`GET /api/v2/admin/reconcile` returns:

```json
{
  "ok": true,
  "account_errors": 0,
  "inventory_errors": 0,
  "reserve_errors": 0,
  "net_units_micros": "0"
}
```

Any nonzero error count or net total makes `ok:false`.

## Server-Sent Events

`GET /api/v2/instances/{id}/events` emits events named `market`:

```text
id: 1234
event: market
data: {"instance_id":"...","version":3,"type":"trade","created_ms":1788912001000}
```

Resume with `Last-Event-ID: 1234` or `?after=1234`. Types currently include `opened`, `trade`, `evidence`, `suspension`, `closed`, `resolving`, `resolved`, and `voided`. Keep-alive frames occur every 15 seconds. Committed events are pushed through PostgreSQL notifications, typically arriving within milliseconds; a slow catch-up pass over the durable event log runs every 30 seconds as a safety net. Events signal that state changed; fetch a fresh snapshot after reconnecting or receiving an event.

## Retired routes

These legacy paths return `410 options_retired`:

- `POST` or `GET /contracts`
- `GET /markets/{id}/quote`
- `GET /markets/{id}/implied-sigma`

Other Python-era routes are not mounted and normally return the static fallback or not found, depending on path. No active route performs Black–Scholes or options pricing.
