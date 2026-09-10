PolyNTU Rust architecture (v0.2).

The active application is a Rust HTTP service, a PostgreSQL database, and the existing React frontend. The service runs a background worker from the same codebase. Each concrete market instance has its own outcomes, AMM inventory, funded reserve, observation window, and final result. Templates group recurring instances for browsing and scheduling.

```text
React UI -> Axum API -> execution / resolution -> PostgreSQL transaction
                         |                       accounts + ledger
                         +-> pure LMSR engine    positions + trades
                                                 instances + evidence
Demo worker -> normalized evidence -> resolution  claims + outbox
React EventSource <----------- persisted public market events
```

| Path | Responsibility |
|---|---|
| `backend/src/amm.rs` | Pure Decimal LMSR arithmetic; share/credit scales; price and inventory limits; funding bound. No network, clock, RNG, or database access. |
| `backend/src/market.rs` | Typed market rules, observations, validation, outcome evaluation, deterministic demo definitions. |
| `backend/src/execution.rs` | Signed quote claims, price limits, idempotency, transaction locking, execution and receipts. |
| `backend/src/store.rs` | PostgreSQL setup, account grants, instance creation, public views, portfolios and reconciliation. |
| `backend/src/resolution.rs` | Source revisions, evidence validation, cutoff transitions, settlement batches, suspension and demo time. |
| `backend/src/worker.rs` | Close due instances, produce due demo observations, retry settlement and schedule recurring demos. |
| `backend/src/auth.rs` | Random account credentials, token hashing, quote signatures, constant-time admin-token verification. |
| `backend/src/api.rs` | Versioned HTTP routing, authentication/authorization, bounded payloads, CORS and SSE. |
| `backend/migrations/` | Versioned schema and database constraints/triggers. Applied automatically at startup. |
| `backend/queries/` | Auditable SQL that benefits from a standalone file. |
| `backend/tests/` | Numerical reference fixtures and isolated PostgreSQL integration tests. |
| `frontend/src/` | Market discovery, outcome trading, account access, positions and receipt recovery. |
| `scripts/` | Development helpers, fixture generation and the HTTP benchmark. |
| `legacy/python-backend/` | Archived options prototype, excluded from the active application. |

An account owns a random bearer token; only its SHA-256 hash is stored in PostgreSQL. Public demo enrollment is enabled only in explicit demo mode, and the executable requires a loopback bind in that mode. Non-demo account provisioning uses an administrator token. This is basic account isolation, not campus SSO: credential rotation, recovery, enrollment limits, and institutional identity integration are future work.

Every credit transfer stores a source, destination and amount. A database trigger applies both balance changes in the same transaction; the `ledger_entries` view exposes the corresponding debit and credit. Transfers, trades, evidence, settlement claims, and administrator audit records are append-only. Published instance definitions and final outcomes are protected by a database trigger. The initial treasury allocation is one million simulated units; new accounts receive 1,000 units from that finite budget.

Trade lock order is idempotency record, instance row, account rows sorted by ID, then positions. Account rows use `FOR NO KEY UPDATE`: balance writes still serialize, while foreign-key `KEY SHARE` locks from concurrent idempotency inserts remain compatible. Using `FOR UPDATE` here caused lock-upgrade deadlocks during the first HTTP load run; migration 0003 and a targeted regression test address that interaction. A trade rechecks the database clock after waiting for locks. Inventory, balances, position, receipt, and outbox event commit together. Transient PostgreSQL deadlocks/serialization/lock-timeout errors have a maximum of three attempts; the client retains the same idempotency key across connection errors.

Settlement freezes the latest authoritative source revision after the grace period, then credits up to 100 accounts in one transaction. Claims are unique per instance/account. Committed batches survive restarts. Unused reserve returns to the treasury only when every claim is accounted for. Reconciliation compares account balances against ledger entries, positions against inventory, and remaining obligations against reserves using the final result where one exists.

The worker selects actionable instances: due simulator observations, results ready for finalization, expired evidence deadlines, and unfinished settlement. Closed markets still waiting for evidence cannot occupy every batch slot and delay unrelated ready markets. Simulator source IDs are validated at creation; a failed observation still permits the published missing-data void path.

The worker and API can run in several processes with the same database, clock mode, and signing secrets. PostgreSQL row locks and uniqueness constraints coordinate them. SSE polls a durable per-instance outbox; it can resume from `Last-Event-ID`. Since the instance lock serializes event-producing writes, sequence IDs are ordered by commit within that instance. The UI also refreshes snapshots periodically. Outbox retention/compaction is not implemented yet.

Demo observations use a private database-persisted random seed combined with the instance ID. This keeps replay deterministic across restarts without making future results computable from the public ID. Observations are generated only after their window. The seed never appears in public APIs; this is demonstration data, not a forecasting model or a live feed.

The implementation deliberately uses share-quantity entry for v1. Budget-to-quantity inversion, resting orders, a separate forecasting model, native Python bindings, live external-feed clients, general draft editing, and a public deployment are not part of this rebuild. The engine and normalized evidence boundary support adding them without replacing accounting.
