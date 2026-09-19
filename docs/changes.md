# PolyNTU change record

This file records significant user-visible and architectural changes. Detailed rationale belongs in [architectural decisions](decisions/) and verification evidence belongs in [verification](verification.md).

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

Live evidence adapters, SSO, credential recovery/rotation, separate resolver roles, rate limiting, outbox retention, public deployment hardening, and budget-to-quantity entry remain future work.

