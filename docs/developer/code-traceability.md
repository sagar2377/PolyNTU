# Requirements, code, and test traceability

This matrix helps human reviewers verify that documented promises correspond to implementation and tests. A reference identifies evidence, not infallibility.

## Core product requirements

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
| No trade after close | time recheck under instance lock | state/inventory trigger after close | waiting-lock cutoff test |
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
| Creators resolve their markets with an ed25519 signature over a fixed message | `auth::resolution_message`, `auth::verify_ed25519`, `Store::record_creator_resolution` | one `creator-signature` evidence row per instance under the unique event/revision constraints | creator resolution test with wrong-nonce, non-creator, administrator, and replay rejections plus settlement |
| Resolver endpoints settle or void per the fixed request/response contract | `resolver::ResolverRequest`, `resolver::parse_response`, `resolver::call`, worker call loop, `Store::record_resolver_evidence` | `resolver_endpoint` required for resolver authority (migration 0010) | resolver settle test against a live local server and void test for invalid/unreachable answers |
| Bucketed price and volume history is reconstructed from executed trades | `Store::instance_history` | read-only replay over `trades`; no stored history | history bucketing test including the untouched-market case |
| The series day view weights live brackets by traded volume | day computation in `Store::series_view` | computed on request; no stored day state | day view weighting test across traded and untraded brackets |

## Operational and security requirements

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

## Source-file coverage map

| Active file | Main documentation | Tests/evidence |
|---|---|---|
| `backend/src/lib.rs` | [Backend](backend.md) | Compiled by every backend check |
| `backend/src/main.rs` | [Architecture](../architecture.md), [Operations](operations.md) | Release build/startup HTTP record |
| `backend/src/error.rs` | [Backend](backend.md), [API](../api.md) | HTTP authorization/error paths; not every variant directly asserted |
| `backend/src/auth.rs` | [Security](security-and-privacy.md), [Trading](trading-and-accounting.md) | Account/quote authorization integration paths |
| `backend/src/amm.rs` | [Trading](trading-and-accounting.md), ADR 0002 | Unit/property and 384-fixture test |
| `backend/src/market.rs` | [Backend](backend.md), [Evidence](evidence-and-settlement.md) | Category and schedule unit tests; all-category HTTP test |
| `backend/src/execution.rs` | [Trading](trading-and-accounting.md) | Concurrency, expiry, ownership, rollback, cutoff tests |
| `backend/src/fee.rs` | [Trading](trading-and-accounting.md), ADR 0004 | Fee unit tests; fee-split and fee-free integration tests |
| `backend/src/store.rs` | [Backend](backend.md), [Database](database.md) | Integration harness and reconciliation after every DB test |
| `backend/src/resolution.rs` | [Evidence](evidence-and-settlement.md), [Backend](backend.md) | Revision, void, rollback, resume, creator-resolution, and resolver-evidence tests |
| `backend/src/resolver.rs` | [Backend](backend.md), [Evidence](evidence-and-settlement.md) | Resolver response unit test; resolver settle/void integration tests |
| `backend/src/worker.rs` | [Architecture](../architecture.md), [Evidence](evidence-and-settlement.md) | Fairness/all-category tests, rolling spawn tests, resolver tests, and workloads |
| `backend/src/cache.rs` | [Backend](backend.md), [Architecture](../architecture.md) | Token eviction exercised by the login rotation tests; invalidation by every mutating integration test |
| `backend/src/events.rs` | [Architecture](../architecture.md) | SSE delivery under load in the HTTP workloads |
| `backend/migrations/*.sql` | [Database](database.md) | Fresh migrations in each integration database |
| `backend/queries/reconcile_reserves.sql` | [Database](database.md), [Trading](trading-and-accounting.md) | Reconciliation after tests/workloads |
| `frontend/src/api.js` | [Frontend](frontend.md), [API](../api.md) | Lint/build; behavioural tests absent |
| `frontend/src/App.jsx` | [Frontend](frontend.md) | Lint/build; manual behaviour required |
| `frontend/src/pages/*.jsx` | [Frontend](frontend.md) | Lint/build; manual behaviour required |
| `frontend/src/components/TradePanel.jsx` | [Frontend](frontend.md), [Trading](trading-and-accounting.md) | Lint/build; receipt recovery not browser-automated |
| `frontend/src/components/PriceHistoryChart.jsx`, `frontend/src/components/DayProbabilityChart.jsx` | [Frontend](frontend.md), [API](../api.md) | Lint/build; chart behaviour not browser-automated |
| `scripts/*.ps1` | [Development](../development.md), [Operations](operations.md) | Used in recorded local runs; no script unit tests |
| `scripts/*.mjs` | [Testing](testing-and-verification.md) | Retained benchmark JSON |
| `.github/workflows/verify.yml` | [Development](../development.md) | Workflow defined; successful hosted run not retained here |

## Documentation-to-source review checklist

When reviewing a claim:

1. identify whether it is requirement, current behaviour, historical decision, measurement, or future work;
2. follow the implementation references above;
3. check the latest migration rather than only the first schema definition;
4. inspect the test assertion, not merely its name;
5. inspect raw benchmark fields and measurement boundaries;
6. verify the documented working-tree/commit baseline; and
7. report differences instead of choosing whichever statement is more convenient.

## Planned requirements

Everything planned is implemented; the resolution authority and market experience requirements have moved into the core table above. The [use case model](use-cases.md) holds the full register, the flows, and the business rules, and every decision record is accepted and implemented. The one deferred item is the real bus timing adapter, a live data source rather than platform work.

## Known unverified areas

The frontend interaction model, visual layout/accessibility, Docker runtime, hosted CI, backup restoration, public hardening, multi-process capacity, and live evidence-provider behaviour do not currently have complete retained verification. Their documentation describes design or procedure and labels the gap.

