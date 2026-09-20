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
| Account bearer token | `Authorization: Bearer <token>` | `/me`, private portfolio/history, quotes, trades, creator verification requests, and series publication |
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
| `GET /api/v2/series` | None | Up to 100 published series, active series first. |
| `GET /api/v2/series/{id}` | None | One series definition plus up to 100 of its brackets and the computed day view. |
| `POST /api/v2/series` | Account holding the creator role | Publishes a one-time market or a recurring series. |
| `GET /api/v2/instances/{id}` | None | Current instance snapshot and selected evidence. |
| `GET /api/v2/instances/{id}/history` | None | Time-bucketed price and volume history for one instance. |
| `GET /api/v2/instances/{id}/events` | None | Resumable public SSE events. |
| `POST /api/v2/instances/{id}/resolution` | Account; only the series creator | Submits a signed creator resolution for one closed instance. |
| `POST /api/v2/resolvers/ntu-bus` | None | The platform's own NTU Bus API adapter: asks the live provider first, falls back to the deterministic simulated feed, and answers pending before a bracket's observation window has ended. |
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
| `POST /api/v2/admin/worker/tick` | Administrator | Runs one close/bracket-spawn/evidence/settlement/seeding cycle. |
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

`GET /api/v2/markets` returns objects with `id`, `category`, and `title`. Templates organize recurring instances; they do not own inventory or settlement. Every published series also creates a template row with the series ID, so its brackets keep a browse grouping.

### Market series

A series is a published, immutable definition that either publishes exactly one instance immediately (one-time) or spawns bracket instances on a rolling grid (recurring). Creators publish manual-data series; simulated series stay demo-internal.

`POST /api/v2/series` requires the bearer session of an account holding the creator role (403 otherwise) and accepts the `NewSeries` shape:

```json
{
  "title": "Blue line arrival at North Spine",
  "resolution_criterion": "Yes if at least one matching bus arrives in the published [start, end) window...",
  "rule": {
    "kind": "bus",
    "route_id": "NTU-blue",
    "direction": "clockwise",
    "stop_id": "north-spine"
  },
  "source_id": "campus-bus-observer",
  "liquidity_units": 100,
  "fee_charged": true,
  "schedule": {
    "kind": "once",
    "close_ms": 1788919200000,
    "observation_start_ms": 1788919200000,
    "observation_end_ms": 1788922800000,
    "finalize_after_ms": 1788922801000,
    "evidence_deadline_ms": 1788922860000
  }
}
```

A recurring schedule replaces the five times with:

```json
{
  "kind": "recurring",
  "interval_ms": 120000,
  "active_start_minute": 360,
  "active_end_minute": 1439,
  "max_concurrency": 5,
  "end_ms": null
}
```

Validation includes:

- title 5–240 characters and resolution criterion 30–4,000 characters, as for instances;
- a valid typed rule and liquidity 10–100,000;
- `fee_charged` defaults to true and is fixed at publication;
- an optional `resolution` field fixes the resolution authority at publication ([ADR 0007](../decisions/0007-resolution-authority.md)): `{"kind":"creator","public_key":"base64"}` (a 32-byte ed25519 public key) or `{"kind":"resolver","endpoint":"https://..."}`; absent means the platform administrator resolves;
- recurring: `interval_ms` between 1 minute and 1 day, `max_concurrency` between 1 and 50, and an active window in minutes of day (start 0–1438, end 1–1439, start before end) interpreted in Singapore time; `active_days` optionally restricts which days of the week spawn brackets, as ISO day numbers (1 = Monday to 7 = Sunday, non-empty, stored sorted and deduplicated; absent means every day); `end_ms` is optional and must leave room for at least one more slot, and its absence means perpetual; and
- once: the same future time ordering as an instance (`close <= observation start < observation end <= finalize < deadline`, within one year).

A one-time series publishes its single instance immediately; if the bracket cannot be funded, the series row is removed again so nothing half-published remains. Recurring brackets close and observe their slot `[T, T+interval)`, finalize one second after the observation end, and carry an evidence deadline 60 seconds after it. Each spawned recurring bracket's title is `{series title} · HH:MM to HH:MM` (Singapore time, the bracket's `[T, T+interval)` window), truncated on a character boundary to fit the 240-character instance bound; a one-time series' instance keeps the creator's title unchanged. The response is the series view below.

`GET /api/v2/series` returns up to 100 rows ordered active series first, then newest. Each row contains `id`, `creator_account_id`, `title`, `category`, `state`, `recurrence`, `interval_ms`, `max_concurrency`, `fee_charged`, `end_ms`, and `created_ms`.

`GET /api/v2/series/{id}` returns the full definition: `id`, `creator_account_id`, `title`, `resolution_criterion`, `rule`, `source_id`, `data_mode`, `liquidity_units`, `fee_charged`, `state`, `created_ms`, a `resolution` object with `authority`, `public_key`, and `endpoint` ([ADR 0007](../decisions/0007-resolution-authority.md)), a `schedule` object with `kind`, `interval_ms`, `active_start_minute`, `active_end_minute`, `active_days`, `max_concurrency`, and `end_ms` (null where not applicable), up to 100 `instances` ordered by newest close time first, and a computed `day` object (see below).

The `day` object is computed on request from the listed brackets; it is never stored. It contains:

- `weighted_probability`: the volume-weighted first-outcome probability across live brackets, `sum(price x volume) / sum(volume)` over live brackets with traded volume; null when no live bracket has traded.
- `slots`: one entry per listed bracket, in the same newest-close-first order, each with `bracket_start_ms`, `close_ms`, `probability` (the first outcome's current price), `volume_micros` (total traded amount in that bracket), `state`, `result`, and the first outcome's `outcome_id` and `outcome_label`.

### Instance snapshot

Instance list and detail endpoints share the following core fields:

| Field | Meaning |
|---|---|
| `id` | Instance UUID. |
| `template_id` | Stable grouping/scheduling key. |
| `category` | `weather`, `bus`, `elections`, `queue_crowd`, or `attendance`. |
| `title` | Published human-readable question title; a spawned recurring-series bracket carries its time window (see [Market series](#market-series)). |
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
| `fee_charged` | Whether trades on this instance pay the 25-basis-point fee; fixed at creation. |
| `series_id` / `bracket_start_ms` | Series membership and grid slot for series brackets; null for direct creations. |
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

### Price and volume history

```http
GET /api/v2/instances/{id}/history?bucket_ms=60000
```

Returns time-bucketed prices and traded volume for one instance (UC-19). `bucket_ms` is clamped to 1 second to 1 day and defaults to 60,000.

```json
[
  {
    "start_ms": 1788919140000,
    "prices": [0.5, 0.5],
    "volume_micros": 0
  },
  {
    "start_ms": 1788919200000,
    "prices": [0.5137, 0.4862],
    "volume_micros": 5137761
  }
]
```

Semantics:

- Prices are reconstructed by replaying up to 5,000 recorded trades from the opening inventory, so each point is the price after the last fill in its bucket; no prices are stored.
- The first point carries the opening prices (volume 0) at the first traded bucket's start; later points carry their bucket's closing prices and volume, timestamped `start_ms` at their bucket's end.
- `prices` holds one entry per published outcome, in outcome order.
- `volume_micros` is the summed traded amount in that bucket.
- Buckets with no trades produce no point; gaps are simply absent.
- An instance with no trades returns a single point at the opening prices with zero volume.

### Pagination

```http
GET /api/v2/instances?limit=100&offset=0
GET /api/v2/markets/bus-blue/instances?limit=100&offset=0
```

The server does not return a total count or next-page token. A client infers that it reached the last page when fewer than `limit` rows are returned. Rows are ordered so markets that have not closed yet come first, soonest close first, followed by closed markets, most recently closed first; settled one-time markets can never bury the live ones on the first page.

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

`amount_micros` is the all-in debit (buy) or credit (sell): the LMSR amount plus a 25-basis-point trading fee, reported separately as `fee_micros` and rounded against the trader. Fee-free markets (`fee_charged:false`, fixed at creation) report `fee_micros` as 0 and an all-in amount equal to the pure LMSR amount. `average_price` divides the all-in amount by the quantity. The exact fields depend on inventory and liquidity. The token embeds the authoritative rounded amount and fee and expires after at most 15 seconds. A quote for an instance the account created is rejected with 409 `conflict`: creators cannot trade in their own markets, and the fee share is their compensation.

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
- Amounts are all-in: they include the 25-basis-point trading fee, except on fee-free markets where no fee applies.
- Using the quoted amount confirms exactly the previewed financial result.
- Reusing a key with the identical body returns its stored response.
- Reusing it with another body returns 409.
- The creator of the instance cannot execute trades: the same 409 rejection as at quote time.
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

## Bracket retention

Terminal brackets (`resolved` or `voided`) of recurring series whose evidence deadline passed more than 24 hours ago are purged by the worker in batches, together with their trades, positions, evidence, settlement claims, and outbox events. One-time markets are kept indefinitely.

Client-visible effects:

- instance detail, history, and events for a purged bracket return 404 `not_found`;
- purged brackets disappear from the series bracket list and the computed day view; the series row itself remains; and
- positions, settlement credits, and trades of purged brackets disappear from the portfolio and trade history, while balances keep the settled units: the append-only ledger trail survives every purge.

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
  "fee_charged": true,
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
- `fee_charged` (default true) is fixed at creation: a fee-free market charges nothing, collects no fee, and pays no creator share;
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

Administrator evidence is rejected outright, with 400 `invalid_request`, for any instance whose series resolution authority is not `admin`: the authority is fixed at creation and administrator evidence cannot resolve creator-signed or resolver-settled markets ([ADR 0007](../decisions/0007-resolution-authority.md)).

## Resolution

### `POST /api/v2/instances/{id}/resolution`

Submits a creator's signed human resolution for one instance of a series whose resolution authority is `creator` ([ADR 0007](../decisions/0007-resolution-authority.md)). Requires the bearer session of the series creator.

```json
{
  "outcome_id": "yes",
  "nonce": "one-time-random-string",
  "signature": "base64-ed25519-signature"
}
```

The signature is ed25519 over the exact UTF-8 string:

```text
polyntu.resolution.v1:{instance_id}:{outcome_id}:{nonce}
```

verified against the public key fixed at series creation. The nonce is 1–120 characters after trimming; use a fresh random value for each resolution.

The keypair the creator signs with is derived from the account password, so a programmatic creator can reproduce it:

- password input: the account password, exactly as registered;
- PRF: PBKDF2-HMAC-SHA256;
- salt: the UTF-8 string `polyntu.resolution.v1:{email}`, with the email lowercased as registered;
- iterations: 600,000;
- output: 32 bytes, used as the ed25519 seed; and
- public key: the 32-byte ed25519 verifying key, encoded as standard base64 (RFC 4648, padded), exactly the `public_key` published in the series' `resolution` object at creation.

The server never sees the password and performs no derivation: it only verifies each signature against the public key fixed at creation, so any correct implementation of this derivation produces a keypair the server accepts.

Conditions and errors:

- 404 `not_found`: unknown instance.
- 403 `forbidden`: the submitter is not the series creator (administrators included; only the creator can sign).
- 400 `invalid_request`: the series authority is not `creator`, the nonce is malformed, the outcome is unknown, or signature verification failed.
- 409 `conflict`: the instance is not `closed`, the observation window has not ended, the published evidence deadline has passed, or the instance already has a creator resolution (replays conflict; one resolution per instance).

Successful response:

```json
{
  "evidence_id": "evidence-uuid",
  "evaluated_result": {"kind": "winner", "outcome": 0},
  "outcome_id": "yes"
}
```

The verified outcome is recorded as evidence with source `creator-signature` (parser `creator-signature-v1`; the payload carries the outcome, nonce, signature, and public key) and settles through the normal finalization pipeline. The key is re-derivable from the account password in any browser, so the failure mode is a lost password: with no password recovery, resolution becomes impossible and the instance voids at its published deadline.

### External resolver contract

This is the interface an external service implements when a series is published with `{"kind":"resolver","endpoint":"..."}`. At the finalize window the settlement worker POSTs the fixed request to the endpoint:

```json
{
  "instance_id": "instance-uuid",
  "series_id": "series-uuid",
  "bracket_start_ms": 1788919200000,
  "window_start_ms": 1788919200000,
  "window_end_ms": 1788919260000,
  "outcomes": [{"id": "yes", "label": "Yes"}, {"id": "no", "label": "No"}],
  "title": "Blue line · arrival at Opp SPMS · 14:20 to 14:22",
  "evidence_deadline_ms": 1788919320000,
  "rule": {"kind": "bus", "route_id": "NTU-blue", "direction": "clockwise", "stop_id": "opp-spms"}
}
```

`bracket_start_ms` is null for a one-time series; `window_start_ms`/`window_end_ms` are the instance's observation window. The request is self-contained so an integrator can answer without a second lookup: `outcomes` lists every published outcome ID and label the answer may name, `title` identifies the bracket in human logs, and `evidence_deadline_ms` is the moment after which answers no longer count. The response must be exactly one of:

```json
{"outcome_id": "yes"}
```

naming one published outcome ID of that instance, or

```json
{"pending": true}
```

Anything else, including unknown outcome IDs and malformed bodies, is invalid. Extra response fields are ignored; the platform's own adapter adds a `source` field that the recorded evidence keeps, naming the path that answered.

- A valid answer is recorded as evidence with source `external-resolver` and settles through the normal pipeline.
- `pending`, malformed, and unreachable answers retry on every worker tick (one second) until the published evidence deadline, when the instance voids per the existing policy.
- Requests time out after 5 seconds; responses above 64 KiB are rejected.
- Endpoints must be https; plain http is accepted only on loopback, where local adapters run during development and tests.

### The platform's NTU Bus API adapter

The demo bus series resolves through this same contract with an endpoint the platform serves itself: `POST /api/v2/resolvers/ntu-bus`. The live data path comes first: the NTU Bus API provider (`provider/ntubus/service.py`, a standalone service that implements this contract and watches the live Omnibus feed, polling every route's buses and pickup points). The adapter tries the provider a few times and relays its answer with `"source": "ntubus-live"`; only when the provider stays without a definitive answer does the deterministic simulated feed answer, with `"source": "simulated-fallback"`, so development and CI work without the provider running. The recorded evidence carries the source, so which path answered is auditable. `POLYNTU_NTUBUS_PROVIDER` configures the provider URL (the default is the provider's own loopback default, `http://127.0.0.1:8090/resolve`; an empty value disables the live path).

The adapter accepts the fixed resolver request (only `instance_id` is read; the stored instance is the authority for its rule, window, and outcomes) and answers pending until the bracket's observation window has ended, so nothing is revealed early. Demo seeding builds the adapter URL from the server's own `POLYNTU_BIND` loopback address and covers the four scheduled campus lines (Blue and Red daily, Green weekdays, Brown weekends, each on its published 07:30 to 23:00 span; the Grey line has no published schedule), with real pickup points as stops. A valid answer that a demo clock jump made late is still recorded for simulated instances: the answer is deterministic, so recording it after the deadline is a replay exactly like simulated evidence; manual instances keep the hard published deadline.

## Demo clock, worker, and reconciliation

### Advance clock

```json
{"minutes": 60}
```

One request accepts 1–10,080 minutes, the cumulative offset is limited to ten years, and demo mode is required. The response contains `server_time_ms` and the number of accounts settled by the immediately following worker cycle.

### Tick worker

`POST /api/v2/admin/worker/tick` returns `{"settled_accounts":N}`. The number counts accounts processed during that cycle, not markets finalized. One cycle closes due instances, spawns due series brackets, produces simulated evidence, settles, ends finished series, and seeds demo markets.

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
