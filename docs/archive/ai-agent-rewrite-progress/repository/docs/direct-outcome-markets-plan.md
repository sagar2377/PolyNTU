PolyNTU implementation plan: replace options with direct outcome markets.

Historical proposal prepared 8 September 2026 against repository commit `ae15584`. The user subsequently approved rebuilding directly in Rust, superseding the Python-first/C++ path below. The rebuild is implemented; see [architecture](architecture.md), [decision records](decisions/0001-rust-backend.md), [changes](changes.md), and [verification](verification.md) for the current system. Performance figures in this proposal remain acceptance targets, not measured results.

**Build one outcome-trading engine for all five categories.** Remove call/put contracts, Black-Scholes pricing, Greeks, implied volatility, and variable option payoffs from the active product. Users buy or sell shares in a clearly defined outcome using the project's existing simulated units. A winning share resolves to one unit; other shares resolve to zero. Bus timings become a primary prediction market on arrival events.

Keep FastAPI and implement the new mechanism in Python first, backed by PostgreSQL. Use a modular application with a background worker from the same codebase. Evaluate C++ after measuring the new execution path. The initial scope includes binary and small categorical markets, buying, selling owned shares, account balances, positions, and automatic resolution. Resting limit orders, leverage, short selling, derivatives, real-money payments, and a general order-book matching engine are outside this release.

**The current backend needs a mechanism change, not just new categories.** The source inspection found these concrete starting points:

| Current code | Finding | Planned change |
|---|---|---|
| `backend/app/main.py`, `get_quote` | Each quote prices calls/puts and runs a 20,000-path Monte Carlo calculation. | Replace with a small deterministic outcome-share calculation. |
| `backend/app/main.py`, `_live_spot` and `create_contract` | Quote and creation request fresh random estimates independently. | Quote and execution share the same engine and reference persisted market versions. |
| `backend/core/settlement.py`, `create_contract` | Creation records a contract without debiting an account or changing shared liquidity. | Add atomic execution, positions, liquidity state, and a balanced ledger. |
| `backend/core/db.py` | SQLite is the default; contract values are floats; repository methods commit separately. | Add PostgreSQL migrations and transaction-scoped repositories for trading. |
| `backend/app/state.py`, `configure` | Instances and clock live in process memory; startup only regenerates default markets. | Persist every instance and recover its state independently of process startup or RNG order. |
| `backend/app/main.py`, `create_contract` and `advance_clock` | Creation has no explicit trading cutoff; settlement scans all contracts and relies on clock advancement. | Enforce closing inside execution and settle indexed due instances through a resumable worker. |
| `/portfolio`, `/admin/markets` | One global portfolio and no enforced admin authentication. | Introduce authenticated account ownership and admin/resolver roles. |
| `frontend/src/components/QuotePanel.jsx` | The interaction is built around thresholds, calls/puts, and Greeks. | Replace it with outcome selection, amount, quote preview, and trade confirmation. |

No latency benchmark or test run was performed during this planning pass. These findings identify work and likely sources of unnecessary cost; they do not establish which component currently dominates latency.

**Use a funded LMSR automated market maker for the first release.** This allows immediate trades without requiring a matching participant and uses the same mechanism for two or several mutually exclusive outcomes. Market scoring rules support this form of automated participation; choosing it here is an architectural recommendation for a small campus platform. [Hanson's original paper](https://hanson.gmu.edu/mktscore.pdf)

For outstanding share quantities `q`, fixed liquidity parameter `b > 0`, and `n` outcomes:

```text
C(q) = b * log(sum(exp(q_i / b)))
price_i = exp(q_i / b) / sum(exp(q_j / b))
trade cost = C(q + delta_q) - C(q)
```

Positive quantities buy shares; negative quantities sell shares already owned. Charge the cost difference, rather than multiplying the displayed marginal price by quantity. Larger trades change their own average execution price. With zero initial inventory and uniform starting prices, fund each instance with at least `b * ln(n)` simulated units, rounded up. These formulas and the funding bound are documented in the [Gnosis LMSR implementation primer](https://gnosis-pm-js.readthedocs.io/en/v1.3.0/lmsr-primer.html).

Choose `b` from an explicit per-instance subsidy budget before opening and keep it fixed. Larger budgets reduce price movement for the same trade. Use uniform initial prices for v1; biased priors need a separately checked funding calculation. Start with zero fees and publish the rounding policy. Budget recurring instances across the platform so generating many bus or weather markets cannot allocate unlimited liquidity.

Maintain the following invariants in code and database transactions:

- Every instance has a fixed set of 2–8 mutually exclusive and exhaustive outcomes. Binary Yes/No is the two-outcome case. Range markets use disjoint bins with explicit boundaries and an overflow bin.
- Outstanding outcome shares equal the sum of participant positions. Account balances and owned shares cannot become negative.
- Each trade transfers units between a participant and that instance's funded reserve. Initial account grants, subsidy allocations, trades, and final credits all have balanced ledger entries.
- Before final settlement, an instance's reserve covers its largest possible outcome liability: `reserve >= max(outstanding shares per outcome)` in settlement units. Reserved subsidy cannot be withdrawn while claims remain.
- A quote has no effect on prices, inventory, balances, observations, or the simulation clock. Only committed trades change inventory; background ingestion updates evidence separately.
- Simulated observations and optional forecasting models can inform users, but cannot silently reset the AMM prices or expose future simulated truth.

**Define market questions and resolution rules before opening them.** Each category supplies validation, instance scheduling, evidence normalization, and outcome evaluation. It does not supply its own pricing, accounting, or trade execution implementation.

| Category | First market shape | Required resolution evidence and edge cases |
|---|---|---|
| Weather | Yes/No: does accumulated rainfall at named station S reach a fixed threshold during `[14:00, 15:00)` on a named date? | Freeze station, units, sampling/aggregation rules, interval boundaries, minimum coverage, correction grace period, and backup-source policy. Missing readings are not zero rain. |
| Bus timings | Yes/No: does a bus on route R, direction D, arrive at stop S during `[07:30, 07:40)` on a named date? | Freeze route/stop identifiers, arrival detection rules, and observation window. Close trading before 07:30. Resolve from actual arrival evidence. Complete coverage with no arrival is No; missing coverage follows the missing-data policy. |
| Elections | Categorical: which candidate wins one specified seat in the fictional demo election? | Fixed candidates and published rules for ties, runoff, withdrawal, cancellation, and no winner by the deadline. Use final result evidence, not a poll or estimated vote share. Keep the repository's fictional election fixtures and existing fictional-only validation for this release. |
| Queue/crowd | Yes/No: does the queue at location L contain at least N people at a specified time? | Define the physical counting area, queue versus venue occupancy, sampling window, aggregation method, and trusted count source. Queue length and waiting time are different measurements. Add crowd occupancy as a second template under the same category. |
| Event/lecture attendance | Yes/No: do at least N unique attendees check in to event E by a specified cutoff? | Freeze event/session ID, deduplication rules, attendance window, correction deadline, and cancellation policy. Count actual attendance rather than registrations. Store aggregate evidence and an auditable source reference; avoid copying attendee identities into public market data. |

Use named dates and absolute UTC timestamps, displayed in Asia/Singapore time. Trading close, observation interval, evidence deadline, and finalization time are separate fields. Do not create questions such as “within the next five minutes” whose meaning changes every time somebody opens the page. For bus markets, start with a few useful route/stop/time combinations to avoid spreading participation and subsidy over too many markets.

Binary bus and attendance markets ship first; later arrival-time and attendance-count bins reuse the categorical engine. Treat related thresholds as views over one common bin market when added, so separate markets do not publish logically inconsistent cumulative probabilities.

**Build evidence handling independently of trading.** Persist source ID, event ID, event time, received time, normalized measurement, raw payload or immutable payload reference, quality status, and parser version. Deduplicate deliveries, handle out-of-order events, and replay recorded observations deterministically. Provider HTTP calls, retries, and aggregation belong in background workers rather than quote or trade requests.

Weather has a concrete candidate source: NEA's station-level rainfall dataset updates every five minutes and explicitly allows gaps and later corrections. Validate station coverage and measurement semantics before choosing the first live market. [NEA rainfall dataset](https://data.gov.sg/datasets/d_6580738cdd7db79374ed3152159fbd69/view)

Actual bus-arrival evidence is still an integration dependency. NTU directs users to its Omnibus application, but the reviewed page does not establish an accessible backend feed for this project. LTA's Bus Arrival API describes estimated arrivals; an ETA alone cannot establish that a bus actually arrived, and campus shuttle coverage must not be assumed. Investigate a permitted source of recorded arrivals or sufficiently reliable telemetry before enabling live bus resolution. [NTU campus shuttle information](https://www.ntu.edu.sg/about-us/visiting-ntu/internal-campus-shuttle), [LTA API guide](https://datamall.lta.gov.sg/content/dam/datamall/datasets/LTA_DataMall_API_User_Guide.pdf)

Queue/crowd and attendance need an institution-provided sensor/count feed or structured, authenticated evidence upload. Availability and access have not been established. Ship labeled simulator adapters for all five categories so mechanism development is independent of those integrations. Never substitute generated observations for missing live evidence. A live instance opens only when its source, coverage checks, and failure policy have passed a rehearsal.

Use the lifecycle `DRAFT -> OPEN -> CLOSED -> RESOLVING -> RESOLVED`, with `VOIDED` as an alternative terminal state and an optional trading suspension flag. Published outcomes, liquidity, and resolution rules become immutable when trading opens. Trading checks state and the authoritative current time while holding the instance lock; the scheduler is not the only cutoff enforcement.

Evidence proposes a result; category-specific grace periods allow delayed or corrected observations before finalization. Missing or conflicting evidence waits until the published deadline, then follows the published void rule. For v1, voided markets redeem every outcome share at `1/n` units, independent of entry price. Represent this fraction exactly, aggregate per account, round final credits down to the ledger tick, and record the residual in the instance reserve. This is an explicit proposed product policy, not an automatic refund of all historical trades.

Persist the final result and evidence version once. Credit accounts in bounded, idempotent settlement batches, with a unique claim key per instance and account. Mark an instance terminal only when every claim is accounted for. Repeated jobs and process crashes cannot credit a claim twice. After finalization, late corrections are recorded for audit and do not silently rewrite balances or historical results. Resolver permissions and audit records cover manual proposals and overrides before finalization.

**Make one database transaction the boundary of a trade.** Introduce persisted templates, instances, outcomes, AMM inventory/version, users, accounts, ledger transactions/entries, positions, trades, evidence, resolution claims, and an outbox. Use schema migrations, foreign keys, nonnegative checks, unique event keys, and indexes for active/due instances and account history. Keep current templates as an organizing concept; every concrete instance owns its own inventory, reserve, and resolution state.

PostgreSQL supports row locks that serialize conflicting updates; all execution and settlement paths must acquire their locks in one documented order to limit deadlocks. Use short transactions, bounded retries, and unique constraints alongside locks. [PostgreSQL locking documentation](https://www.postgresql.org/docs/current/explicit-locking.html)

The execution sequence is:

1. Authenticate the account and validate the typed request, including finite values, quantity bounds, outcome membership, and an idempotency key scoped to the account. Hash the payload; reusing a key with different content returns a conflict.
2. Acquire the idempotency claim and lock the instance, reserve, affected account, and position in the common order. A completed duplicate returns its stored response without executing again, including after quote expiry.
3. Recheck current time, market state, quote expiry/version, account balance, and owned shares. Obtain the cutoff time after lock acquisition so a request waiting in a queue cannot trade after closing.
4. Recalculate using the same versioned engine as the quote and enforce the user's maximum total cost or minimum proceeds. For v1, changed market versions require a fresh quote; do not silently substitute a different execution price.
5. Write the trade, inventory/version, positions, account movements, balanced ledger entries, completed idempotency response, and outbox event in that transaction. Commit before acknowledging success.
6. Deliver price/portfolio updates from the committed outbox. Delivery may repeat; consumers deduplicate by event ID and use sequence numbers to detect gaps. After reconnecting, fetch a current snapshot if needed.

Use integer micro-units for simulated balances and a documented fixed share scale; full-share redemption must convert exactly to one unit. Validate overflow bounds. Exchange these values as integers or decimal strings so frontend floating-point parsing cannot alter them. Database integer or exact numeric types are suitable for accounting; floating-point database types are inexact. [PostgreSQL numeric types](https://www.postgresql.org/docs/current/datatype-numeric.html)

LMSR logarithms and exponentials still need numerical computation. Use stable log-sum-exp and stable cost-difference formulas, with explicit bounds on quantity and `b`. Test extreme prices, very small trades, and large inventory. Round participant debits upward and proceeds downward once at the ledger boundary; record the rounding difference. Compare against a high-precision reference, require error below half a ledger tick across supported inputs, and use higher precision near ambiguous rounding boundaries. Do not claim integer accounting makes logarithms exact.

**Expose a small API that makes user actions predictable.** Add versioned endpoints while migrating the frontend:

| Endpoint | Contract |
|---|---|
| `GET /api/v2/markets` and `GET /api/v2/markets/{id}/instances` | Browse categories/templates and persisted instances, with pagination and filters. |
| `GET /api/v2/instances/{id}` | Outcomes, prices, state/version, closing time, resolution rule, evidence freshness, and liquidity information. |
| `POST /api/v2/quotes` | Instance, outcome, side, and share quantity; returns total cost/proceeds, average price, resulting price, share quantity, state version, expiry, and a signed quote token. Quotes do not reserve inventory. |
| `POST /api/v2/trades` | Quote token, maximum cost/minimum proceeds, and idempotency header; returns a durable receipt and new account/position versions. |
| `GET /api/v2/me/portfolio` and `GET /api/v2/me/trades` | Authenticated account balances, owned shares, settlement credits, and paginated history. |
| `GET /api/v2/instances/{id}/events` | Server-sent committed price/state updates with reconnect support. |
| Admin/resolution endpoints | Create/fund/open/suspend instances and propose/finalize evidence-backed results with enforced roles. |

The UI becomes: choose a market, choose an outcome, enter a quantity or budget, preview the exact trade, and confirm. Budget entry can solve for a quantity in the same quote service and round down to the share tick. Show both marginal probability and average execution price; a 60% displayed price does not mean an arbitrary-size trade executes at 0.60 per share. Show data timestamp, close time, stale-quote errors, and the final receipt. Remove options controls, Greeks, Monte Carlo panels, and implied-sigma fields.

**Optimize the measured request path before changing languages.** Start with pure Python numerical functions, pooled PostgreSQL connections, bounded queries, and background ingestion/settlement. Reuse the existing SQLAlchemy stack. Avoid introducing Redis, a separate broker, or networked engine services until measurements establish a need. A transactionally stored outbox and worker are sufficient for the first implementation.

Measure math, database query time, lock wait, serialization, and full HTTP latency separately. Cache only non-authoritative browse snapshots keyed by instance/version; trades always check committed state. Evaluate multiple API workers only after state is persisted. Concentrated traffic to one market and simultaneous spending from one account need their own load cases.

Proposed starting workload: 100 open instances, 200 connected clients, 10,000 positions, 100 quote requests/second and 20 trade requests/second, sustained for 10 minutes on a recorded 4-vCPU/8-GB environment with PostgreSQL colocated. Test both spread traffic and 80% of requests targeting one instance. Report submitted requests, successful trades, expected rejections, unexpected errors, and p50/p95/p99 separately; stale-quote rejection must not disguise poor execution throughput. Increase load to find saturation after the baseline passes.

| Acceptance measure | Initial target |
|---|---|
| Quote API | p95 <= 50 ms, p99 <= 150 ms server-side. |
| Committed trade API | p95 <= 150 ms, p99 <= 400 ms server-side. |
| User feedback | Confirmation visible within 500 ms on the specified campus-like network test; report network cost separately. |
| Committed update propagation | p95 <= 1 second, excluding upstream observation delays. |
| Settlement | 10,000 position claims completed within 60 seconds after evidence becomes final, without violating trading targets. |
| Correctness | Zero unbalanced ledger transactions, negative participant holdings, duplicate trades/credits, late accepted trades, or lost acknowledged writes in the test suite. |

Treat these targets as initial engineering goals. Record hardware, worker count, database settings, dataset, client mix, and latency boundaries with every result.

Prototype a C++ kernel only if the Python version misses the agreed workload targets and profiling attributes at least 30% of relevant request time to engine CPU after query/locking fixes. Keep the kernel behind a pure `quote/apply_delta` interface, retaining authentication, transactions, persistence, and source adapters in Python. Require identical rounded ledger results against Python/reference fixtures, randomized differential tests, sanitizer checks, reproducible Windows/Linux builds, and at least a 20% improvement in representative end-to-end p95 latency with no p99 regression before adopting it. These percentages are proposed decision thresholds, not performance predictions. A faster arithmetic microbenchmark alone does not justify a whole-backend rewrite.

**Deliver in dependency order with a working bus market as the first complete slice.** Each row should be a reviewable milestone; do not assign calendar estimates until team capacity and feed access are known.

| Milestone | Deliverable | Completion gate |
|---|---|---|
| 1. Fix the specification and baseline | Outcome/share/rounding/void rules, subsidy limits, source contracts, API schemas, benchmark harness, legacy data inventory. | Deterministic worked buy/sell/resolve examples and a recorded baseline; no source access assumptions hidden in market definitions. |
| 2. Implement the common engine | Binary and categorical LMSR, inventory updates, reserve checks, high-precision reference cases. | Numerical and property tests pass across the supported parameter range. |
| 3. Add durable trading | PostgreSQL migrations, account identity, ledger, positions, idempotent quote/trade APIs, and role checks. | Concurrent execution, restart recovery, ownership checks, and reconciliation pass against real PostgreSQL. |
| 4. Complete the bus market | Persisted schedules, simulator evidence, cutoffs, lifecycle, settlement/voids, and the minimal replacement trade UI. | A user can fund a demo account, quote, buy, sell, close, resolve, and inspect the receipt; existing options writes are disabled at cutover. |
| 5. Add the remaining categories | Weather, fictional categorical elections, queue and crowd templates, event and lecture attendance; shared simulator fixtures. | Every category completes the same lifecycle without changes to accounting or pricing. Trial live weather integration and bus evidence access as separate connector work. |
| 6. Verify and remove the old product path | Concurrency/load/crash tests, metrics, docs, dependency cleanup, migration rehearsal, final options removal. | Correctness and workload gates pass; all five categories are usable with labeled data modes; live modes require validated evidence sources. |
| 7. Decide on C++ from results | Profiling report and, only if the trigger is met, an isolated native prototype. | Adopt only if parity, build, safety, and end-to-end performance gates pass; otherwise retain Python and document why. |

Suggested code boundaries are `core/amm.py`, `core/execution.py`, `core/ledger.py`, revised `core/market.py` and `core/settlement.py`, `app/routes/`, `workers/`, `adapters/`, `migrations/`, and `benchmarks/`. Retain the existing market-type registry idea but replace `to_underlying`, `payoff_call`, `payoff_put`, `delta_sign`, and `greek_hints` with outcome specification and evidence evaluation. Adapt domain simulators as fixture sources where useful.

**Migrate options data without pretending it represents funded outcome shares.** Back up the existing database and export contracts with their original IDs, rules, and states. The old demo contracts have no account debit or ownership information, so archive them as legacy records rather than converting them into new balances or positions. Create fresh v2 accounts and explicit initial grants. Preserve legacy data without needing active option pricing code.

Cut over the frontend and API together after the bus slice works. Disable legacy creation routes; remove the old pricing/Greeks/implied-sigma routes and UI from the active release. Delete unused option modules and obsolete tests after archive checks; keep reusable domain-data tests. Update `README.md`, `docs/adding-a-market-type.md`, fixtures, and dependencies to describe outcome trading. Verify no calls to Black-Scholes remain in the active request path. Keep v2 and legacy data separate so a deployment rollback cannot erase acknowledged v2 trades; on a failed cutover, suspend trading and recover forward from the ledger.

**Test accounting accuracy and forecast quality separately.** The essential correctness coverage is:

- Numerical properties: prices sum to one within tolerance; buying an outcome increases its marginal price; permutation symmetry; one complete set redeems for one unit; supported trade sequences preserve reserve coverage; round trips cannot create units from rounding.
- Execution races: concurrent double spending, simultaneous sells, stale quotes, expired quotes, closing while a request waits on a lock, duplicate idempotency keys, and retries after an acknowledged or unacknowledged commit.
- Recovery: worker/API death before and after commit, crash during settlement batches, duplicate/out-of-order evidence, subscriber reconnection, and reloading all instances without RNG-dependent changes.
- Resolution: interval boundaries, missing data, delayed corrections, no arrival with full versus missing coverage, ties/cancellation, repeated check-ins, and uniform void redemption with deterministic rounding.
- Reconciliation and access: replay ledger entries into balances, reconcile positions to inventory and claims, isolate user portfolios, and enforce market/admin/resolver permissions.

Evaluate forecast quality later on held-out resolved events using calibration and Brier scores, grouped by category and forecast horizon. Compare against simple baselines and distinguish simulator runs from live evidence. An AMM gives coherent prices and executable trades; it cannot guarantee accurate predictions with sparse or uninformed participation. Display liquidity and participation context so users can judge how much evidence supports a price.
