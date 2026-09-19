# PolyNTU change record

This file records significant user-visible and architectural changes. Detailed rationale belongs in [architectural decisions](decisions/) and verification evidence belongs in [verification](verification.md).

## 20 September 2026: Market series, rolling spawn, and the per-market fee policy

- Verified creators publish market series through `POST /api/v2/series` (creators only; anyone else gets 403): a one-time market whose single instance publishes immediately, or a recurring series with an interval (1 minute to 1 day), a daily active window in Singapore time, a maximum concurrency capped at 50, and an optional end date whose absence means perpetual. Published definitions are immutable; only the series state moves from active to ended. `GET /api/v2/series` and `GET /api/v2/series/{id}` expose the list and the definition with its brackets.
- Rolling spawn: every worker tick publishes each active recurring series' upcoming grid slots, up to its maximum concurrency, skipping slots outside the active window or past the end date. A bracket closes and observes its slot [T, T+interval), finalizes one second after the observation end, and has its evidence deadline 60 seconds after it. A series ends when its end date has passed and no non-terminal bracket remains; a one-time series ends when its single instance settles.
- Per-market fee policy: `fee_charged` is fixed at creation (default true), set by the administrator for directly created instances and by the creator for a series. A fee-free market charges nothing, collects no fee, and pays no creator share.
- The demo bus is now a rolling fee-free welfare series: platform-owned, simulated, a bracket every 2 minutes, 06:00 to 23:59 Singapore time, 5 live brackets covering a rolling 10 minutes, perpetual. Its purpose is student welfare, crowd-sourcing real-time road traffic and arrival estimation, so no trading fee is charged. The one-shot bus demo spec is gone; six one-shot demo specs remain.
- Creators cannot trade in their own markets: their quotes and executions are rejected with the message that the fee share is their compensation.
- Frontend: a strip of active series chips on the browse page, a series page showing the schedule (interval, operating window, horizon, end date, fee, liquidity) with live and settled brackets, a Create market page for creators (one-time or recurring, category-specific rule fields, fee choice, schedule form), and the market page links to its series and shows the fee policy.
- The backend now has 11 unit tests, 30 integration tests, and 1 numerical test, all passing. See [ADR 0006](decisions/0006-market-series-and-recurrence.md).

## 20 September 2026: NTU accounts, password login, and creator verification

- Registration with a display name, a unique NTU email (`name@ntu.edu.sg` or `name@unit.ntu.edu.sg`, normalized to lowercase), and a password of at least 12 characters stored as an argon2id hash. Registered accounts start as members with the 10,000-unit welcome gift; the one-click demo account keeps its 1,000-unit grant and cannot log in.
- Password login rotates the account's single session token: an account holds at most one live bearer token and each login invalidates every previous one. Unknown email and wrong password return the same error, with equal argon2 work for unknown emails so response timing cannot enumerate accounts.
- Creator verification workflow: a member files one pending request, the administrator approves or rejects it with a recorded reason, approval permanently grants the creator role, a rejected member may re-apply, and every decision writes an administrator audit row.
- Admin role: admin routes accept an admin-role bearer session next to the shared token, and demo databases seed an administrator account (`admin@ntu.edu.sg`, password `admin`, demo mode only).
- Total issuance raised from 1M to 1B units as a second idempotent bootstrap transfer rather than a migration, because migrations run before the issuance account exists on a fresh database.
- Upgraded backend dependencies (sqlx 0.9, rand 0.10, hmac 0.13, sha2 0.11, base64 0.23, tower-http 0.7, argon2 0.6) and frontend dependencies (React 19.3, Vite 8.3).
- The account entry panel now leads with NTU registration, followed by email/password login, the demo accounts, and the access-token flow; members see a creator verification panel, and a rotated-out session clears the stored token instead of erroring.
- The backend now has 10 unit tests, 24 integration tests, and 1 numerical test, all passing. See [ADR 0005](decisions/0005-ntu-accounts-and-creator-roles.md).

## 20 September 2026: Use case model and platform plan documented

- Added the [use case model](developer/use-cases.md): six actors, a register of 24 use cases marked existing, partial, or planned, 13 business rules, the domain entities, and 12 detailed descriptions with preconditions, flows of events, and alternative flows.
- Added the use case diagram as PlantUML source at `docs/diagrams/use-case.puml` with the rendered image beside it, regenerate with `scripts/render-diagrams.ps1`.
- Recorded three proposed decisions: [ADR 0005](decisions/0005-ntu-accounts-and-creator-roles.md) (NTU email accounts, passwords, creator roles), [ADR 0006](decisions/0006-market-series-and-recurrence.md) (creator-owned series, recurrence, rolling spawn, creator trading ban), and [ADR 0007](decisions/0007-resolution-authority.md) (creator-signed human resolution, contracted external resolvers, administrator excluded).
- Documented the gap between the plan and the current build, with four implementation phases. No platform behaviour changed in this record.

## 20 September 2026 — CI runs only the areas a commit affects

- Added a `changes` job that detects whether a commit touched the backend, the frontend, or neither, and made the `verify` and `performance` jobs conditional on the result: backend changes gate the Rust checks and the performance benchmark, frontend changes gate the frontend lint and build, and commits touching neither skip both jobs.

## 20 September 2026 — Trading fees and market-creator revenue share

- Every trade now pays a 25-basis-point fee on the LMSR amount, rounded against the trader. Quotes and receipts expose all-in amounts with the fee reported separately; `limit_micros` bounds the all-in amount, so existing clients that echo the quoted amount are unaffected.
- The fee rides inside the single trader/reserve transfer and accumulates in the reserve as a per-instance pot, avoiding a per-trade hot row on the treasury account.
- At settlement the pot is split 50/50 between the platform treasury and the instance's recorded market creator (`instances.creator_account_id`, optional and immutable); platform-created markets pay their whole pot to the treasury.
- The pure LMSR engine, its fixtures, and the reserve-coverage invariant are unchanged; the fee is policy in the new `fee` module. See [ADR 0004](decisions/0004-trade-fees.md).

## 20 September 2026 — CI workflow fixes and action updates

- Fixed the `performance` job's `rust-cache` step, which failed with `spawn ENOTDIR` before any cache work: the `workspaces` input takes the workspace root directory, not a `Cargo.toml` path, because the action runs `cargo metadata` with that value as its working directory. The action logged the error but exited successfully, so the job stayed green while silently skipping both cache restore and save.
- Moved `rust-cache` after the `rustup` toolchain selection in both jobs so the cache key reflects the Rust version that actually builds, and added it to the `verify` job so test runs reuse compiled dependencies.
- Updated `actions/checkout` and `actions/setup-node` to v7, `actions/upload-artifact` to v7, and the CI Node version to 24; this also clears the Node 20 deprecation warnings on the runner.
- Made the benchmark step's server shutdown and exit-status propagation execute on benchmark failure despite the runner shell's `-e` mode.

## 20 September 2026 — Hot-path optimization and throughput KPI gate

- Served quotes from a read-through instance/token cache with a local clock estimate; a cache hit performs no database round trips, and every mutation invalidates or writes through before returning.
- Cut the trade transaction from fourteen statements to five by fusing reads (idempotency claim, instance plus clock, account locks plus balances plus position) and applying all remaining writes plus the commit-time notification in one data-modifying CTE.
- Replaced per-client SSE outbox polling with PostgreSQL `LISTEN/NOTIFY` fan-out over per-instance broadcast channels; the durable outbox remains authoritative with a 30-second catch-up safety net, so a lost notification delays but never drops an event.
- Settlement now processes independent instances concurrently in bounded chunks, and the server runs an explicitly multi-threaded runtime with a 32-connection pool.
- Added `scripts/benchmark-kpi.mjs`, a short closed-loop throughput KPI with regression gates, wired into CI as a `performance` job in `verify.yml`.

## 19–20 September 2026 — Windows helper script updates

- Made the helper scripts run under both Windows PowerShell 5.1 and PowerShell 7: replaced a .NET-Core-only random-number API and removed a native stderr redirect that 5.1 turns into a terminating error.
- Split the local launcher: `scripts/run-dev.ps1` now runs a debug backend without `--locked` for fast iteration, and the previous locked release behaviour is available as `scripts/run-prod.ps1`.
- Removed the frontend build from `run-dev.ps1`; the quick start uses `run-prod.ps1`, and `run-dev.ps1` pairs with the Vite dev server.

## 9 September 2026 — Documentation verification and two-depth reference

- Added a concise general overview and a detailed human-developer documentation set.
- Added a documentation register distinguishing current, historical, proposed, and archived material.
- Preserved the earlier AI agent's rewrite/progress documentation under `docs/archive/ai-agent-rewrite-progress/`.
- Corrected the documented trade lock order, administrator-audit scope, archive file count, account-administration wording, pagination explanation, and deterministic-quote wording.
- Added code, database, API, frontend, security/privacy, operations, testing, glossary, and requirements-to-code traceability references.
- Kept external provider discussion brief and treated normalized evidence availability as an assumed future integration dependency.

## 8–9 September 2026 — Rust outcome-market rebuild

- Replaced option contracts, Black–Scholes pricing, and Greeks with funded LMSR outcome shares.
- Added seven demo templates across weather, bus, fictional election, queue/crowd, and attendance categories.
- Added integer accounting, decimal pricing, signed expiring quotes, user limits, account isolation, and idempotent atomic execution.
- Added immutable definitions, typed evidence, source revisions, cutoff enforcement, deterministic private-seed demo evidence, bounded settlement, and reconciliation.
- Rebuilt the React interface around market discovery, buy/sell previews, private portfolios, and pending-receipt recovery.
- Archived the Python/SQLite implementation without converting its contracts into v2 balances.
- Added migrations, Windows helpers, Docker/Compose configuration, CI, numerical fixtures, PostgreSQL integration tests, and HTTP/SSE benchmarks.
- Fixed an account-lock upgrade deadlock by using `FOR NO KEY UPDATE` for balance-only account locks.
- Changed worker selection so markets waiting for evidence cannot starve ready settlements.
- Recorded 20 Rust tests, 384 numerical fixtures, a clean ten-minute HTTP workload, and a 10,000-claim settlement workload.

## Deferred work

Live evidence adapters, SSO, password recovery and secret rotation, separate resolver roles, rate limiting, outbox retention, public deployment hardening, and budget-to-quantity entry remain future work.

