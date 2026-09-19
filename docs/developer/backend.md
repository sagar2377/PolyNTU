# Backend code reference

This document explains every active Rust source module and the important call relationships. It is a guide to the code, not generated Rust API documentation.

## Crate entry points

### `backend/src/lib.rs`

The library crate exposes `amm`, `api`, `auth`, `cache`, `error`, `events`, `execution`, `fee`, `market`, `resolution`, `resolver`, `store`, and `worker`. Integration tests import the application through this library. `#![forbid(unsafe_code)]` prevents unsafe blocks in the application crate.

### `backend/src/main.rs`

`main` is the only executable entry point. It:

- configures `tracing_subscriber`;
- reads required environment variables;
- checks that demo mode binds only to loopback;
- opens `Store` and therefore applies migrations/initialization;
- constructs `AppState`, which validates secret length and separation;
- seeds demo markets;
- parses allowed CORS origins;
- mounts API/static routing;
- starts `worker::run`; and
- serves until Ctrl+C.

An initialization failure is returned before the listener starts. The worker is aborted after HTTP graceful shutdown completes.

## Error handling: `error.rs`

`Error` separates invalid input, state conflicts, missing authentication, forbidden administration, missing resources, database failures, and other internal failures.

| Variant | HTTP result |
|---|---|
| `Invalid(String)` | 400 `invalid_request`, caller-safe message |
| `Conflict(String)` | 409 `conflict`, caller-safe message |
| `Unauthorized` | 401 `unauthorized` |
| `Forbidden` | 403 `forbidden` |
| `NotFound` | 404 `not_found` |
| `Database` / `Internal` | 500 `internal_error`; details logged only |

The generic 500 response advises retrying with the same idempotency key because the server cannot tell the browser whether a trade response was lost after commit. The advice is specifically safe for exact trade retries; callers of unrelated endpoints may simply retry according to their operation policy.

`invalid` and `conflict` are small constructors used to keep domain checks readable.

## Authentication and signing: `auth.rs`

- `random_token` reads 32 bytes from the operating-system RNG (`SysRng`) and encodes URL-safe base64 without padding; it returns an error rather than panicking if the entropy source fails.
- `hash` returns lowercase SHA-256 hex, building the hex string manually because the `sha2` upgrade dropped digest formatting; stored token hashes keep their exact historical encoding. Account tokens are looked up by this hash.
- `sign` serializes a value as JSON, base64url-encodes it, and appends an HMAC-SHA256 signature over the encoded body.
- `verify` rejects tokens longer than 4,096 characters, verifies the HMAC before decoding/deserializing, and maps failures to `Unauthorized`.
- `valid_secret` checks only that the string has at least 32 characters. Deployment must still generate high-entropy secrets.
- `verify_admin` derives fixed HMAC tags from configured and candidate strings and uses HMAC verification for constant-time tag comparison.
- `valid_ntu_email` accepts only `name@ntu.edu.sg` or `name@unit.ntu.edu.sg` (further unit labels allowed, lowercase only) with bounded local/domain lengths; callers normalize to lowercase first.
- `hash_password` returns an argon2id hash in PHC string form with a fresh random salt; `verify_password` fails closed on a malformed stored hash so every verification is treated the same way.
- `resolution_message` builds the exact string a creator signs for a human resolution, `polyntu.resolution.v1:{instance_id}:{outcome_id}:{nonce}` (ADR 0007).
- `verify_ed25519` checks that signature against a base64 32-byte public key with ed25519-dalek, failing closed on malformed keys or signatures; the platform holds only public keys, so it can verify but never forge a resolution.

The quote secret and administrator token must be different. Login rotates an account's single session token; tokens still have no expiry, there is no password reset or recovery, and there is no administrator role hierarchy beyond the admin role.

## Market arithmetic: `amm.rs`

### Constants

| Constant | Value | Meaning |
|---|---:|---|
| `CREDIT_SCALE` | 1,000,000 | Microcredits per unit |
| `SHARE_SCALE` | 1,000 | Millishares per share |
| `MAX_QUANTITY` | 100,000 | Maximum millishares in one request |
| `MAX_INVENTORY` | 1,000,000,000 | Maximum millishares per outcome |
| `ENGINE_VERSION` | `lmsr-decimal-v1` | Signed-quote compatibility identifier |

`Side` serializes as `buy` or `sell`. `Calculation` contains the rounded amount, new inventory, and before/after probabilities.

### Functions

- `exp` applies decimal exponential tolerance `1e-25` and translates failure to a bounded numerical error.
- `validate` enforces outcome count, liquidity, nonnegative/bounded inventory, and concentration spread.
- `probabilities` shifts inventories by their maximum before exponentiation and normalizes decimal weights.
- `prices` converts bounded decimal probabilities to display-only `f64`.
- `funding` computes `ceil(b * CREDIT_SCALE * ln(n)) + 1`.
- `calculate` validates the requested outcome/quantity, applies signed inventory delta, computes a stable cost difference, rounds buy debits upward or sale proceeds downward, and returns both inventory and display prices.

The pure engine does not check account holdings. A sale that would take aggregate market inventory below zero fails in `calculate`; account-specific overselling is rejected later in execution.

## Domain model: `market.rs`

### Outcome and metric types

`Outcome` has stable `id` and display `label`. Binary rules derive `yes` and `no`; elections derive `candidate-0` through `candidate-N` from labels.

`Metric` distinguishes `queue_length`, `crowd_occupancy`, and `unique_attendance`. They share a numeric rule shape without conflating their meaning.

### `Rule`

The tagged enum contains:

- `Weather { station_id, threshold_milli_mm }`
- `Bus { route_id, direction, stop_id }`
- `Election { candidates, is_fictional }`
- `Count { metric, location_id, threshold }`

`Rule::category` maps the rule to the database/frontend category key. `Rule::validate` bounds identifiers/thresholds and requires 2–8 distinct fictional candidates. `Rule::outcomes` derives the immutable outcome set.

`Rule::evaluate` compares one matching observation with the published rule:

- complete weather/count inputs select Yes/No at `>= threshold`;
- complete bus coverage selects Yes when any arrival is in `[start,end)` and No otherwise;
- non-final election evidence waits;
- a sole known final candidate wins;
- final `winner:null` voids; and
- shape/identifier/unit mismatches are invalid rather than evidence for No.

`Rule::simulated` deterministically derives one matching observation from a supplied private seed string. The worker supplies a hash of the database simulation secret and instance ID; callers do not invoke it during quotes.

### Instance request/storage types

`NewInstance` is the administrator request. `validate` checks the rule/funding, text and identifier lengths, mode/source constraints, future time ordering, and one-year maximum deadline.

`Instance` mirrors the PostgreSQL row with JSON-decoded rule/outcomes/result. `tradable` requires persisted state `open`, not suspended, and authoritative time before close. `outcome_index` resolves a stable public outcome ID.

`EvidenceInput` is the public normalized evidence envelope. All request/observation types deny unknown JSON fields so spelling mistakes do not silently disappear.

`demo_specs` creates six definitions, all with uniform liquidity 100 and simulator source; the former one-shot bus spec became the rolling series below. Non-election templates recur through the worker; the election remains a single demo occurrence.

### Series types (ADR 0006)

`Schedule` is the tagged recurrence enum: `Once` carries the five instance times, and `Recurring` carries `interval_ms` (one minute to one day), `active_start_minute`/`active_end_minute` (minutes of day, Singapore time), `max_concurrency` (capped at 50), and an optional `end_ms` whose absence means perpetual. `validate` bounds all of them. `bracket_timing` maps a slot start to instance times: the slot is both the close and the observation window `[T, T+interval)`, finalization follows one second after the observation end (`BRACKET_FINALIZE_MARGIN_MS`), and the evidence deadline sixty seconds after (`BRACKET_DEADLINE_MARGIN_MS`).

`inside_active_window` interprets a slot start in Singapore time (UTC+8, no daylight saving) and tests it against the daily window. `NewSeries` is the publication request (title, criterion, rule, source, liquidity, `fee_charged` defaulting to true, an optional resolution authority, schedule) and `instance_spec` builds one bracket's `NewInstance`. The resolution authority is `ResolutionSpec` ([ADR 0007](../decisions/0007-resolution-authority.md)): `Creator` with a base64 32-byte ed25519 public key, or `Resolver` with an https endpoint (plain http on loopback only, for local adapters); absent means the platform administrator resolves. `Series` mirrors the database row; `Series::bracket_spec` rebuilds a bracket spec from a stored row. `demo_series_spec` is the rolling fee-free demo bus: simulated data, a 2-minute interval, the 06:00 to 23:59 operating window, maximum concurrency 5, perpetual.

## HTTP layer: `api.rs`

`AppState` owns the cloned `Store` and shared quote/admin secrets. Its private helpers resolve bearer tokens to accounts and gate administrator access: `require_admin` accepts either the configured shared `X-Admin-Token` (constant-time HMAC comparison) or the bearer session of an admin-role account, reading the role fresh from the database on every request.

`router` creates a nested `/api/v2` router, explicit retired routes, global 64 KiB limit, configured CORS, request tracing, and the shared state. The series routes (`POST /api/v2/series`, `GET /api/v2/series`, `GET /api/v2/series/{id}`) resolve the bearer account and delegate to the store; publication requires the creator role and publishes manual-data series. `GET /api/v2/instances/{id}/history` and `POST /api/v2/instances/{id}/resolution` follow the same thin pattern, delegating to `Store::instance_history` and `Store::record_creator_resolution`. Handlers remain deliberately thin: authenticate/parse, call one store/service method, and serialize its result.

The SSE handler is the exception: it validates the instance, initializes a nonnegative cursor, subscribes to the instance's broadcast channel, replays up to 100 outbox rows per catch-up pass, and emits `market` events as notifications arrive, with a 30-second catch-up safety net. The stream ends only on a database query failure or client disconnect.

See [API implementation map](api-reference.md) for each route/handler/service combination.

## Execution service: `execution.rs`

`QuoteRequest`, `QuoteClaims`, and `TradeRequest` define the external and signed shapes. `parse_micros` accepts only 1–16 ASCII digits and converts them to nonnegative `i64`.

### `Store::quote`

This read-only method loads the instance from the read-through cache (database on a miss), checks tradability against the cached clock estimate, resolves the outcome index, calls `amm::calculate`, builds a claim expiring at `min(now+15s, close)`, signs it, and returns exact/string and display values.

### `Store::execute`

The outer method validates the idempotency-key syntax, verifies quote signature/account/engine, parses the limit, hashes the complete trade body, and calls `execute_once`. PostgreSQL errors with SQLSTATE `40001`, `40P01`, or `55P03` are retried at most three total attempts with 20/40 ms backoff. Exhaustion becomes a conflict instructing an exact retry.

### `Store::execute_once`

This owns the trade transaction in five statements: claim idempotency (replaying a committed response on conflict), lock the instance while reading database time, lock both accounts while reading balances and the position, insert the transfer, then apply the remaining writes — position upsert, inventory/version update, trade insert, idempotency response, outbox event, and `pg_notify` — in one data-modifying CTE before committing.

The position read is not explicitly `FOR UPDATE`; serialization is provided by the instance lock. Do not remove or reorder that lock without redesigning position and inventory concurrency together.

## Persistence/read models: `store.rs`

### Shared helpers

- `db_now`: database wall time plus demo offset.
- `lock_accounts`: sorted `FOR NO KEY UPDATE` account locks.
- `balance`: reads one account balance inside an existing connection/transaction.
- `transfer`: inserts one ledger transfer; the database trigger updates balances.
- `event`: appends a durable outbox row and issues the commit-time `pg_notify` in the same statement.
- `audit`: appends an administrator audit row.
- `instance_view`: computes display prices and the effective public state/read model.

### Initialization

`Store::connect` creates a 32-connection pool with 5-second acquisition timeout, sets 2-second PostgreSQL lock timeout and 15-second statement timeout on each connection, applies migrations, and calls `initialize`.

`initialize` serializes on the singleton settings row, enforces mode consistency, creates a private simulation secret once, creates issuance/treasury accounts, and appends two idempotent issuance transfers: `initial-issuance` of 1,000,000 units and `issuance-expansion` of the remaining 999,000,000, raising total issuance to 1,000,000,000 units (`INITIAL_ISSUANCE_UNITS`, `TOTAL_ISSUANCE_UNITS`). In demo mode it also seeds the administrator account `demo-admin` (`admin@ntu.edu.sg`, password `admin`, role `admin`); the seed token's plaintext is discarded, so the password is the only way in.

### Accounts and instances

`account_for_token` hashes a bearer token up to 200 characters and returns only a user account. `create_account` validates the display name, generates a token/UUID, locks treasury and new account, checks budget, and transfers a 1,000-unit grant (`DEMO_GRANT_UNITS`); demo accounts carry no email, password, or role.

`register_account` validates the display name, a unique NTU email (`auth::valid_ntu_email`), and a password of at least 12 characters, inserts the account with role `member` and an argon2id hash, and transfers the 10,000-unit welcome gift (`WELCOME_GIFT_UNITS`).

`login` looks the account up by email, verifies the argon2 hash, and rotates the single session token: the new plaintext is returned once, the row's `token_hash` is replaced, and `evict_account` drops the previous token from the cache so it stops resolving immediately. Unknown emails verify against the fixed `LOGIN_TIMING_HASH` so unknown-email and wrong-password failures cost the same argon2 work and return the same `InvalidCredentials` error.

`create_verification_request` lets a member file one pending creator request under an account lock, `verification_request`/`verification_requests` read the requester's latest request and the administrator's filtered review list, and `decide_verification_request` approves or rejects under a row lock, permanently setting the role to `creator` on approval and always appending a `verification_decision` audit row. `account_is_admin` reads the role fresh on every admin request.

`instance`, `instance_detail`, `templates`, and `instances` construct public views. Detail includes the selected evidence payload. `create_instance` validates/derives the full definition, creates/funds a reserve, inserts the immutable instance, and appends the opened event; direct (non-series) creations also write an administrator audit row, while scheduler-spawned brackets publish through the outbox only.

`instance_history` (UC-19) rebuilds time-bucketed prices and volume for one instance by replaying up to 5,000 recorded trades from the opening inventory: the first point carries the opening prices, later points the price after the last fill in their bucket with the bucket's summed traded amount, and an untouched instance returns one flat point.

### Series and rolling spawn (ADR 0006)

`create_series` checks the data mode (creators publish manual series; simulated series stay demo-internal), validates the spec, requires the creator role under an account lock (platform seeding passes no creator), creates the series row plus a `templates` row under the series ID, and audits `create_series`. A one-time schedule immediately publishes its single bracket through `create_instance`; if that fails, the series row is removed again so nothing half-published remains.

`series_view` returns the definition plus up to 100 instances, newest close first, and computes the day view (UC-21) on request: per-bracket slot probabilities and volumes, plus the volume-weighted first-outcome probability across live brackets, null when no live bracket has traded. Nothing in the day view is stored. `series_list` orders active series first. `spawn_due_brackets` loads every active recurring series and calls `spawn_series_brackets` per series: it walks the grid slots strictly after now for up to `max_concurrency` slots, skips slots past `end_ms` or outside the active window, skips brackets that already exist, and publishes the rest through `create_instance` (a lost race with a competing scheduler is swallowed). Failures are logged per series and retried on the next tick. The same method then ends series: a recurring series whose end has passed with no non-terminal bracket left, and a one-time series once its single instance is terminal.

### Private reads and reconciliation

`portfolio` uses a repeatable-read, read-only transaction so account, positions, and claims form one consistent snapshot. The same page parameters apply to positions and claims. `trades` is a separate paginated query.

`reconcile` uses another repeatable-read snapshot to compare materialized balances with ledger entries, inventory arrays with position sums, reserve balances with remaining liabilities, and total balances with zero.

## Resolution service: `resolution.rs`

`ingest_evidence` invokes the shared recorder in manual mode. `record_evidence` validates limits and payload size, hashes the full input, locks the instance, handles exact duplicates, rejects/audits late evidence, enforces mode/time/source/window, evaluates the observation, inserts an immutable revision, selects the highest revision, and emits audit/event records. On the manual (non-simulator) path it also enforces the ADR 0007 exclusion: any instance whose series authority is not `admin` is rejected, so administrator evidence cannot resolve creator-signed or resolver-settled markets.

`record_creator_resolution` (ADR 0007) records a creator's signed human resolution: only the series creator may submit, the instance must be closed with the observation window ended and the deadline not passed, the ed25519 signature over `auth::resolution_message` must verify against the public key fixed at creation, and one resolution per instance is allowed. The verified outcome becomes evidence with source `creator-signature` (parser `creator-signature-v1`) and settles through the normal pipeline.

`record_resolver_evidence` (ADR 0007) records an external resolver's already-validated answer as evidence with source `external-resolver`, rejecting it once the result is final or the deadline has passed and treating a duplicate answer as a no-op.

`suspend` changes only an open instance, requires a meaningful reason, advances the version, and audits/emits the change.

`close_due` locks/skips up to 100 due instances, moves each to `closed`, advances versions, and appends closure events.

`settle_batch` performs result fixation and up to 100 account credits. Claims are inserted even when credit is zero, preserving “processed exactly once” semantics. After all claims, remaining reserve returns to treasury and the instance becomes terminal.

`advance_demo_clock` atomically increases the persisted offset, bounds one request and cumulative offset, and audits it.

## Resolver contract: `resolver.rs`

The external resolver contract (ADR 0007). `ResolverRequest` is the fixed request the settlement worker POSTs to a series' configured endpoint at the finalize window: instance and series identifiers, the bracket and observation window, and the typed rule. `parse_response` accepts exactly `{"outcome_id":"<published id>"}` or `{"pending":true}` and classifies anything else as invalid, including unknown outcome IDs, non-string values, and malformed JSON. `call` performs the POST with reqwest (rustls) under a 5-second timeout and rejects responses above 64 KiB; unreachable and oversized responses surface as errors the worker treats as missing evidence.

## Worker: `worker.rs`

`seed_demo` seeds the rolling demo bus series once (platform-owned, simulated, fee-free) and creates missing future occurrences for the six one-shot demo specs. A template/close uniqueness constraint handles competing schedulers; elections are deliberately not recurring.

`tick` closes due instances, spawns due series brackets, selects actionable rows, produces due simulator evidence, asks resolver-authority instances' external endpoints from their finalize window (ADR 0007: a valid answer becomes resolver evidence; pending, malformed, and unreachable sources retry on every tick until the published deadline voids the instance), settles independent instances concurrently in chunks of eight, and then seeds future demos. Per-series spawn failures and per-instance evidence/settlement errors are logged so one failure cannot stall the others.

`run` invokes `tick` every second with missed ticks skipped. There is no shutdown channel; `main` aborts the spawned task after HTTP shutdown.

## Caches: `cache.rs`

`Cache` holds the read-through instance map (2-second TTL, 10,000-entry capacity), the token-to-account map (50,000-entry capacity), and the demo clock offset behind an atomic. Token entries are immutable for the life of a session; `evict_account` drops the one token a login rotated out so it stops resolving immediately in this process. `apply_trade` updates a cached instance's inventory/version under a version guard so the in-process write-through and the notification listener can both apply the same committed trade idempotently; `invalidate` drops an entry whose state changed in ways the cache cannot reconstruct. `now_ms` combines the system clock with the cached offset.

## Event fan-out: `events.rs`

`OutboxEvent` mirrors one notification payload; `sse_data` reproduces the historical SSE data shape. `EventBus` lazily creates one bounded broadcast channel per instance and removes a channel once it has no receivers. `run_listener` owns a dedicated `LISTEN` connection, applies cache updates on every notification, publishes to the bus, and reconnects with a one-second delay after a failure. `run_clock_refresher` re-reads the demo clock offset every 30 seconds so a clock advanced by another process converges.

## Dependency direction

```text
main -> api, events, store, worker
api -> execution/store/resolution/worker through Store methods
execution -> amm, auth, fee, events channel constant, market, store helpers
resolution -> auth, fee, market, store helpers
resolver -> error, market
worker -> market, resolver, resolution/store methods
store -> amm, auth, cache, events channel constant, market
events -> cache, store
cache -> market
market -> amm validation/funding
amm/auth/error/fee -> no database service dependencies
```

Keeping `amm` pure and category accounting-free is the main extension boundary.

