Developing and running PolyNTU v2.

The backend needs Rust 1.88 or newer plus the platform's native build tools. PostgreSQL 17 is the verification database; schema and business logic use PostgreSQL features and are not backed by a SQLite fallback. The frontend uses the committed npm lockfile and Node.js 22 or newer. Use `cargo --locked` and `npm ci` for reproducible installs.

On this Windows workspace, portable PostgreSQL binaries are under `.tools/postgres/pgsql/`, with cluster data under `.local/postgres-data/`. The binaries were obtained from EDB's PostgreSQL distribution. They are local development dependencies and are not committed. For a fresh machine, install PostgreSQL from its official distribution or use the Compose database described below.

```powershell
# Repository root; one-time frontend dependencies if needed.
Push-Location frontend
npm ci
Pop-Location

# Starts local PostgreSQL, builds the frontend, and runs Rust in this terminal.
./scripts/run-dev.ps1
```

`scripts/rust-env.ps1` helps Windows find installed MSVC and Windows SDK libraries. `scripts/configure-dev.ps1` generates two distinct random secrets once in `.local/dev-secrets.json` and sets environment variables without printing credentials. Keep this file across restarts. These helpers change the current shell environment, not system settings. The PostgreSQL helper binds only `127.0.0.1:55432`; it uses trust authentication for this local-only cluster and must not be reused for a public database.

The verification session also starts a hidden preview on port 8000 and records its process metadata in `.local/demo-process.json`. Use `./scripts/stop-preview.ps1` before starting your own foreground `run-dev.ps1` session on that port. The stop helper verifies the recorded executable belongs to this workspace and leaves PostgreSQL/data intact. Stop a foreground development run with Ctrl+C.

For manual setup on any platform, create a database, set the variables below, then run `cargo run --release --locked` from `backend/`. Build the frontend from `frontend/` with `npm ci` and `npm run build`. The backend serves `frontend/dist` by default when run from the backend directory. To work on React separately, run `npm run dev`; Vite proxies `/api` and `/health` to loopback port 8000.

| Variable | Meaning |
|---|---|
| `DATABASE_URL` | Required PostgreSQL URL for the application database. |
| `POLYNTU_QUOTE_SECRET` | Required random signing secret, at least 32 characters; persistent across restarts. |
| `POLYNTU_ADMIN_TOKEN` | Required separate random administrator token, at least 32 characters. |
| `POLYNTU_DEMO_MODE` | `true` enables demo accounts, deterministic observations and virtual clock controls. Defaults off. |
| `POLYNTU_BIND` | Defaults `127.0.0.1:8000`. Demo mode rejects a non-loopback bind. |
| `POLYNTU_CORS_ORIGINS` | Comma-separated frontend origins; defaults to localhost/127.0.0.1 port 5173. |
| `POLYNTU_FRONTEND_DIR` | Built static files; defaults `../frontend/dist`. |
| `RUST_LOG` | Tracing filter; e.g. `polyntu=info,tower_http=debug`. Do not log bearer tokens or quote bodies. |
| `VITE_API_URL` | Optional frontend API host. Empty uses the same origin or Vite proxy. |
| `TEST_DATABASE_URL` | Integration-test administrative database URL; account needs CREATE DATABASE permission. |

Use separate databases for demo and non-demo operation. Mode is fixed when the database is initialized; startup rejects a mode mismatch, even when the clock offset is zero. All workers sharing a database must use the same mode and secrets. Demo time advances are bounded to a cumulative ten-year offset.

SQLx applies `backend/migrations/` automatically. Do not edit an already-applied migration: add a new numbered migration. Do not delete a v2 database to work around a migration error after users have traded. See the migration document for recovery.

Verification commands:

```text
cargo fmt --manifest-path backend/Cargo.toml -- --check
cargo test --manifest-path backend/Cargo.toml --lib --locked
cargo test --manifest-path backend/Cargo.toml --test numerical --locked
cargo test --manifest-path backend/Cargo.toml --test integration --locked -- --test-threads=4
cargo clippy --manifest-path backend/Cargo.toml --all-targets --locked -- -D warnings
cargo bench --manifest-path backend/Cargo.toml --bench engine --locked
```

Run `npm run lint` and `npm run build` in `frontend/`. Run `python scripts/generate-math-fixtures.py` only when intentionally regenerating the independent numerical fixtures. That script uses Python's standard library, with 80-digit Decimal precision and a fixed seed.

Integration tests create uniquely named `polyntu_test_<UUID>` databases and drop only their own database on success. A failed test may leave its isolated database for inspection; it does not modify the application's database. The test suite exercises HTTP, balances, multiple simultaneous transactions, injected database failures, restarts, data revision order, final credits and reconciliation. Use four test threads to keep connection usage bounded.

The optional `scripts/benchmark.mjs` uses a dedicated backend/database and `BENCH_URL` (default loopback port 18000), `POLYNTU_ADMIN_TOKEN`, `BENCH_SECONDS` (default 600) and `BENCH_REPORT` (default `.local/performance.json`). It creates 100 markets, 200 accounts and 10,000 positions, holds 200 SSE clients, and schedules 100 quote requests plus 20 trade workflows per second. Half the run spreads traffic and half concentrates 80% on one instance. Reports distinguish committed trades, expected conflicts, unexpected errors, dropped scheduling work and p50/p95/p99. Node timers are approximate; use the actual submitted counts in the report when assessing achieved rates. Do not aim this workload at a user database.

On this Windows workspace, `./scripts/run-benchmark.ps1` creates an isolated database, starts a hidden release server on port 18000, runs the workload, and stops that server. It retains the database and `.local` report/logs for inspection. Port 18000 must be free. Run `./scripts/run-benchmark.ps1 -Workload benchmark-settlement.mjs` for a separate 10,000-claim resolution workload with 200 event streams and concurrent quote/trade traffic on still-open instances. That workload uses real fixed timestamps, uploads final evidence and verifies each credit through private portfolio responses.

For a container setup, `compose.yaml` starts PostgreSQL and the application with demo mode off. Supply `POSTGRES_PASSWORD`, `POLYNTU_QUOTE_SECRET` and `POLYNTU_ADMIN_TOKEN` in the shell environment, then run `docker compose up --build`. Provision accounts and market definitions through authenticated admin endpoints. The Compose configuration is provided for portability; actual verification on this task's host used native Windows binaries.

The project prohibits computer/browser automation. Verification here uses source review, builds, Rust tests and HTTP requests. Open the local URL manually to review the interface.
