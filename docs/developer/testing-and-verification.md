# Testing and verification reference

PolyNTU separates arithmetic tests, database/HTTP integration tests, frontend static checks, and performance workloads. Reconciliation is used throughout database testing as an end-state invariant check.

## Verification layers

| Layer | Location | Primary purpose |
|---|---|---|
| Rust unit/property | `backend/src/amm.rs`, `market.rs` | Pure math and category-rule behaviour |
| Independent numerical | `backend/tests/numerical.rs` | Exact rounded parity with external high-precision fixtures |
| PostgreSQL integration | `backend/tests/integration.rs` | HTTP/service/database transactions, races, rollback, recovery, settlement |
| Frontend static | npm lint/build | React rules, syntax, bundling, production asset generation |
| Engine benchmark | `backend/benches/engine.rs` | Arithmetic regression signal |
| HTTP workload | `scripts/benchmark.mjs` | Quote/trade latency and correctness under spread/concentrated load plus SSE |
| Settlement workload | `scripts/benchmark-settlement.mjs` | Bounded-claim completion with concurrent traffic and SSE |
| Reconciliation | `Store::reconcile` | Ledger, inventory, reserve, and total balance consistency |

## Rust library tests

### LMSR tests

`known_binary_quote_and_round_trip` fixes the known 10-share binary quote amount and proves immediate sale cannot gain from rounding.

`invalid_ranges_and_sell_inventory_are_rejected` covers aggregate negative inventory, invalid outcome, excessive quantity, and concentration limit.

`normalized_monotone_funded_and_no_roundtrip_gain` runs 128 generated cases across 2–8 outcomes, quantities, inventories, and liquidity. It checks probability normalization, selected-price monotonicity, no round-trip gain, and reserve funding while buying each outcome.

### Rule tests

`bus_boundaries_and_missing_coverage` checks half-open interval endpoints and distinguishes incomplete coverage from No.

`all_categories_have_deterministic_matching_evidence` ensures seven demo specifications validate, cover five categories, and generate matching final observations.

`elections_require_distinct_fictional_candidates` enforces distinct candidates and the fictional-only safeguard.

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

| Test | Behaviour demonstrated |
|---|---|
| `foreign_key_read_locks_do_not_block_account_balance_updates` | Migration 0003 lock compatibility; parallel balance update completes while another transaction holds an FK-related read lock. |
| `waiting_evidence_cannot_starve_ready_resolution` | Actionable worker selection resolves a ready row despite 101 waiting rows; invalid simulator source rejected. |
| `http_all_five_categories_buy_sell_resolve_and_authorization` | Seven templates/five categories, HTTP auth, retired route, buy/sell, duplicate receipt, demo resolution, portfolio claims. |
| `concurrent_duplicate_requests_have_one_effect_and_survive_restart` | 24 identical concurrent requests have one effect; receipt survives store reopen, expiry, and conflicting-body rejection. |
| `competing_quotes_cannot_execute_the_same_inventory_version` | Only one of twenty distinct requests using the same quote/version commits. |
| `concurrent_cross_market_spending_cannot_overdraw_account` | Account lock/nonnegative constraint allows only one of two otherwise overspending market trades. |
| `ownership_expiry_limits_and_pure_quotes_are_enforced` | Account-bound quote, limit, sale ownership, expiry, and quote purity. |
| `close_is_checked_after_waiting_for_market_lock` | Request waiting before close is rejected after clock passes close. |
| `out_of_order_evidence_uses_latest_revision_and_is_idempotent` | Exact duplicate, lower late revision, revision conflict, latest-result finalization, late final evidence rejection. |
| `missing_evidence_voids_categorical_positions_with_exact_rounding` | Three-outcome `1/n` void and floor to 333,666 microcredits. |
| `failed_trade_rolls_back_ledger_inventory_and_idempotency` | Injected trade insertion failure leaves no partial effect; same key succeeds after trigger removal. |
| `failed_settlement_batch_can_retry_without_duplicate_credit` | Injected claim failure rolls back balance; retries create one claim. |
| `settlement_resumes_after_a_committed_batch` | 101 accounts settle as 100 then 1 across store reopen, with reconciliation between batches. |

## Frontend verification

`npm run lint` uses Oxlint with React rules-of-hooks as an error and only-export-components as a warning. `npm run build` runs the Vite production build.

There are currently no frontend unit, component, accessibility, or end-to-end browser tests. Project instructions prohibit automated browser/computer control, so the development workflow requires human visual/interaction review for:

- navigation and responsive layout;
- keyboard focus/order;
- account creation/token restoration/sign-out;
- quote expiry and stale version handling;
- pending receipt recovery after simulated network interruption;
- portfolio pagination; and
- public evidence/result display.

## HTTP workload design

`scripts/benchmark.mjs` creates 200 accounts and 100 manual markets, then seeds 10,000 positions. It opens 200 event streams and schedules:

- 100 dedicated quote requests per second; and
- 20 trade workflows per second, each with its own preview.

The first half spreads traffic. The second sends 80% to one market. A 200-request in-flight cap records rather than hides skipped scheduling. Reports separate samples, committed trades, expected 409 conflicts, unexpected errors, skipped work, and p50/p95/p99. Reconciliation is required at start/end.

The PowerShell runner creates an isolated named database and hidden release executable on port 18000, then retains database/report/logs for inspection. It does not delete that benchmark database automatically.

## Settlement workload design

`benchmark-settlement.mjs` creates 50 closing markets × 200 one-share positions = 10,000 claims, plus two still-open trading instances. It keeps 200 SSE clients and scheduled quote/trade traffic active, uploads complete final evidence, polls completion every 500 ms, verifies all private claim amounts, and runs reconciliation.

The measurement begins at evidence-upload start, so it includes upload, finalization wait, worker cycles and polling granularity.

## Commands

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

- no retained raw logs for the recorded unit/integration/lint/build run;
- no frontend behavioural automation;
- no target-campus network measurement;
- no target 4-vCPU/8-GB reproduction;
- no saturation, soak, chaos, backup/restore, or multi-process benchmark;
- no event propagation percentile measurement;
- no schema migration upgrade matrix over multiple historical v2 versions;
- no rate-limit/security/adversarial fuzz suite;
- no live data-adapter tests; and
- no formal numerical proof beyond fixtures/properties.

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
