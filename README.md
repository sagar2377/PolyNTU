# PolyNTU

PolyNTU is an academic campus prediction-market application built with Rust, PostgreSQL, and React. Participants use simulated units to buy and sell shares in clearly defined outcomes. A winning share resolves to one unit; losing shares resolve to zero. No real money is involved.

The active product is version 0.2: a Rust/Axum backend, PostgreSQL accounting store, background resolution worker, and React frontend. The earlier Python options prototype is archived under `legacy/python-backend/` and is not executed by the current application.

## Quick start on the project Windows workspace

Prerequisites are Rust 1.88 or newer, a compatible C/C++ build toolchain, PostgreSQL 17 or newer, and Node.js 22 or newer. This workspace can use its project-local PostgreSQL installation.

```powershell
# From the repository root. Install frontend packages once on a fresh checkout.
Push-Location frontend
npm ci
Pop-Location

# Start PostgreSQL, build React, and run the release backend.
./scripts/run-prod.ps1
```

Open [http://127.0.0.1:8000](http://127.0.0.1:8000). Create a demo account, open a market, preview a trade, and confirm it. Demo databases also seed an administrator account, `admin@ntu.edu.sg` with password `admin`, for signing in and working the verification queue. In demo mode, the administrator token stored in `.local/dev-secrets.json` can advance the demonstration clock. Keep that file private.

For other platforms, Docker, separate frontend development, and environment variables, see [development setup](docs/development.md). For fast backend iteration, `./scripts/run-dev.ps1` runs a debug backend and skips the frontend build; keep the Vite dev server (`npm run dev` in `frontend/`) running for the interface.

## What the system provides

- Five market categories: weather, bus timings, fictional elections, queue/crowd measurements, and event/lecture attendance.
- NTU email registration, password login, and single-session tokens that invalidate older sessions.
- An administrator-reviewed verification workflow that grants the creator role.
- One-time and recurring market series; recurring series spawn brackets on a fixed grid inside a daily operating window.
- One funded LMSR pricing engine for binary and small categorical markets.
- Exact integer ledger units and bounded decimal market calculations.
- Per-market trading fees shared between the creator and the treasury; welfare markets can opt out of fees entirely.
- Signed, account-bound, expiring quotes and idempotent trade execution.
- Immutable published rules, append-only evidence, and resumable settlement.
- Resolution by administrator evidence, creator-signed statements, or a configured external resolver.
- Bucketed price and volume history reconstructed from executed trades.
- Authenticated private portfolios and durable trade receipts.
- Persisted public market events delivered through Server-Sent Events (SSE).
- Deterministic, explicitly labelled simulator evidence for local demonstrations.

## Repository map

| Path | Purpose |
|---|---|
| `backend/src/` | Active Rust application and market engine. |
| `backend/migrations/` | PostgreSQL schema, constraints, and triggers. |
| `backend/tests/` | Numerical and PostgreSQL integration tests. |
| `frontend/src/` | React interface and browser-side API client. |
| `scripts/` | Local setup, fixture generation, and benchmark helpers. |
| `docs/` | Current general and developer documentation. |
| `docs/archive/ai-agent-rewrite-progress/` | Preserved documentation written by the earlier AI development agent while performing the rewrite. |
| `legacy/python-backend/` | Retired Python options prototype, kept for historical inspection only. |

## Documentation

Start with the [documentation index](docs/README.md).

- [General overview](docs/overview.md): product concepts, user flow, lifecycle, and limitations.
- [Developer guide](docs/developer-guide.md): reading order for the complete technical documentation.
- [Documentation audit](docs/documentation-audit.md): reviewed scope, corrections, coverage, and verification boundaries.
- [Architecture](docs/architecture.md): service boundaries, transactions, and recovery.
- [API](docs/api.md): public HTTP contract and examples.
- [Development](docs/development.md): setup, configuration, and commands.
- [Verification](docs/verification.md): evidence, benchmark results, and measurement limits.
- [Adding a market type](docs/adding-a-market-type.md): category extension checklist.
- [Migration and legacy](docs/migration.md): Python archive and v2 data policy.

## Verification snapshot

The retained verification artifacts report 20 Rust tests, including 384 independent high-precision numerical fixtures. The final ten-minute HTTP workload submitted 59,999 dedicated quotes and 11,999 trade workflows with zero unexpected errors, skipped work, or reconciliation discrepancies. A separate workload settled 10,000 account claims in 7.64 seconds while quote/trade traffic and 200 event streams remained active.

These are local development-machine measurements, not production capacity guarantees. See [verification](docs/verification.md) for raw reports, test commands, hardware, and limitations. A short closed-loop throughput KPI (`scripts/benchmark-kpi.mjs`) runs in CI as a performance-regression gate with deliberately loose thresholds.

## Current scope limits

Live provider adapters, campus SSO, password recovery, rate limiting, outbox retention, budget-to-quantity entry, and public production deployment are not implemented. The current manual evidence endpoint accepts authenticated structured observations but cannot independently prove their real-world accuracy, and creator-signed or resolver-based resolutions likewise trust their key holders and endpoints. External feeds are treated as a future integration dependency; the present application assumes that suitable normalized observations can eventually be supplied.
