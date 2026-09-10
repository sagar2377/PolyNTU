PolyNTU Rust rebuild verification, started 8 September 2026.

The verification host is Windows on an Intel Core Ultra 9 185H (16 cores, 22 logical processors), with 31.4 GiB usable RAM. Rust/Cargo 1.88, PostgreSQL 17.6, and Node 25.7 were used. PostgreSQL is colocated on loopback port 55432: `fsync`, `synchronous_commit` and `full_page_writes` were verified on, with `max_connections=100`. The Rust service uses one process, a Tokio runtime, and a pool capped at 24 connections. This is a development laptop measurement, not the original proposal's 4-vCPU/8-GB environment.

| Check | Result |
|---|---|
| Rust library tests | 6 passed, including 128 generated property-test cases. |
| Independent numerical reference | 384 rounded results matched Python Decimal at 80-digit precision, including inventory/concentration boundaries and fractional-share trades. |
| PostgreSQL integration tests | 13 passed, each using its own UUID-named database. |
| Formatting and static analysis | `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` passed. |
| React frontend | `npm run lint` and `npm run build` passed. |
| Release executable | Built successfully with locked dependencies and overflow checks enabled. |
| Legacy database | Original SQLite file and preserved backup have identical SHA-256 hashes. |
| Actual HTTP/static delivery | Rust health endpoint, HTML and generated JavaScript returned 200; the local demo listed seven templates and all five categories. |

The release engine microbenchmark (10,000 iterations per outcome count, buying 10 shares of one outcome at `b=100`) measured 18.25 microseconds with 2 outcomes, 19.95 with 3, and 24.88 with 8. This includes calculation and allocation but no authentication, serialization or database work. It is a single-run microbenchmark; HTTP results are the relevant user-facing measurement.

Integration coverage includes the seven demo templates/five categories through HTTP; buy and sell execution; ownership, expiry and price limits; pure quotes; duplicate idempotency requests and stored receipts after reopening the store; same-version competition; cross-market overspending; waiting past close while blocked on a market lock; rollback after an injected trade failure; source deduplication and out-of-order revisions; missing-data categorical void rounding; rollback after an injected settlement failure; and resuming after a committed 100-account settlement batch. Every database test ends with ledger/inventory/reserve reconciliation.

The worker fairness regression creates 101 closed markets awaiting evidence and a newer market with a complete result. The ready market resolves without waiting for the earlier markets' missing-data deadlines. The same test rejects unrecognized simulator source IDs.

The first load run found a real lock-upgrade deadlock: foreign-key checks on concurrent idempotency inserts acquired account `KEY SHARE` locks before the application requested `FOR UPDATE`. Account balance writes now use `FOR NO KEY UPDATE` in application locking and the ledger trigger. A dedicated regression test holds the competing foreign-key lock while requiring another trade to complete.

The first completed ten-minute run had zero unexpected errors and zero reconciliation discrepancies, but Windows timer granularity delivered about 65 dedicated quotes and 18 trade attempts per second. Its p95 HTTP latencies were 5.6–7.3 ms for quotes and 17.8–24.2 ms for committed trades. The load generator was corrected to schedule arrivals by elapsed time instead of assuming every ten-millisecond timer fires on time.

A subsequent full-rate run overlapped with a release/microbenchmark compilation. The client skipped 330 scheduled quotes when its 200-request in-flight cap was reached. That run failed the workload check despite zero server errors and successful reconciliation. Its later hot phase completed 30,000 dedicated quotes and 5,999 trades with one stale-quote conflict, but the whole run is not counted as a clean baseline. The [contention report](benchmarks/2026-09-09-build-contention.json) retains those results. The final verification separates compilation from traffic generation.

The final [ten-minute HTTP/SSE run](benchmarks/2026-09-09-http.json) passed with 100 open instances, 200 accounts, 10,000 seeded positions and 200 event streams. It submitted **59,999 dedicated quotes and 11,999 trade workflows in 600.02 seconds**. All dedicated quotes succeeded; 11,978 trades committed, while 21 hot-market requests received expected stale-quote conflicts. There were **zero unexpected errors, skipped requests, or reconciliation discrepancies**. Each workflow also requested its own preview, so the total quote rate was approximately 120/s; the dedicated quote rate was 100/s and trade-attempt rate was 20/s.

| Five-minute phase / request | Successful requests | p50 HTTP ms | p95 HTTP ms | p99 HTTP ms |
|---|---:|---:|---:|---:|
| Spread / quotes | 29,999 | 1.59 | 2.47 | 3.98 |
| Spread / committed trades | 5,999 | 2.61 | 5.05 | 14.48 |
| 80% on one market / quotes | 30,000 | 1.53 | 2.35 | 3.52 |
| 80% on one market / committed trades | 5,979 | 2.52 | 4.47 | 16.51 |

These HTTP results are within the proposed quote/trade latency targets on this host. They do not establish the same capacity on the proposal's smaller machine or on a campus network. The run used the final release build without concurrent compilation. The separate local demo remained running with its normal background scheduler.

The independent [settlement workload](benchmarks/2026-09-09-settlement.json) completed **10,000 claims across 50 instances in 7.64 seconds**, measured from the start of uploading evidence until all instances reported resolved. This includes the finalization wait and completion polling at 500-ms intervals. All 10,000 individual portfolio credits matched the expected one-unit payment, and ledger/inventory/reserve reconciliation passed. While claims were processed, 200 SSE clients stayed connected and two separate instances continued trading.

| During settlement | Successful requests | p50 HTTP ms | p95 HTTP ms | p99 HTTP ms |
|---|---:|---:|---:|---:|
| Dedicated quotes | 764 | 1.76 | 2.40 | 3.61 |
| Committed trades | 153 | 4.28 | 60.45 | 74.41 |

The settlement interval sustained approximately 100 dedicated quotes and 20 committed trades per second. It had zero conflicts, unexpected errors or invalid credits. This is one measured 10,000-claim run; its short latency sample is not a saturation or long-duration reliability result.

Measurements include client HTTP round-trip and JSON parsing. Trade latency is the submitted execution request through its committed response; it excludes the preceding preview and user think time. Quote samples refer to dedicated quote traffic, while each trade workflow issues an additional preview. Expected stale-quote conflicts are counted separately from committed trades. The scripts retain submitted counts, dropped work and unexpected errors so rejected work cannot inflate throughput claims.

The original proposal also included campus-network visible confirmation, event propagation percentiles, saturation testing, profiling attribution, and a fixed 4-vCPU/8-GB environment. Those have not been established by this local verification. There is no measured comparison with a Python implementation of the same new mechanism, so these results do not support a Rust-versus-Python speedup ratio. Numerical fixtures provide evidence over tested inputs, not a formal proof of every transcendental rounding boundary.

The project prohibits browser/computer automation. The interface was checked through source review, lint/build and HTTP delivery, without visual browser inspection. The supplied Docker/Compose configuration and GitHub Actions workflow have not been run on this host. Actual live weather, bus, election, crowd and attendance feeds are not configured; account recovery, campus SSO, and operational load limits remain separate integration work.
