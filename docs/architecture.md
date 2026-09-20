# PolyNTU architecture

Status: **current**  
Application version: **0.2**  
Last reviewed against the working tree: **20 September 2026**

## System context

PolyNTU is one deployable application backed by PostgreSQL. The Rust executable serves the versioned JSON API, Server-Sent Events, and the built React files. It also starts a periodic background worker from the same codebase.

```text
Participant browser
   |  HTTP JSON, bearer token, idempotency key
   |  public SSE stream
   v
Axum API ---------------------------------------------------+
   |                                                        |
   +--> auth/signatures                                     |
   +--> pure LMSR calculation                               |
   +--> trade execution                                     |
   +--> evidence/resolution                                 |
   |                                                        |
   v                                                        |
PostgreSQL <-------- periodic worker -----------------------+
   |                     |
   |                     +--> external resolver endpoints
   |                          (resolver-authority series, ADR 0007)
   +-- accounts and balanced ledger transfers
   +-- templates, instances, outcomes, and inventory
   +-- positions, trades, and idempotency receipts
   +-- evidence, claims, administrator audit, and outbox
```

Market categories provide typed rules and evidence evaluation only. They do not receive network access, calculate prices, authenticate users, or move balances.

## Runtime startup

`backend/src/main.rs` performs startup in this order:

1. Configure tracing from `RUST_LOG` or the default filter.
2. read and validate the database URL, quote secret, administrator token, demo mode, and bind address;
3. reject a non-loopback bind when demo mode is enabled;
4. connect to PostgreSQL, apply embedded SQLx migrations, and initialize fixed accounts/settings;
5. construct `AppState`, which rejects short or identical secrets;
6. seed demo instances when demo mode is enabled;
7. build the API router, CORS layer, request-size limit, tracing layer, and static-file fallback;
8. spawn the background worker; and
9. serve until Ctrl+C, then abort the worker task after Axum shuts down.

The executable contains the backend only. The Docker build compiles React separately and copies `frontend/dist` into the final image.

## Active source modules

| File | Responsibility and boundaries |
|---|---|
| `backend/src/lib.rs` | Exposes active modules and forbids `unsafe` code in the application crate. |
| `backend/src/main.rs` | Process configuration, database connection, router construction, worker startup, static delivery, and graceful HTTP shutdown. |
| `backend/src/error.rs` | Shared error categories and conversion to the API error envelope. Internal/database details are logged but hidden from clients. |
| `backend/src/auth.rs` | Random bearer tokens, SHA-256 hashing, HMAC-SHA256 signed values, token verification, minimum secret validation, constant-time administrator-token comparison, and ed25519 resolution-signature verification (ADR 0007). |
| `backend/src/amm.rs` | Pure LMSR arithmetic, input bounds, funding calculation, prices, and rounded trade amounts. It has no database, clock, network, account, or RNG access. |
| `backend/src/market.rs` | Outcomes, typed rules and observations, validation, evaluation, instance and series types, schedules, the resolution authority spec, and demo specifications. |
| `backend/src/execution.rs` | Quote structures, signing claims, amount parsing, idempotency, transaction locking, execution checks, ledger movement, positions, trades, receipts, and trade outbox events. |
| `backend/src/store.rs` | Pool setup, migrations, bootstrap, database time, account provisioning, instance and series creation/read models, rolling bracket spawn, terminal-bracket retention purge, portfolio and trade history queries, bucketed instance history and the series day view, transfers, audit/events, and reconciliation. |
| `backend/src/cache.rs` | Read-through instance and token caches with a local clock estimate; every mutation invalidates or writes through before returning. |
| `backend/src/events.rs` | PostgreSQL notification listener, per-instance SSE broadcast channels, and demo clock refresh. |
| `backend/src/resolution.rs` | Evidence validation/deduplication, creator-signed and resolver evidence recording, suspensions, due-market closing, finalization, bounded settlement, reserve release, and demo clock changes. |
| `backend/src/resolver.rs` | The external resolver contract (ADR 0007): the fixed request, response parsing against the published options, and the bounded HTTPS call. |
| `backend/src/worker.rs` | One-second scheduling loop, rolling series bracket spawn, fair actionable-instance selection, private deterministic simulator evidence, external resolver calls, concurrent bounded settlement, terminal-bracket retention purge, and demo seeding (including the rolling bus series). |

See [backend code reference](developer/backend.md) for important types and functions in each file.

## Data and responsibility boundaries

### Browser

The browser owns navigation, presentation, local input checks, polling, SSE-triggered refreshes, and recovery of an uncertain request. It does not decide price validity, market state, ownership, balance sufficiency, settlement, or evidence correctness.

### API and services

Axum authenticates requests and applies the 64 KiB body limit. Service methods validate domain inputs and own transaction boundaries. Every state-changing decision is rechecked against database state.

### PostgreSQL

PostgreSQL is the durable authority for time-adjusted cutoffs, accounts, ledger balances, inventory, positions, idempotency responses, evidence, results, claims, events, and audit records. Constraints and triggers protect invariants even when a future code path is added incorrectly.

### External evidence adapters

No live adapter is present. A future adapter either normalizes provider data into `EvidenceInput` and submits it through the administrator route, or, for a resolver-authority series, implements the external resolver contract and answers the worker's fixed request with a published outcome ID ([ADR 0007](decisions/0007-resolution-authority.md)). Provider access and reliability are acknowledged challenges, but the current design assumes a suitable data source can be obtained; the real bus timing adapter remains the deferred example.

## Authoritative time

`db_now` reads PostgreSQL `clock_timestamp()` and adds the persisted demo offset from the singleton settings row. Quote expiry, market close, evidence acceptance, finalization, and audit timestamps use that value. Browser time only drives display formatting and a locally adjusted countdown.

Demo mode persists its offset and limits the cumulative offset to ten years. A single advance request accepts 1–10,080 minutes. Production and demo processes must use separate databases; startup rejects a stored mode mismatch.

## Transaction boundaries

### Quote

A quote performs reads only, served from a read-through instance cache and a local estimate of the database clock, so a cache hit costs no database round trips:

1. load the instance from the cache, fetching it from PostgreSQL on a miss;
2. read the cached clock estimate;
3. confirm it is open, before its close, and not suspended;
4. map the outcome ID to its fixed index;
5. calculate the bounded LMSR amount and before/after prices; and
6. sign account, instance, outcome, side, quantity, amount, version, expiry, and engine version.

It does not reserve inventory or write an idempotency row. The cache is safe because quotes are previews: execution re-validates every claim inside its transaction, so a stale entry can only produce a quote that execution rejects. Mutations invalidate or write through the cache before returning, committed trades update inventory and version in place, and each process's notification listener applies the same updates for writes made by other processes. Entries expire after two seconds regardless. Token-to-account resolution is cached the same way; that mapping is immutable, and balances are always read fresh when they matter.

### Trade

One PostgreSQL transaction owns the entire trade. The actual ordering is:

1. insert the account-scoped idempotency claim if absent, replaying any committed response on conflict;
2. lock the instance row with `FOR UPDATE`, then read the authoritative clock in a separate statement, so a request that waited on the lock rechecks its close time against a fresh clock;
3. lock the participant and reserve account rows in sorted ID order using `FOR NO KEY UPDATE`, reading both balances and the current position in the same statement;
4. recheck state, cutoff, quote expiry, version, signed amount, user limit, balance, holdings, and reserve coverage;
5. insert a balanced transfer, whose trigger updates both account balances; and
6. apply the remaining writes — position upsert, inventory/version update, immutable trade, idempotency response, outbox event, and the commit-time notification — in one data-modifying common table expression, then commit before returning the receipt.

There is no separate explicit position-row lock. The instance lock serializes all trades that could modify positions or inventory for that instance. The participant account lock separately serializes spending across different markets. This distinction is important when changing the locking design.

Foreign-key checks can hold `KEY SHARE` locks on accounts during idempotency and position writes. `FOR NO KEY UPDATE` remains compatible with those locks while still serializing balance updates. PostgreSQL deadlock, serialization, and lock-timeout errors (`40P01`, `40001`, and `55P03`) receive up to three attempts with short backoff. Clients must keep the same idempotency key when retrying an uncertain request.

### Instance creation

Instance creation validates the complete immutable specification, derives category and outcomes from the rule, calculates required funding, creates the reserve account, transfers its subsidy from the treasury, inserts the instance, and writes an `opened` event in one transaction. Direct (non-series) creations also record an administrator audit; scheduler-spawned brackets publish through the outbox event only.

Series publication validates the schedule, requires the creator role under an account lock, inserts the immutable series row plus a template row under the series ID, and audits the creation. A one-time schedule immediately publishes its single bracket through instance creation; if funding fails, the series row is removed again.

### Evidence

Evidence ingestion locks the instance, validates the source/window/state/deadline, checks duplicate event content and source revisions, evaluates the typed observation, appends the record, points the instance to the highest revision, advances its version, audits the receipt, and appends an event in one transaction. Manual evidence is rejected outright for series whose resolution authority is not `admin` (ADR 0007). Creator-signed resolutions and validated resolver answers enter through their own recorders under the same instance lock; neither runs rule evaluation, because the authority itself names the outcome.

### Settlement

`settle_batch` locks one instance. Once eligible, it fixes the latest complete evaluated result or applies the deadline void policy. It then selects at most 100 unsettled participant accounts, locks them with the reserve and treasury, credits unique claims, and commits the batch. When no claims remain, it returns unused reserve to the treasury and marks the instance `resolved` or `voided`.

## Lifecycle and immutability

The persisted transitions are:

```text
open -> closed -> resolving -> resolved
                           \-> voided
```

Migration `0002_immutable_rules.sql` rejects changes to published template/category/title/rule/outcomes/source/data mode/times/liquidity/reserve fields. It requires each instance update to advance its version exactly once, forbids inventory changes after closing, and prevents updates to terminal instances or fixed results. Migration `0009_market_series.sql` extends the protected set with the fee flag, creator attribution, series membership, and bracket slot, and applies the same immutability to series definitions, whose only mutable field is the lifecycle state. Migration `0010_resolution_authority.sql` adds the resolution authority, public key, and resolver endpoint to the protected series definition (ADR 0007).

Ledger transfers, trades, evidence, settlement claims, and administrator audit records are append-only through database triggers; the single sanctioned delete path is the bracket retention purge described below. Idempotency rows are intentionally updated once with the durable response.

## Data retention

One rule covers the current retention behaviour: terminal brackets of recurring series are disposable after a day, everything else stays.

- Deleted: a recurring-series bracket in state `resolved` or `voided` whose evidence deadline passed more than 24 hours ago is purged with its whole subtree (outbox rows, positions, trades, settlement claims, evidence, and the instance) in batches of 20 per worker tick. One-time markets are never purged.
- Kept forever: the append-only accounting history. Every ledger transfer is shared between two accounts, so drained reserve accounts and their ledger trails survive every purge; admin audit and idempotency rows survive too, since neither references instances by foreign key.
- A reserve that still holds units blocks the purge, so a settlement anomaly surfaces instead of vanishing.
- The only delete path through the append-only triggers is this purge, which runs inside a transaction setting the session-local `polyntu.purge = 'on'` flag (migration `0011_bracket_retention.sql`); every other update or delete keeps raising.

## Worker scheduling and fairness

The worker ticks every second and skips accumulated timer ticks. Each cycle:

1. persists closure for up to 100 due open instances using `FOR UPDATE SKIP LOCKED`;
2. spawns due brackets for every active recurring series, keeping each rolling horizon filled inside its active window and ending finished series;
3. selects up to 100 actionable instances rather than simply the oldest closed rows;
4. generates evidence for due simulated instances using a hash of a private database secret and instance ID;
5. asks resolver-authority instances' configured endpoints from their finalize window (ADR 0007): a valid answer is recorded as resolver evidence, while pending, malformed, and unreachable sources retry on later ticks until the published deadline voids the instance;
6. settles independent instances concurrently in bounded chunks of eight, logging an error without ending the worker;
7. purges terminal recurring brackets past the retention window in batches of 20, logging an error without ending the worker; and
8. seeds the rolling demo bus series once and ensures one future demo occurrence exists for each recurring demo template and one election occurrence overall.

The actionable query prevents many markets waiting for evidence from starving a later market that is ready to resolve; it also admits resolver-authority instances from their finalize window so the worker can ask their external source. Each instance still settles serially under its row lock; concurrency is across instances only.

## Events and consistency

State-changing instance transactions append an `outbox` row and issue a `pg_notify` on the same channel in the same statement, so the notification is delivered exactly when the transaction commits. A dedicated listener connection in each process receives these notifications and fans them out to connected SSE subscribers through per-instance broadcast channels. Each stream first replays the durable outbox by global sequence ID — accepting `Last-Event-ID` or `?after=` for resumption — and re-checks it on a slow interval, so a lost or lagged notification delays an event but never drops it. The UI treats an event as a signal to refetch the current snapshot; it also polls periodically.

Events may be delivered more than once and are not a replacement for snapshots. Outbox rows are deleted only by the bracket retention purge; no general compaction exists, so operators must still monitor outbox growth from one-time markets before public deployment.

## Concurrency and multicore use

The server runs Tokio's multi-thread runtime, one worker thread per core, so independent requests — including cache-served quotes, trades on different markets, evidence, and settlement batches — execute in parallel. Two serialisation points remain by design: the instance row lock orders trades within one market, and the PostgreSQL commit provides the durability guarantee for acknowledged trades. Quotes are CPU-bound after caching and scale with cores; trade throughput across markets is bounded by the connection pool (32) and commit latency.

## Multi-process behaviour

Several API/worker processes may share one database if they use the same mode and quote secret. Row locks and uniqueness constraints coordinate competing trades, evidence, claims, and demo schedules. An administrator token must also be consistent wherever administrator requests may land. The project has not established production orchestration, leader election, connection-budget sizing, or horizontal-load results.

## Recovery model

- A committed trade is recoverable by repeating its exact body and idempotency key.
- An uncommitted transaction has no partial ledger, position, trade, receipt, or event effects.
- Evidence records and final results survive worker restarts.
- Settlement continues from accounts without an existing claim.
- Reconciliation detects divergence between materialized balances and the ledger, positions and inventory, or reserve and remaining obligations.
- A migration or deployment failure must be recovered forward against the same PostgreSQL database; the legacy SQLite file is never a valid v2 rollback database.

Operational procedures are in [operations](developer/operations.md).

