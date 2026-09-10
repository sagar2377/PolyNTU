# PolyNTU verification record

Status: **retained local evidence; reproducible commands documented**  
Verification began: **8 September 2026**

## Evidence policy

Verification claims must identify the command or workload, environment, result, and retained artifact when one exists. A passing test proves the tested behaviour under that environment; it is not a proof of every input, deployment, network, or provider.

The benchmark JSON files are retained in `docs/benchmarks/`. Build, lint, and test pass statements were recorded by the development agent but their complete raw console logs were not committed. Human reviewers can reproduce them with [development commands](development.md).

## Verification environment

| Item | Recorded value |
|---|---|
| Host | Windows development laptop |
| CPU | Intel Core Ultra 9 185H; 16 cores, 22 logical processors |
| Usable memory | 31.4 GiB |
| Rust/Cargo | 1.88 |
| PostgreSQL | 17.6 on loopback port 55432 |
| Node.js | 25.7 |
| PostgreSQL durability | `fsync`, `synchronous_commit`, and `full_page_writes` enabled |
| PostgreSQL connections | `max_connections=100` |
| Application | One release process, Tokio runtime, pool capped at 24 |

This differs from the original proposed 4-vCPU/8-GB environment. Results must not be presented as measured capacity for that smaller target or a campus network.

## Test and static-analysis results

| Check | Recorded result | Reproduction source |
|---|---:|---|
| Rust library tests | 6 passed | `backend/src/amm.rs`, `backend/src/market.rs` |
| Property cases inside the LMSR property test | 128 generated cases | Proptest configuration in `amm.rs` |
| Independent numerical test | 1 passed over 384 fixtures | `backend/tests/numerical.rs` and fixture JSON |
| PostgreSQL integration tests | 13 passed | `backend/tests/integration.rs` |
| Rust formatting | Passed | `cargo fmt --check` |
| Rust clippy | Passed with warnings denied | `cargo clippy --all-targets -- -D warnings` |
| React lint/build | Passed | `npm run lint`, `npm run build` |
| Release build | Passed with locked dependencies and overflow checks | Cargo release profile |
| Legacy database preservation | Matching SHA-256 | Original and `.local` backup |
| HTTP/static delivery | Health, HTML, and generated JavaScript returned 200 | Recorded manual HTTP check; no raw log retained |

The 20 Rust test functions are six library tests, one numerical test, and thirteen integration tests. The 384 fixtures use Python Decimal at 80-digit precision and include random, high-inventory, smallest-quantity, and concentration-edge cases.

## Behaviour covered by integration tests

- all seven demo templates and five categories through HTTP;
- account/admin authorization and retired endpoint behaviour;
- buy, sell, ownership, balance, cost/proceeds limits, expiry, and pure quotes;
- concurrent duplicate delivery and receipt recovery after reopening the store;
- same-version competition and cross-market overspending;
- trading cutoff after waiting for an instance lock;
- rollback after an injected trade failure;
- foreign-key `KEY SHARE` compatibility with account balance locks;
- evidence deduplication, out-of-order revisions, finalization, and late rejection;
- missing-evidence categorical void rounding;
- rollback/retry during settlement and continuation after a committed 100-account batch;
- worker fairness when 101 earlier markets are waiting for evidence; and
- reconciliation after every database test.

See [testing reference](developer/testing-and-verification.md) for test-to-invariant mapping.

## Engine microbenchmark

The recorded release microbenchmark ran 10,000 calculations for a 10-share buy at `b=100`:

| Outcomes | Recorded mean per calculation |
|---:|---:|
| 2 | 18.25 microseconds |
| 3 | 19.95 microseconds |
| 8 | 24.88 microseconds |

It excludes authentication, serialization, network, database, locking, and transaction work. It is useful only for detecting large arithmetic regressions; HTTP measurements better represent user-facing latency.

## Ten-minute HTTP and SSE workload

Raw artifact: [`benchmarks/2026-09-09-http.json`](benchmarks/2026-09-09-http.json)

The run used 100 open instances, 200 accounts, 10,000 seeded positions, and 200 SSE clients. Over 600.02 seconds it submitted 59,999 dedicated quotes and 11,999 trade workflows. All dedicated quotes succeeded; 11,978 trades committed and 21 concentrated-market attempts returned expected stale-version conflicts. There were zero unexpected errors, skipped requests, or reconciliation discrepancies.

Each trade workflow performs a separate preview, so total quote traffic was approximately 120/s while dedicated quote traffic targeted 100/s and trade workflows targeted 20/s.

| Five-minute phase and request | Successful samples | p50 HTTP ms | p95 HTTP ms | p99 HTTP ms |
|---|---:|---:|---:|---:|
| Spread quotes | 29,999 | 1.59 | 2.47 | 3.98 |
| Spread committed trades | 5,999 | 2.61 | 5.05 | 14.48 |
| 80% concentrated quotes | 30,000 | 1.53 | 2.35 | 3.52 |
| 80% concentrated committed trades | 5,979 | 2.52 | 4.47 | 16.51 |

An earlier workload overlapped with compilation and skipped 330 scheduled requests at the client's in-flight cap. It is retained as [`benchmarks/2026-09-09-build-contention.json`](benchmarks/2026-09-09-build-contention.json) and is not the clean baseline.

## Settlement workload

Raw artifact: [`benchmarks/2026-09-09-settlement.json`](benchmarks/2026-09-09-settlement.json)

The workload created 10,000 one-unit claims across 50 instances and 200 accounts. From evidence-upload start until all instances appeared resolved, settlement completed in 7.64 seconds. All 10,000 private portfolio credits matched one unit, 200 event streams stayed connected, two separate instances continued trading, and reconciliation passed.

| Concurrent request | Successful samples | p50 HTTP ms | p95 HTTP ms | p99 HTTP ms |
|---|---:|---:|---:|---:|
| Dedicated quotes | 764 | 1.76 | 2.40 | 3.61 |
| Committed trades | 153 | 4.28 | 60.45 | 74.41 |

The settlement interval was short and polled completion every 500 ms. These samples are not a saturation or long-duration reliability study.

## Measurement boundaries

- HTTP latency includes client round-trip and response JSON parsing.
- Trade latency covers only submitted execution, excluding preview and user think time.
- Dedicated quote counts exclude previews issued inside trade workflows.
- Stale-version conflicts are expected rejections and are reported separately.
- Client-skipped work and unexpected errors are retained so rejected load cannot inflate throughput claims.
- Local loopback measurements do not include campus network cost.

## Not established

- capacity or percentiles on a fixed 4-vCPU/8-GB host;
- campus-network visible-confirmation latency;
- committed event-propagation percentiles;
- saturation point, multi-hour reliability, or profiling attribution;
- production Docker/Compose behaviour on this host;
- a successful GitHub Actions run;
- formal proof of every decimal transcendental rounding boundary;
- performance comparison with a Python implementation of the new mechanism;
- visual browser correctness; or
- live evidence-source accuracy and availability.

These limitations should remain beside any copied performance summary.

