# Developing PolyNTU

Status: **current contributor setup**

## Supported toolchain

| Dependency | Required/verified version | Purpose |
|---|---|---|
| Rust and Cargo | 1.88 or newer | Backend build, tests, clippy, formatting, benchmark |
| Native C/C++ build tools | Platform-compatible | Link dependencies on Windows and other platforms |
| PostgreSQL | 17 verified | Durable application and integration-test database |
| Node.js | 22 or newer; 25.7 used in the retained local run | React build, lint, and HTTP workloads |
| Python | Optional | Regenerating independent decimal fixtures only |

Use `cargo --locked` and `npm ci` so dependency resolution follows the committed lockfiles.

## Fastest Windows setup in this workspace

Portable PostgreSQL binaries may be installed under `.tools/postgres/pgsql/`, with cluster data under `.local/postgres-data/`. These directories are local-only and ignored by Git.

```powershell
# Repository root: install frontend dependencies once.
Push-Location frontend
npm ci
Pop-Location

# Start local PostgreSQL, configure secrets, build React, and run Rust.
./scripts/run-dev.ps1
```

The database listens only on `127.0.0.1:55432` and uses trust authentication. Never reuse this development configuration for a network-accessible database.

Stop a foreground application with Ctrl+C. If a previous verification session left a recorded hidden preview on port 8000, run:

```powershell
./scripts/stop-preview.ps1
```

The stop helper validates that the recorded executable belongs to this workspace and does not stop PostgreSQL or delete data.

## Manual setup on any platform

1. Create an empty PostgreSQL database.
2. Build the frontend:

   ```text
   cd frontend
   npm ci
   npm run build
   ```

3. Set `DATABASE_URL`, distinct persistent quote/admin secrets, and the intended mode.
4. From `backend/`, run:

   ```text
   cargo run --release --locked
   ```

When started from `backend/`, the default frontend directory is `../frontend/dist`. For live React development, use `npm run dev`; Vite proxies `/api` and `/health` to loopback port 8000.

## Environment variables

| Variable | Required | Meaning |
|---|---:|---|
| `DATABASE_URL` | Yes | PostgreSQL connection URL. |
| `POLYNTU_QUOTE_SECRET` | Yes | Persistent quote-signing secret of at least 32 characters. |
| `POLYNTU_ADMIN_TOKEN` | Yes | Separate administrator token of at least 32 characters. Must differ from the quote secret. |
| `POLYNTU_DEMO_MODE` | No | Exact string `true` enables demo enrollment, simulator evidence, and virtual clock controls. Defaults off. |
| `POLYNTU_BIND` | No | Socket address; defaults to `127.0.0.1:8000`. Demo mode rejects non-loopback addresses. |
| `POLYNTU_CORS_ORIGINS` | No | Comma-separated allowed origins; defaults to localhost and loopback port 5173. |
| `POLYNTU_FRONTEND_DIR` | No | Static build path; defaults to `../frontend/dist`. |
| `RUST_LOG` | No | Tracing filter, for example `polyntu=info,tower_http=debug`. |
| `VITE_API_URL` | No | Frontend API origin. Empty means same-origin/Vite proxy. |
| `TEST_DATABASE_URL` | Integration tests | Administrative PostgreSQL URL whose account may create databases. |
| `POSTGRES_PASSWORD` | Compose | PostgreSQL password used by `compose.yaml`. |
| `BENCH_URL` | Benchmarks | Dedicated benchmark server; defaults to `http://127.0.0.1:18000`. |
| `BENCH_SECONDS` | HTTP benchmark | Duration; defaults to 600. |
| `BENCH_REPORT` | Benchmarks | Output JSON path under `.local/` by default. |

Never log bearer tokens, administrator tokens, quote bodies, or `.local/dev-secrets.json`.

## Helper scripts

| Script | Behaviour |
|---|---|
| `scripts/rust-env.ps1` | Locates project-local Cargo and installed MSVC/Windows SDK libraries. Changes only the current shell environment. |
| `scripts/configure-dev.ps1` | Generates distinct persistent development secrets once and exports local settings. |
| `scripts/start-local-db.ps1` | Initializes/starts loopback PostgreSQL and creates the `polyntu` database if needed. |
| `scripts/run-dev.ps1` | Runs environment setup, database startup, frontend build, and a debug backend (`cargo run`, unlocked) for fast iteration. |
| `scripts/run-prod.ps1` | Runs the same pipeline with the locked, optimised release backend. |
| `scripts/stop-preview.ps1` | Stops only the verified hidden preview recorded by this workspace. |
| `scripts/generate-math-fixtures.py` | Rebuilds high-precision LMSR reference JSON using fixed-seed Python Decimal calculations. |
| `scripts/run-benchmark.ps1` | Builds the release backend and runs an isolated database/server workload on port 18000. |

## Database modes and migrations

Use separate databases for demo and non-demo operation. The mode is fixed when the database is initialized. All processes sharing a database must use the same mode and quote secret.

SQLx applies `backend/migrations/` during `Store::connect`. Never edit an already-applied migration. Add the next numbered migration and test upgrading a copy of realistic data. Do not delete a v2 database to hide a migration failure after any acknowledged trades.

For rollback and archive policy, see [migration](migration.md). For schema details, see [database reference](developer/database.md).

## Backend checks

From the repository root:

```text
cargo fmt --manifest-path backend/Cargo.toml -- --check
cargo test --manifest-path backend/Cargo.toml --lib --locked
cargo test --manifest-path backend/Cargo.toml --test numerical --locked
cargo test --manifest-path backend/Cargo.toml --test integration --locked -- --test-threads=4
cargo clippy --manifest-path backend/Cargo.toml --all-targets --locked -- -D warnings
cargo bench --manifest-path backend/Cargo.toml --bench engine --locked
```

Integration tests create uniquely named `polyntu_test_<UUID>` databases and drop only their own database after successful completion. A failed test can intentionally leave its isolated database for investigation. Four test threads bound connection use.

## Frontend checks

```text
cd frontend
npm ci
npm run lint
npm run build
```

The project instructions prohibit browser/computer automation. A human may open the local URL for visual review. Source inspection, builds, unit/integration tests, and HTTP checks remain the automated verification path.

## Fixture regeneration

Run fixture generation only when intentionally changing the numerical contract:

```text
python scripts/generate-math-fixtures.py
```

Review the resulting JSON diff, explain why expected amounts changed, and run both numerical and integration tests. Fixture regeneration must never be used merely to make a failing implementation test pass.

## Docker Compose

`compose.yaml` runs PostgreSQL 17 and the application with demo mode disabled. Supply secrets in the shell environment:

```text
docker compose up --build
```

Required values are `POSTGRES_PASSWORD`, `POLYNTU_QUOTE_SECRET`, and `POLYNTU_ADMIN_TOKEN`. The host mapping binds port 8000 to loopback. Provision accounts and instances through administrator endpoints.

The Compose file is a portability aid, not a documented production deployment. TLS, backups, monitoring, secret management, rate limiting, database maintenance, and public network policy remain operator responsibilities.

## Continuous integration

`.github/workflows/verify.yml` runs on pushes and pull requests using Ubuntu, PostgreSQL 17, Node 22, and Rust 1.88. It checks formatting, all Rust tests, clippy with warnings denied, npm install, frontend lint, and frontend build.

The retained verification session did not execute the workflow on this host. A future documentation check should add Markdown link validation and linting without treating generated/archive content as current contracts.

## Documentation change checklist

When implementation behaviour changes:

1. update the canonical topic document and API/schema examples;
2. update the relevant ADR if an accepted decision changed;
3. update tests and the [traceability matrix](developer/code-traceability.md);
4. add an entry to [changes](changes.md);
5. run internal link checks and the relevant build/test commands; and
6. record the commit and verification date in the documentation index.

