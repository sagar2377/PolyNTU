# PolyNTU verification and traceability record

Status: **retained local evidence; reproducible commands documented**  
Verification began: **8 September 2026**

This one record holds the retained verification evidence (tests, workloads, benchmarks), the reproduction commands, and the requirements-to-code traceability matrix.

## Evidence policy

Verification claims must identify the command or workload, environment, result, and retained artifact when one exists. A passing test proves the tested behaviour under that environment; it is not a proof of every input, deployment, network, or provider.

The benchmark JSON files are retained in `docs/benchmarks/`. Build, lint, and test pass statements were recorded by the development agent but their complete raw console logs were not committed. Human reviewers can reproduce them with [development commands](development.md) and the commands below.

## Verification environment

| Item | Recorded value |
|---|---|
| Host | Windows development laptop |
| CPU | Intel Core Ultra 9 185H; 16 cores, 22 logical processors |
| Usable memory | 31.4 GiB |
| Rust/Cargo | 1.88 for the 8-9 September records; 1.98.1 for the 20 September measurements |
| PostgreSQL | 17.6 on loopback port 55432 |
| Node.js | 25.7 |
| PostgreSQL durability | `fsync`, `synchronous_commit`, and `full_page_writes` enabled |
| PostgreSQL connections | `max_connections=100` |
| Application | One release process, Tokio runtime; pool capped at 24 for the 8-9 September records and 32 for the 20 September measurements |

This differs from the original proposed 4-vCPU/8-GB environment. Results must not be presented as measured capacity for that smaller target or a campus network.

## Verification layers

PolyNTU separates arithmetic tests, database/HTTP integration tests, frontend static and browser checks, and performance workloads. Reconciliation is used throughout database testing as an end-state invariant check.

| Layer | Location | Primary purpose |
|---|---|---|
| Rust unit/property | `backend/src/amm.rs`, `market.rs`, `auth.rs`, `fee.rs`, `resolver.rs` | Pure math, category rules, accounts, fees, resolver answers |
| Independent numerical | `backend/tests/numerical.rs` | Exact rounded parity with external high-precision fixtures |
| PostgreSQL integration | `backend/tests/integration.rs` | HTTP/service/database transactions, races, rollback, recovery, settlement |
| Frontend static | npm lint/build | React rules, syntax, bundling, production asset generation |
| Browser end-to-end | `scripts/browser-e2e.mjs` | Full user flow against the real UI in headless Chrome, run in CI |
| Engine benchmark | `backend/benches/engine.rs` | Arithmetic regression signal |
| HTTP workload | `scripts/benchmark.mjs` | Quote/trade latency and correctness under spread/concentrated load plus SSE |
| Settlement workload | `scripts/benchmark-settlement.mjs` | Bounded-claim completion with concurrent traffic and SSE |
| Throughput KPI | `scripts/benchmark-kpi.mjs` | Closed-loop capacity and latency regression gate, run in CI |
| Reconciliation | `Store::reconcile` | Ledger, inventory, reserve, and total balance consistency |

## Test and static-analysis results

| Check | Recorded result | Reproduction source |
|---|---:|---|
| Rust library tests | 14 passed | `backend/src`, catalogued below |
| Independent numerical test | 1 passed | `backend/tests/numerical.rs` and the committed fixture JSON |
| PostgreSQL integration tests | 38 passed | `backend/tests/integration.rs`, catalogued below |
| Rust formatting | Passed | `cargo fmt --check` |
| Rust clippy | Passed with warnings denied | `cargo clippy --all-targets -- -D warnings` |
| React lint/build | Passed | `npm run lint`, `npm run build` |
| Release build | Passed with locked dependencies and overflow checks | Cargo release profile |
| Legacy database preservation | Matching SHA-256 | Original and `.local` backup |
| HTTP/static delivery | Health, HTML, and generated JavaScript returned 200 | Recorded manual HTTP check; no raw log retained |

The current counts were recorded on 20 September 2026 on rustc 1.98.1. The retained 8-9 September session recorded the then-current suites, twenty Rust test functions made of six library tests, one numerical test, and thirteen integration tests, on Rust 1.88; the catalogues below describe today's suites.

## Rust library tests

Grouped by area.

### LMSR tests

`known_binary_quote_and_round_trip` fixes the known 10-share binary quote amount and proves immediate sale cannot gain from rounding.

`invalid_ranges_and_sell_inventory_are_rejected` covers aggregate negative inventory, invalid outcome, excessive quantity, and concentration limit.

`normalized_monotone_funded_and_no_roundtrip_gain` runs 128 generated cases across 2–8 outcomes, quantities, inventories, and liquidity. It checks probability normalization, selected-price monotonicity, no round-trip gain, and reserve funding while buying each outcome.

### Rule tests

`bus_boundaries_and_missing_coverage` checks half-open interval endpoints and distinguishes incomplete coverage from No.

`all_categories_have_deterministic_matching_evidence` ensures the six one-shot demo specifications plus the recurring series specification validate, cover five categories between them, and generate matching final observations.

`active_window_is_interpreted_in_singapore_time` pins the recurring operating window to minute-of-day in SGT (UTC+8).

`bracket_titles_carry_their_time_window` pins the spawned recurring bracket title format: the series title plus `· HH:MM to HH:MM` in SGT for the bracket's [slot, slot+interval) window, including the midnight wrap, with an over-long base title truncated on a character boundary so the result fits the 240-character instance bound.

`elections_require_distinct_fictional_candidates` enforces distinct candidates and the fictional-only safeguard.

### Account and fee tests

`accepts_ntu_addresses_only` walks valid and invalid NTU email forms through the manual parser, including lookalike domains, uppercase, and overlong local parts.

`password_hashes_round_trip` proves argon2 hashes verify their source password and reject a different one.

`resolution_key_seed_matches_the_browser_derivation` pins the password-derived creator resolution seed (PBKDF2-HMAC-SHA256, 600,000 iterations, salt `polyntu.resolution.v1:{email}`) to a WebCrypto known-answer vector, including the resulting public key, and rejects wrong passwords and wrong emails.

`fee_rounds_up_and_never_exceeds_the_amount` checks 25 bps rounding on the charge amount.

`charged_amounts_stay_nonnegative_and_creator_split_sums` checks fee-free markets charge the pure engine amount and the creator/treasury split sums to the fee.

### Resolver tests

`responses_must_name_one_published_option` accepts only `{"pending":true}` or a published outcome id from an external resolver answer, and rejects unknown ids and malformed bodies.

## Independent numerical fixtures

`scripts/generate-math-fixtures.py` uses Python standard-library `Decimal` with 80-digit precision and fixed RNG seed `20260908`. It directly computes cost differences and applies the same published rounding policy without importing Rust/backend code.

The committed JSON contains 384 cases over outcome counts 2/3/8, liquidity 10/100/1,000/100,000, random inventory, high inventory, concentration edges, buy/sell, and quantities down to one millishare.

`matches_independent_eighty_digit_decimal_reference` deserializes every case, calls the Rust engine, and requires exact equality of `amount_micros`.

Regeneration is a specification change, not routine test maintenance. Review every fixture diff and the mathematical reason for it.

## PostgreSQL integration harness

Each test:

1. reads `TEST_DATABASE_URL`;
2. creates a unique alphanumeric `polyntu_test_<UUID>` database;
3. connects `Store` in demo mode, applying real migrations;
4. exercises services and/or the Axum router;
5. requires final reconciliation; and
6. closes connections and drops only its generated database on success.

Failure before cleanup can leave the isolated database for inspection. The harness never points tests at the application database unless the operator incorrectly supplies that URL as the administrative base; use a dedicated test PostgreSQL cluster/account where practical.

## Integration test catalogue

Grouped by area.

### Accounts, login, and sessions

| Test | Behaviour demonstrated |
|---|---|
| `registration_creates_a_member_account_with_the_welcome_gift` | A valid NTU email and password register a member account funded with the welcome gift. |
| `registration_rejects_invalid_submissions_and_duplicate_emails` | Non-NTU emails, short passwords, and duplicate emails are rejected without creating rows. |
| `registration_is_reachable_over_http_and_the_demo_grant_is_unchanged` | HTTP registration works and the demo grant is unaffected. |
| `login_rotates_the_session_token` | Login issues a fresh token and invalidates every previous session for the account, cached or not. |
| `login_failures_are_indistinguishable_and_demo_accounts_cannot_log_in` | Wrong password and unknown email return the same error; demo accounts without credentials cannot log in. |
| `login_is_reachable_over_http` | HTTP login returns a session usable on authenticated endpoints. |
| `demo_databases_seed_an_administrator_account` | Demo initialization seeds the `admin@ntu.edu.sg` administrator account idempotently. |

### Creator verification

| Test | Behaviour demonstrated |
|---|---|
| `approval_grants_the_creator_role_permanently` | Administrator approval flips the account role to creator and the decision cannot be reopened. |
| `rejection_records_a_reason_and_allows_reapplication` | Rejection stores the reason and the account may submit a fresh request. |
| `verification_endpoints_are_reachable_and_gated_over_http` | Submission, listing, and decision routes enforce account versus administrator authorization. |

### Series and spawning

| Test | Behaviour demonstrated |
|---|---|
| `creators_publish_one_time_series_with_their_single_instance` | A creator-role account publishes a one-shot series with its instance; non-creators are refused. |
| `recurring_series_keep_the_rolling_horizon_filled` | Worker ticks spawn brackets on the grid up to the concurrency limit inside the operating window. |
| `series_end_stops_spawning_and_settled_brackets_end_the_series` | Past the end no new brackets spawn; when every bracket settles the series ends. |
| `the_demo_bus_is_a_rolling_fee_free_series` | The demo bus seeds as a platform-owned fee-free recurring series with NTU Bus API resolver authority and spawns only inside the SGT window. |
| `old_recurring_brackets_purge_but_one_time_markets_stay` | A terminal recurring bracket past the retention window is purged with its whole subtree (outbox, positions, trades, claims, evidence, instance) while the drained reserve and its ledger trail remain; the one-time market and its trade history stay, the spawned title carries its time window, the append-only guard still rejects deletes outside the purge path, and reconciliation passes. |

### Fees and creator trading

| Test | Behaviour demonstrated |
|---|---|
| `trading_fees_are_charged_and_split_with_the_creator` | The 25 bps fee is charged on trades and split evenly between creator and treasury. |
| `fee_free_markets_charge_no_fee_and_pay_no_creator_share` | Fee-free markets charge the pure engine amount and pay no creator share. |
| `creators_cannot_trade_in_their_own_markets` | The creator account is refused with a conflict; the fee share is their compensation. |

### Resolution authority

| Test | Behaviour demonstrated |
|---|---|
| `resolution_authority_is_fixed_at_creation_and_blocks_admin_evidence` | Administrator evidence cannot resolve creator/resolver-authority instances; the authority is immutable after publication. |
| `creators_resolve_their_markets_with_signed_statements` | A keypair derived from the creator's account password, exactly as the browser derives it, signs a valid ed25519 resolution over the resolution message that settles the bracket; wrong nonce, signature, outcome, or account is rejected. |
| `resolver_authority_settles_from_the_external_endpoint` | The worker calls the configured endpoint and settles on the returned published outcome id. |
| `resolver_authority_voids_when_answers_stay_invalid_or_unreachable` | Unpublished answers and unreachable endpoints record nothing and the deadline voids the instance. |
| `bus_series_settles_through_the_ntu_bus_adapter` | A bus series whose resolver endpoint is the platform's own `/api/v2/resolvers/ntu-bus` settles through the real HTTP route: the worker's call, the adapter's answer from the deterministic simulated feed, the recorded `external-resolver` evidence, and no simulated-evidence fallback. |

### Price history and day view

| Test | Behaviour demonstrated |
|---|---|
| `instance_history_buckets_prices_and_volume` | Trade replay reconstructs post-fill prices and volume per bucket. |
| `series_day_view_weights_live_brackets_by_units_bet` | The day view reports volume-weighted probability and per-slot state, result, and volume. |

### Engine, concurrency, and settlement

| Test | Behaviour demonstrated |
|---|---|
| `foreign_key_read_locks_do_not_block_account_balance_updates` | Migration 0003 lock compatibility; parallel balance update completes while another transaction holds an FK-related read lock. |
| `waiting_evidence_cannot_starve_ready_resolution` | Actionable worker selection resolves a ready row despite 101 waiting rows; invalid simulator source rejected. |
| `http_all_five_categories_buy_sell_resolve_and_authorization` | Six templates/five categories, HTTP auth, retired route, buy/sell, duplicate receipt, demo resolution, portfolio claims. |
| `concurrent_duplicate_requests_have_one_effect_and_survive_restart` | 24 identical concurrent requests have one effect; receipt survives store reopen, expiry, and conflicting-body rejection. |
| `competing_quotes_cannot_execute_the_same_inventory_version` | Only one of twenty distinct requests using the same quote/version commits. |
| `concurrent_cross_market_spending_cannot_overdraw_account` | Account lock/nonnegative constraint allows only one of two otherwise overspending market trades. |
| `ownership_expiry_limits_and_pure_quotes_are_enforced` | Account-bound quote, limit, sale ownership, expiry, and quote purity. |
| `close_is_checked_after_waiting_for_market_lock` | A request that blocks on the instance row lock before its market closes is rejected once the clock passes close; the test waits through `pg_stat_activity` until the request is genuinely blocked before advancing the clock, making the stale-clock race reproducible. |
| `out_of_order_evidence_uses_latest_revision_and_is_idempotent` | Exact duplicate, lower late revision, revision conflict, latest-result finalization, late final evidence rejection. |
| `missing_evidence_voids_categorical_positions_with_exact_rounding` | Three-outcome `1/n` void and floor to 333,666 microcredits. |
| `failed_trade_rolls_back_ledger_inventory_and_idempotency` | Injected trade insertion failure leaves no partial effect; same key succeeds after trigger removal. |
| `failed_settlement_batch_can_retry_without_duplicate_credit` | Injected claim failure rolls back balance; retries create one claim. |
| `settlement_resumes_after_a_committed_batch` | 101 accounts settle as 100 then 1 across store reopen, with reconciliation between batches. |

## Frontend verification

`npm run lint` uses Oxlint with React rules-of-hooks as an error and only-export-components as a warning. `npm run build` runs the Vite production build.

A scripted browser end-to-end test, `scripts/browser-e2e.mjs`, drives the real React UI in headless Chrome over the Chrome DevTools protocol using only Node's global WebSocket, covering registration with the password-derived resolution key, creator verification through the seeded administrator, publishing a signed market with the cached key, trading through quote and confirm, the price history chart and the day probability bars, resolution with the cached key, and both password-prompt paths after the cache is cleared. Continuous integration runs it as the path-scoped `browser-e2e` job on any backend, frontend, or test-script change: the job builds the release backend and the frontend, serves them together in demo mode against a PostgreSQL 17 service, and runs headless Chrome on a debug port with a dedicated user data dir, retaining the server log as an artifact on failure. Locally the script needs the app serving the built frontend in demo mode and a Chrome with `--remote-debugging-port` on a dedicated `--user-data-dir`; the script header documents the invocation.

There are no frontend unit, component, or accessibility tests, and interactive browser control remains prohibited, so the development workflow still requires human visual/interaction review for:

- navigation and responsive layout;
- keyboard focus/order;
- account creation, login, and sign-out;
- quote expiry and stale version handling;
- pending receipt recovery after simulated network interruption;
- portfolio pagination; and
- public evidence/result display.

## Engine microbenchmark

The recorded release microbenchmark ran 10,000 calculations for a 10-share buy at `b=100`:

| Outcomes | Recorded mean per calculation |
|---:|---:|
| 2 | 18.25 microseconds |
| 3 | 19.95 microseconds |
| 8 | 24.88 microseconds |

It excludes authentication, serialization, network, database, locking, and transaction work. It is useful only for detecting large arithmetic regressions; HTTP measurements better represent user-facing latency.

## Ten-minute HTTP and SSE workload

`scripts/benchmark.mjs` creates 200 accounts and 100 manual markets, then seeds 10,000 positions. It opens 200 event streams and schedules:

- 100 dedicated quote requests per second; and
- 20 trade workflows per second, each with its own preview.

The first half spreads traffic. The second sends 80% to one market. A 200-request in-flight cap records rather than hides skipped scheduling. Reports separate samples, committed trades, expected 409 conflicts, unexpected errors, skipped work, and p50/p95/p99. Reconciliation is required at start/end.

The PowerShell runner creates an isolated named database and hidden release executable on port 18000, then retains database/report/logs for inspection. It does not delete that benchmark database automatically.

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

### Repeat measurement, 20 September 2026 (four-phase build)

Raw artifacts: [`benchmarks/2026-09-20-http.json`](benchmarks/2026-09-20-http.json) (retained) and [`benchmarks/2026-09-20-http-repeat.json`](benchmarks/2026-09-20-http-repeat.json) (first run, kept to document repeatability). The client was Node v26.8.1; the 9 September baseline ran v25.7.0.

The same workload with the same parameters ran twice. The two runs agreed within six percent on every percentile, and the first overlapped a concurrent integration test suite against the same PostgreSQL cluster, so the workload is insensitive to that level of background load. In the retained run all 12,000 trade workflows committed with zero stale-version conflicts, and there were zero unexpected errors, skipped requests, or reconciliation discrepancies in either run.

| Five-minute phase and request | Successful samples | p50 HTTP ms | p95 HTTP ms | p99 HTTP ms |
|---|---:|---:|---:|---:|
| Spread quotes | 29,999 | 6.13 | 17.72 | 18.30 |
| Spread committed trades | 5,999 | 17.19 | 21.21 | 23.92 |
| 80% concentrated quotes | 30,001 | 10.39 | 17.25 | 17.69 |
| 80% concentrated committed trades | 6,001 | 19.07 | 21.64 | 24.07 |

The percentiles are four to eight times the 9 September baselines, and the four-phase server changes are not the cause: a closed-loop KPI run on this same build (`./scripts/run-benchmark.ps1 -Workload benchmark-kpi.mjs -Seconds 15`) measured quotes at 4,883/s with p50 1.68 ms and trades at 1,193/s with p50 4.07 ms, against the recorded post-optimization KPI numbers of 5,916/s and 1,110/s, within the spread the KPI section already documents for this shared machine. The scheduled client changed major version between baselines (25.7.0 to 26.8.1) and holds 200 SSE streams open, so the delta sits in the measurement environment rather than the server. Treat the two dates as separate baselines; neither may be quoted as the other's capacity.

## Settlement workload

`benchmark-settlement.mjs` creates 50 closing markets × 200 one-share positions = 10,000 claims, plus two still-open trading instances. It keeps 200 SSE clients and scheduled quote/trade traffic active, uploads complete final evidence, polls completion every 500 ms, verifies all private claim amounts, and runs reconciliation.

The measurement begins at evidence-upload start, so it includes upload, finalization wait, worker cycles and polling granularity.

Raw artifact: [`benchmarks/2026-09-09-settlement.json`](benchmarks/2026-09-09-settlement.json)

The workload created 10,000 one-unit claims across 50 instances and 200 accounts. From evidence-upload start until all instances appeared resolved, settlement completed in 7.64 seconds. All 10,000 private portfolio credits matched one unit, 200 event streams stayed connected, two separate instances continued trading, and reconciliation passed.

| Concurrent request | Successful samples | p50 HTTP ms | p95 HTTP ms | p99 HTTP ms |
|---|---:|---:|---:|---:|
| Dedicated quotes | 764 | 1.76 | 2.40 | 3.61 |
| Committed trades | 153 | 4.28 | 60.45 | 74.41 |

The settlement interval was short; these samples are not a saturation or long-duration reliability study.

## Measurement boundaries

- HTTP latency includes client round-trip and response JSON parsing.
- Trade latency covers only submitted execution, excluding preview and user think time.
- Dedicated quote counts exclude previews issued inside trade workflows.
- Stale-version conflicts are expected rejections and are reported separately.
- Client-skipped work and unexpected errors are retained so rejected load cannot inflate throughput claims.
- Local loopback measurements do not include campus network cost.

## Throughput KPI benchmark

`scripts/benchmark-kpi.mjs` is the short key-performance-indicator workload. Unlike the ten-minute arrival-scheduled run above, it drives closed-loop load: a fixed number of workers iterate as fast as their requests complete, so the result is maximum sustainable capacity, not compliance with a target rate. Each run prepares its own accounts and markets, keeps 20 SSE streams connected, exercises a quote phase and then a trade-workflow phase, finishes with a reconciliation check, and writes a JSON report.

Run it locally through the same isolated-database launcher:

```powershell
./scripts/run-benchmark.ps1 -Workload benchmark-kpi.mjs -Seconds 15
```

The report is written to `.local/kpi.json` (override with `BENCH_REPORT`). The run fails when a gate regresses:

| Gate | Default | Override |
|---|---:|---|
| Quote throughput | at least 100/s | `KPI_MIN_QUOTE_RPS` |
| Quote p99 | at most 250 ms | `KPI_MAX_QUOTE_P99_MS` |
| Trade throughput | at least 15/s | `KPI_MIN_TRADE_RPS` |
| Trade p99 | at most 500 ms | `KPI_MAX_TRADE_P99_MS` |

Quote and trade errors, and a failed reconciliation, also fail the run. Expected 409 conflicts during the trade phase are counted separately and do not fail it.

### Recorded KPI result (20 September 2026, development host)

One 15-second phase each, 16 quote workers, 8 trade workers (one per market), 20 markets, 20 accounts, 16 connected SSE streams, Node 26 client, isolated databases, release builds:

| Metric | Pre-optimization build (87d8fd5) | Optimized build | Change |
|---|---:|---:|---|
| Quote throughput | 2,737/s | 5,916/s | +116% |
| Quote p50 / p95 / p99 | 5.42 / 10.44 / 13.46 ms | 1.92 / 6.60 / 14.24 ms | p50 −65% |
| Trade throughput | 1,017/s | 1,110/s | +9% |
| Trade p50 / p95 / p99 | 4.84 / 7.87 / 10.30 ms | 4.76 / 7.58 / 10.91 ms | within noise |
| Trade conflicts / errors | 0 / 0 | 0 / 0 | none |
| Reconciliation | ok | ok | none |

Two measurement limits: the quote phase is bounded by the single-process Node client, not the server, because after caching a quote costs no database round trip, so the server's real quote ceiling is higher than the recorded number; and trade percentiles move a few milliseconds between runs on this shared development machine. A separate single-trade check measured SSE delivery at 14 milliseconds from outbox timestamp to connected client, with the event arriving before the trade's HTTP response completed; before the change the handler polled once per second.

Continuous integration runs the same script as a separate `performance` job in `.github/workflows/verify.yml`: it builds the release backend, starts it against the job's PostgreSQL 17 service container, runs the benchmark for 15 seconds per phase, and fails the build on a gate regression, retaining the report and server log as artifacts. The job is path-scoped: it runs only when a commit touches the backend, the benchmark script, or the workflow itself. The gates are deliberately loose tripwires for order-of-magnitude regressions, because shared CI runners are slower and noisier than the recorded development machine, so a passing CI number must not be quoted as product capacity. Tune the gates through the environment variables above rather than re-baselining the artifacts.

## Commands

Backend:

```text
cargo fmt --manifest-path backend/Cargo.toml -- --check
cargo test --manifest-path backend/Cargo.toml --lib --locked
cargo test --manifest-path backend/Cargo.toml --test numerical --locked
cargo test --manifest-path backend/Cargo.toml --test integration --locked -- --test-threads=4
cargo clippy --manifest-path backend/Cargo.toml --all-targets --locked -- -D warnings
```

Frontend:

```text
cd frontend
npm ci
npm run lint
npm run build
```

Benchmarks:

```powershell
./scripts/run-benchmark.ps1
./scripts/run-benchmark.ps1 -Workload benchmark-settlement.mjs
```

Never aim benchmark scripts at a user database.

## Verification gaps

- no retained raw logs for the recorded unit/integration/lint/build runs;
- no frontend unit, component, or accessibility automation (the scripted browser test covers only the main end-to-end flow);
- no target-campus network measurement;
- no capacity or percentiles on a fixed 4-vCPU/8-GB host;
- no saturation, soak, chaos, multi-hour reliability, or profiling-attribution benchmark;
- no backup/restore or multi-process benchmark;
- no committed event-propagation percentile measurement;
- no schema migration upgrade matrix over multiple historical v2 versions;
- no rate-limit, security, or adversarial fuzz suite;
- no production Docker/Compose runtime verification on this host, and no public deployment hardening;
- no successful hosted GitHub Actions run retained;
- no formal numerical proof beyond the fixtures and properties, including every decimal transcendental rounding boundary;
- no performance comparison with a Python implementation of the new mechanism;
- no live data-adapter or evidence-source accuracy and availability tests; and
- visual layout, keyboard, and accessibility review beyond the scripted browser end-to-end flow remains human work.

These limitations should remain beside any copied performance summary.

## Requirements, code, and test traceability

This matrix helps human reviewers verify that documented promises correspond to implementation and tests. A reference identifies evidence, not infallibility.

### Core product requirements

| Requirement | Implementation | Database enforcement | Verification |
|---|---|---|---|
| Simulated units only; no payment path | Account/ledger services and React wording | Account/transfer schema contains no currency/payment table | HTTP category/lifecycle test; source review |
| 2–8 direct outcomes | `Rule::outcomes`, `amm::validate` | Inventory cardinality and outcome-length checks | Property/numerical tests; election validation |
| Five category families | `Rule`, `Metric`, `demo_specs` | Template category constraint | All-category unit/HTTP tests |
| Winning share resolves to one unit | `settle_batch` winner calculation | Exact integer columns and claims | Resolution integration tests; settlement workload |
| Missing final data uses published fractional void | `settle_batch`, public void policy | Terminal state/result protection | Exact categorical void test |
| No short selling | Execution compares position to sale quantity | Nonnegative position/inventory checks | Ownership integration test |
| Quotes do not mutate state | `Store::quote` is read-only | No quote table/state update | Pure-quote integration assertion |
| Trades are atomic | `execute_once` transaction | triggers, constraints, unique trade version | Injected failure rollback test |
| Duplicate delivery has one effect | account-scoped body-hash idempotency | idempotency primary key | 24-request concurrency/restart test |
| One distinct trade per market version | instance lock/version check | unique `(instance_id,instance_version)` | competing-quote test |
| No balance overdraft | account locking and precheck | nonnegative non-issuance account check | cross-market spending test |
| No trade after close | clock read after the instance lock, then the close recheck | state/inventory trigger after close | waiting-lock cutoff test |
| Published definitions/results immutable | service has no edit path | `protect_instance` trigger | migration/source inspection; lifecycle tests |
| Evidence revisions append-only/deduplicated | `record_evidence` | unique event/revision; immutable trigger | out-of-order/idempotency test |
| Settlement resumes without double credit | `settle_batch` selection/batching | claim primary key, unique ledger reference | failed/resumed batch tests |
| Reserve remains funded | trade liability check, settlement balance check | nonnegative reserve account | properties, reconcile, settlement workload |
| Trading fee is charged and split with the creator | `fee` module, all-in quotes/execution, `settle_batch` fee split | `trades.fee_micros`, `instances.creator_account_id`, `fee` ledger kind | fee/creator integration test; reconciliation |
| Public updates survive reconnect | outbox insert plus SSE cursor | durable outbox sequence/index | HTTP workload holds streams; propagation not percentile-tested |
| Account portfolios are private | bearer-derived account ID | account foreign keys | unauthorized and account-bound quote tests |
| Accounts register with a unique NTU email and password | `auth::valid_ntu_email`, `auth::hash_password`, `Store::register_account` | unique lowercase NTU-pattern email; email/password/role presence checks (migration 0006) | registration integration tests |
| Registered accounts receive the 10,000-unit welcome gift | `Store::register_account`, `WELCOME_GIFT_UNITS` | balanced grant transfer; nonnegative treasury | registration gift assertion |
| Each login invalidates every previous session | `Store::login` token rotation with `evict_account` | single `token_hash` column per account | login rotation test across two logins |
| Login failures cannot enumerate accounts | `LOGIN_TIMING_HASH` equal-work verification | no per-account error state stored | indistinguishable-failure test |
| Creator verification: one pending request, decided with a reason, permanent role | `create_verification_request`, `decide_verification_request` | partial unique pending index; status/role checks (migrations 0007, 0008) | verification integration tests |
| Admin routes accept an admin-role session | `AppState::require_admin`, `Store::account_is_admin` | role check includes `admin` (migration 0008) | seeded-administrator test |
| Creators publish one-time and recurring series | `Store::create_series`, `market::NewSeries`/`Schedule` | `market_series` definition immutability and checks (migration 0009) | series one-time publication test with member rejection |
| Rolling spawn fills the horizon inside the active window | `Store::spawn_due_brackets`/`spawn_series_brackets`, `market::inside_active_window` | template/close uniqueness; immutable `series_id`/`bracket_start_ms` | rolling horizon grid test; end-date stop test |
| Series end after the last non-terminal bracket | `Store::spawn_due_brackets` ending update | `state IN ('active','ended')` check with no reopening | end-date stop test with voiding and series end |
| Creators cannot trade in their own markets | creator check in `Store::quote` and `execute_once` | none required beyond the immutable creator reference | creator trading ban test |
| Per-market fee policy: fee-free markets charge nothing and pay no creator share | `fee::charged` with `fee_charged`, `Instance.fee_charged` | `instances.fee_charged`/`market_series.fee_charged` immutable (migration 0009) | fee-free market test with no creator payout |
| The demo bus is a rolling fee-free series | `worker::seed_demo`, `market::demo_series_spec` | platform-owned series row with null creator | rolling demo bus test |
| Resolution authority is fixed at series creation: administrator (default), creator key, or resolver endpoint | `market::ResolutionSpec` with `NewSeries::validate`, `Store::create_series` | `market_series` authority columns and checks; immutable under `protect_series` (migration 0010) | resolution authority integration test with malformed key and insecure endpoint rejections |
| Administrator evidence cannot resolve a market whose authority is not administrator | exclusion in `Store::record_evidence` | application-path check only; no schema enforcement | admin-evidence rejection assertion inside the authority test |
| Creators resolve their markets with an ed25519 signature over a fixed message | `auth::resolution_message`, `auth::resolution_key_seed`, `auth::verify_ed25519`, `Store::record_creator_resolution` | one `creator-signature` evidence row per instance under the unique event/revision constraints | creator resolution test with wrong-nonce, non-creator, administrator, and replay rejections plus settlement |
| Resolver endpoints settle or void per the fixed request/response contract | `resolver::ResolverRequest`, `resolver::parse_response`, `resolver::call`, worker call loop, `Store::record_resolver_evidence` | `resolver_endpoint` required for resolver authority (migration 0010) | resolver settle test against a live local server and void test for invalid/unreachable answers |
| Bucketed price and volume history is reconstructed from executed trades | `Store::instance_history` | read-only replay over `trades`; no stored history | history bucketing test including the untouched-market case |
| The series day view weights live brackets by traded volume | day computation in `Store::series_view` | computed on request; no stored day state | day view weighting test across traded and untraded brackets |
| Spawned recurring bracket titles carry their slot's time window | `market::bracket_title`/`sgt_hhmm` in `Series::bracket_spec` | `title` is a protected instance field under `protect_instance` | unit test `bracket_titles_carry_their_time_window`; title assertion in the retention integration test |
| Terminal recurring brackets are purged past the retention window; one-time markets are kept | `Store::purge_expired_brackets`, `BRACKET_RETENTION_MS`, worker tick step | migration 0011: guarded `immutable_record` deletes (`polyntu.purge = 'on'`), deferrable circular `instances_evidence_id_fkey`, partial `instances_purge` index; ledger and audit rows untouched | `old_recurring_brackets_purge_but_one_time_markets_stay` |

### Operational and security requirements

| Requirement | Implementation/evidence | Known gap |
|---|---|---|
| Demo cannot bind publicly | `main.rs` loopback check | Does not harden non-demo deployment |
| Quote/admin secrets differ and are long | `AppState::new` | Length is not entropy; no rotation |
| Account token plaintext not stored in DB | `random_token`, `hash`, account lookup | Browser local storage holds plaintext; demo-account tokens have no recovery (registered accounts log in again) |
| Internal errors hidden from API | `Error::IntoResponse` | Server logs still require access control/redaction discipline |
| Administrator endpoints protected | `require_admin` on all `/admin/*` handlers | The shared token still has no per-action identity; admin-audit coverage incomplete |
| Evidence contains no attendee identities | Typed aggregate observations and documentation | Reference is arbitrary public text; no automatic redaction |
| Reconciliation available | `Store::reconcile`, SQL reserve query | Detection only; no automated repair/alerting |
| Database mode cannot silently change | `Store::initialize` | Separate database remains an operator responsibility |

### Source-file coverage map

| Active file | Main documentation | Tests/evidence |
|---|---|---|
| `backend/src/lib.rs` | [Backend](developer/backend.md) | Compiled by every backend check |
| `backend/src/main.rs` | [Architecture](architecture.md), [Operations](developer/operations.md) | Release build/startup HTTP record |
| `backend/src/error.rs` | [Backend](developer/backend.md), [API](api.md) | HTTP authorization/error paths; not every variant directly asserted |
| `backend/src/auth.rs` | [Security](developer/security-and-privacy.md), [Trading](developer/trading-and-accounting.md) | Account/quote authorization integration paths |
| `backend/src/amm.rs` | [Trading](developer/trading-and-accounting.md), ADR 0002 | Unit/property and 384-fixture test |
| `backend/src/market.rs` | [Backend](developer/backend.md), [Evidence](developer/evidence-and-settlement.md) | Category and schedule unit tests; all-category HTTP test |
| `backend/src/execution.rs` | [Trading](developer/trading-and-accounting.md) | Concurrency, expiry, ownership, rollback, cutoff tests |
| `backend/src/fee.rs` | [Trading](developer/trading-and-accounting.md), ADR 0004 | Fee unit tests; fee-split and fee-free integration tests |
| `backend/src/store.rs` | [Backend](developer/backend.md), [Database](developer/database.md) | Integration harness and reconciliation after every DB test |
| `backend/src/resolution.rs` | [Evidence](developer/evidence-and-settlement.md), [Backend](developer/backend.md) | Revision, void, rollback, resume, creator-resolution, and resolver-evidence tests |
| `backend/src/resolver.rs` | [Backend](developer/backend.md), [Evidence](developer/evidence-and-settlement.md) | Resolver response unit test; resolver settle/void integration tests; the NTU Bus API adapter end-to-end test |
| `backend/src/worker.rs` | [Architecture](architecture.md), [Evidence](developer/evidence-and-settlement.md) | Fairness/all-category tests, rolling spawn tests, resolver tests, and workloads |
| `backend/src/cache.rs` | [Backend](developer/backend.md), [Architecture](architecture.md) | Token eviction exercised by the login rotation tests; invalidation by every mutating integration test |
| `backend/src/events.rs` | [Architecture](architecture.md) | SSE delivery under load in the HTTP workloads |
| `backend/migrations/*.sql` | [Database](developer/database.md) | Fresh migrations in each integration database |
| `backend/queries/reconcile_reserves.sql` | [Database](developer/database.md), [Trading](developer/trading-and-accounting.md) | Reconciliation after tests/workloads |
| `frontend/src/api.js` | [Frontend](developer/frontend.md), [API](api.md) | Lint/build; driven end to end by the CI browser test |
| `frontend/src/App.jsx` | [Frontend](developer/frontend.md) | Lint/build; CI browser test drives sign-in and verification; manual review for the rest |
| `frontend/src/pages/*.jsx` | [Frontend](developer/frontend.md) | Lint/build; CI browser test drives the market, series, and create pages; manual review for the rest |
| `frontend/src/components/TradePanel.jsx` | [Frontend](developer/frontend.md), [Trading](developer/trading-and-accounting.md) | Lint/build; CI browser test covers quoting and confirmation; receipt recovery not browser-automated |
| `frontend/src/components/PriceHistoryChart.jsx`, `frontend/src/components/DayProbabilityBars.jsx` | [Frontend](developer/frontend.md), [API](api.md) | Lint/build; CI browser test verifies the price chart and the day bars render |
| `scripts/*.ps1` | [Development](development.md), [Operations](developer/operations.md) | Used in recorded local runs; no script unit tests |
| `scripts/*.mjs` | [Workloads](#ten-minute-http-and-sse-workload) | Retained benchmark JSON |
| `scripts/browser-e2e.mjs` | [Browser end-to-end](#frontend-verification), [Development](development.md) | Runs in the CI `browser-e2e` job against the real UI |
| `.github/workflows/verify.yml` | [Development](development.md) | Workflow defined; successful hosted run not retained here |

### Documentation-to-source review checklist

When reviewing a claim:

1. identify whether it is requirement, current behaviour, historical decision, measurement, or future work;
2. follow the implementation references above;
3. check the latest migration rather than only the first schema definition;
4. inspect the test assertion, not merely its name;
5. inspect raw benchmark fields and measurement boundaries;
6. verify the documented working-tree/commit baseline; and
7. report differences instead of choosing whichever statement is more convenient.

### Planned requirements

Everything planned is implemented; the resolution authority and market experience requirements have moved into the core table above. The [use case model](developer/use-cases.md) holds the full register, the flows, and the business rules, and every decision record is accepted and implemented. The one deferred item is the real bus timing adapter, a live data source rather than platform work.

## Adding a verification claim

Record:

1. exact command/workload revision;
2. toolchain, host, database settings, process/connection counts;
3. input dataset and duration;
4. submitted, successful, expected rejection, skipped, and unexpected failure counts;
5. latency boundaries and percentiles;
6. reconciliation/credit validation;
7. raw artifact path; and
8. limitations that prevent broader interpretation.
