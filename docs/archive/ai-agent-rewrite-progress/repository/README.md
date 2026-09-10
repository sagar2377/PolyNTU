PolyNTU is an academic campus prediction-market application built with Rust, PostgreSQL and React. Users trade outcome shares using simulated units. A winning share resolves to one unit; other shares resolve to zero. All demo observations are explicitly labeled. No real money is involved.

The active backend has been rebuilt in Rust. Options, Black-Scholes pricing and Greeks have been retired. The five categories are weather, bus timings, fictional elections, queue/crowd, and event/lecture attendance. Seven demo templates exercise all five categories, including separate queue and occupancy measurements and separate event/lecture attendance examples.

**Start the local development application.** Rust 1.88+, a C/C++ build toolchain for Rust dependencies, PostgreSQL 17+, and Node.js 22+ are required. This Windows workspace has a project-local portable PostgreSQL installation. With frontend dependencies installed, run from the repository root in PowerShell:

```powershell
./scripts/run-dev.ps1
```

Open [PolyNTU locally](http://127.0.0.1:8000). The helper starts PostgreSQL on loopback port 55432, generates persistent local development secrets if absent, builds React, and runs the Rust backend. Create a demo account, choose a market/outcome, enter shares, preview, then confirm. To test resolution, enter the administrator token from `.local/dev-secrets.json` in the collapsed demo clock controls and advance time. Keep that file private.

For a fresh checkout, other operating systems, Docker, or separate frontend development, follow [development setup](docs/development.md). The service applies SQL migrations at startup. Real provider credentials are not required for the labeled demo.

**The backend protects trading state.** Quotes are deterministic, signed, account-bound and versioned. Trades atomically update ledger balances, inventory, positions and receipts. Idempotency keys prevent duplicate effects after retries. Published rules and final outcomes are immutable; settlement credits are resumable and unique per account/instance. The frontend preserves an uncertain trade request so it can retrieve the original receipt.

Local verification passed 20 Rust tests, including 384 independent numerical fixtures. A ten-minute workload sustained 100 dedicated quotes and 20 trade attempts per second with 200 event streams: p95 HTTP latency was 2.35–2.47 ms for quotes and 4.47–5.05 ms for committed trades. A separate run settled 10,000 claims in 7.64 seconds while trading continued. See [verification results](docs/verification.md) for hardware, request counts, conflicts, raw reports and measurement limits.

Core commands:

```powershell
# Windows: configure the installed build tools if necessary.
. ./scripts/rust-env.ps1
cargo test --manifest-path backend/Cargo.toml --lib --locked
cargo test --manifest-path backend/Cargo.toml --test numerical --locked

# Integration tests create and remove isolated polyntu_test_* databases.
$env:TEST_DATABASE_URL = 'postgres://polyntu@127.0.0.1:55432/postgres'
cargo test --manifest-path backend/Cargo.toml --test integration --locked -- --test-threads=4

cargo clippy --manifest-path backend/Cargo.toml --all-targets --locked -- -D warnings
```

| Documentation | Contents |
|---|---|
| [Architecture](docs/architecture.md) | Structure, responsibilities, transaction boundaries and recovery. |
| [Development](docs/development.md) | Setup, environment, commands and verification. |
| [API](docs/api.md) | Routes, authentication, quote/trade flow, evidence and units. |
| [Market types](docs/adding-a-market-type.md) | Adding a rule and adapter without modifying accounting. |
| [Rust decision](docs/decisions/0001-rust-backend.md) | Language and framework choice. |
| [Market mechanism decision](docs/decisions/0002-lmsr-accounting.md) | Pricing, precision, funding, rounding and limits. |
| [Resolution decision](docs/decisions/0003-evidence-resolution.md) | Observation sources, revisions, cutoffs and voids. |
| [Migration](docs/migration.md) | Python archive, preserved SQLite data and v2 cutover. |
| [Verification](docs/verification.md) | Tests, measured performance and remaining limits. |
| [Changes](docs/changes.md) | Implementation change record. |

Live feed adapters, campus SSO/account recovery, separate institutional resolver roles, and public deployment remain future integrations. The backend accepts structured, authenticated evidence from a configured source; validating its real-world accuracy requires a trustworthy provider. See the resolution decision for the exact data requirements.
