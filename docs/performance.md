# Performance design

How the current code keeps throughput high without weakening any accounting invariant. Every claim below names the code that implements it. Measured results and their limits are in [verification](verification.md); this document explains the design decisions behind them.

## The shape of the workload

Two paths carry almost all the load and they have opposite needs:

- **Quotes** are read-only previews. A trader asks for a price, sees it, and may or may not confirm. Most quotes never become trades, so this path must be cheap and must not contend.
- **Trades** move money. They must be exactly-once, must not overdraw an account, and must not let two traders corrupt a market's inventory.

The design splits them accordingly: quotes are served with no locking and no database round trips, and trades take a short, ordered set of row locks. Everything else — settlement, bracket spawn, retention — runs in the background worker on its own schedule.

## Quote path: no database, no locks

`Store::quote` in [execution.rs](../backend/src/execution.rs) touches the database only when the read-through cache misses:

1. `instance_cached` serves the instance from an in-process map (2-second TTL, 10,000 entries) and falls back to one `SELECT` on a miss;
2. `now_cached` reads the clock from an atomic holding the database offset, so no time query is issued;
3. the LMSR amount and prices are pure arithmetic in [amm.rs](../backend/src/amm.rs) — no database, clock, network, or RNG access;
4. the claim is HMAC-SHA256 signed in [auth.rs](../backend/src/auth.rs) and returned.

A quote therefore costs one cache lookup, one decimal calculation, and one HMAC. It takes no lock and reserves no inventory, so any number of quotes run concurrently on any market. The cache is safe to be stale because execution re-validates every signed claim inside its transaction: a stale quote can only produce one that execution rejects, never a wrong price.

The one exception is a sell, which reads the trader's own holding to reject an oversell before quoting it. That read is a single indexed row lookup and does not lock.

## Trade path: one lock per market, fixed order

`Store::execute_once` runs one transaction, described step by step in [architecture](architecture.md#transaction-boundaries). The performance-relevant decisions:

- **The instance row lock is the only serialisation point for a market.** Trades on different markets never contend. This is what makes cross-market throughput scale: the benchmark's 20 markets are 20 independent lock domains.
- **Accounts are locked in sorted ID order** by `lock_accounts` in [store.rs](../backend/src/store.rs), which is what prevents deadlock cycles between a trade and a settlement that hold locks in different orders.
- **`FOR NO KEY UPDATE` instead of `FOR UPDATE`** on accounts, because foreign-key checks take `KEY SHARE` locks during the idempotency and position inserts; a stronger mode would create lock-upgrade cycles with them.
- **Six statements, with all remaining writes in one CTE.** `FINALIZE_TRADE` applies the inventory/version update, position upsert, trade insert, idempotency response, outbox row, and `pg_notify` in a single round trip. The comment in the source notes this halves the statement count of the execution path.
- **Idempotency is a claim, not a check.** The first statement inserts the key and replays a committed response on conflict, so a retried request costs one statement and never reaches the locking path.
- **Lock timeouts are bounded per connection** (`lock_timeout=2s`, `statement_timeout=15s`), so a pathological wait surfaces as a retryable error instead of a hung request.
- **Deadlocks are retried** up to three attempts with 20/40 ms backoff on `40001`, `40P01`, and `55P03`.

The cost accepted here is that all trades in one market are serial. See [why the lock is per market](architecture.md#why-the-lock-is-per-market-and-not-per-outcome) for why that is the right trade for this workload.

## Worker: bounded, non-blocking batches

`worker::tick` in [worker.rs](../backend/src/worker.rs) runs on a one-second interval and never holds the request path:

- **Closure uses `FOR UPDATE SKIP LOCKED`** with `LIMIT 100`, so a competing process's rows are skipped rather than waited on.
- **Settlement splits by market instance**, up to `POLYNTU_SETTLE_CONCURRENCY` (default 8) at a time, each in its own task. The unit matters: splitting by account or claim would break the ordering a market's row lock provides.
- **Settlement batches at 100 accounts per transaction**, which keeps each lock hold short instead of claiming 10,000 accounts in one transaction.
- **The actionable query is not "oldest closed first".** It selects instances whose evidence is ready, so a backlog of markets waiting for evidence cannot starve one that can settle now.
- **Every step logs and continues.** A failed purge or a failed settlement task does not end the worker or delay the next tick.

## Event delivery: push, with a durable fallback

Every state-changing transaction writes an outbox row and issues `pg_notify` in the same statement, so the notification is emitted exactly at commit. The listener in [events.rs](../backend/src/events.rs) fans each notification to per-instance broadcast channels, and each SSE stream is its own task.

Two properties matter for performance:

- **Committed trades carry the new inventory in the notification payload**, so the cache updates without a read. The trade that caused the change does not wait for its subscribers.
- **Streams replay from the outbox by sequence ID**, so a dropped notification costs a catch-up query, not a lost event. The 30-second safety pass means a lagged receiver recovers without polling the database per event.

## Concurrency limits are deployment settings

The pool size and settlement concurrency are read from `POLYNTU_POOL_CONNECTIONS` (default 32) and `POLYNTU_SETTLE_CONCURRENCY` (default 8) in [main.rs](../backend/src/main.rs). `Tuning::normalized` clamps settlement to at most one below the pool size, so settlement can never take every connection and stall request handling. A malformed value is a startup error, not a silent default.

## What is deliberately not optimised

- **No per-outcome locking.** LMSR prices every outcome from the whole inventory vector, so per-outcome locks would be a lost-update bug, not an optimisation.
- **No position-row locks.** The instance lock already serialises every write that can touch a market's positions; an extra lock would only add contention.
- **No quote reservation.** Reserving inventory at quote time would turn previews into contention, and the signed-claim revalidation already makes stale quotes harmless.
- **No read replicas or connection multiplexing.** The workload is one process against one database; the pool and the instance cache cover it.
- **No general compaction.** Outbox and idempotency rows are kept, which costs storage rather than throughput.

## Where to look when it is slow

| Symptom | First place to look |
|---|---|
| Quote latency rising with load | cache miss rate: check that instance reads hit `instance_cached`, not `instance` |
| Trade latency rising with load | lock waits on the instance row; a hot market serialises by design |
| Trade throughput flat while adding markets | pool exhaustion; raise `POLYNTU_POOL_CONNECTIONS` |
| Settlement falling behind | `POLYNTU_SETTLE_CONCURRENCY`, and whether `tick_lock` is contended by on-demand ticks |
| Request timeouts under settlement load | settlement is taking too much of the pool; lower `POLYNTU_SETTLE_CONCURRENCY` |
