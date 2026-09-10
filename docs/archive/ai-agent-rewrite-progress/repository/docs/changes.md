PolyNTU change record.

2026-09-08 — Rebuilt the active backend in Rust with PostgreSQL, following the user's decision to replace the Python options backend.

- Replaced option contracts, Black-Scholes pricing and Greeks with funded LMSR outcome shares. Bus arrivals are primary markets. Seven templates cover weather, buses, fictional elections, queue/crowd and event/lecture attendance.
- Added integer accounting, Decimal pricing, signed expiring quotes, cost/proceeds limits, account isolation and idempotent atomic trade execution.
- Added immutable definitions, typed evidence, source revisions, cutoff enforcement, deterministic demo observations, bounded settlement batches and reserve reconciliation.
- Replaced the React options screens with market discovery, buy/sell previews, private portfolios and persistent receipt recovery.
- Archived the previous Python source under `legacy/python-backend/`. The existing SQLite database and its local backup remain intact; legacy contracts are not converted into new balances.
- Added schema migrations, local setup scripts, a container configuration, numerical fixtures, PostgreSQL integration tests and an HTTP/SSE benchmark.
- Fixed a foreign-key/account lock-upgrade deadlock discovered by the first load test. Account balance writes now use `FOR NO KEY UPDATE`; a regression test holds the competing foreign-key lock explicitly.
- Added private persisted simulator seeds and fair worker selection, so waiting evidence cannot starve ready settlements.

Live data-provider adapters, campus SSO, credential recovery/rotation, outbox retention, budget-to-quantity entry and public deployment remain follow-up work. All demo observations are labeled as simulated; manual instances require authenticated evidence and void on missing data by their published deadline.

2026-09-09 — Completed 20 Rust test cases, including 384 independent pricing fixtures and 13 PostgreSQL integration cases. Measured 10,000 settlement credits in 7.64 seconds with concurrent quote/trade traffic and 200 event streams; every credit and accounting reconciliation passed. The final ten-minute run processed 59,999 dedicated quotes and 11,978 committed trades, with 21 expected stale-quote conflicts and no unexpected errors, skipped work, or reconciliation discrepancies. Raw reports and workload qualifications are linked from `verification.md`.

The original proposal in `direct-outcome-markets-plan.md` is historical. The implementation uses Rust from the outset, rather than its proposed Python-first/C++ optimization path. See the decision records for the current design and verification notes for measured results.
