# Operations and troubleshooting

The repository currently supports local development and isolated benchmarks. It does not contain a complete production operations platform. This guide documents safe diagnosis and recovery using the implemented controls.

## Operating modes

| Mode | Intended use | Important restrictions |
|---|---|---|
| Demo | Loopback academic demonstration | Public demo enrollment, virtual clock, deterministic evidence; backend rejects non-loopback bind |
| Non-demo | Manual/admin-provisioned evaluation | Real wall clock and authenticated manual evidence; still lacks production hardening |
| Benchmark | Dedicated temporary database/server | Never point workload scripts at user data |

Mode is persisted at database initialization. Do not toggle one database between demo and non-demo.

## Startup checklist

1. Confirm the intended database and mode.
2. Confirm PostgreSQL is reachable and has enough connections.
3. Set distinct persistent quote/admin secrets of at least 32 high-entropy characters.
4. Confirm the bind/CORS/frontend paths.
5. Back up an existing v2 database before new migrations.
6. Start the application and inspect migration/startup errors.
7. Request `GET /health` and `GET /api/v2/config`.
8. In non-demo mode, provision a controlled test account/instance and verify a small lifecycle before opening use.

## Health and signals

`GET /health` proves that the process can execute a simple PostgreSQL query. It does not prove worker progress, reconciliation, reserve sufficiency, SSE delivery, provider availability, or frontend correctness.

Useful built-in checks:

| Check | What it tells you |
|---|---|
| `GET /api/v2/config` | Process mode and authoritative database-adjusted time |
| `GET /api/v2/instances/{id}` | Effective state, version, evidence, result, and cutoff times |
| `GET /api/v2/admin/reconcile` | Ledger/inventory/reserve/net-unit consistency |
| `POST /api/v2/admin/worker/tick` | Attempts due work immediately and reports accounts settled |
| Server logs | Startup, request, worker, evidence, settlement, and database failures |

There are no metrics, readiness/liveness split, worker heartbeat table, alert rules, or audit-view endpoint.

## Normal lifecycle checks

For one instance, operators should expect:

1. `open` and tradable before close;
2. effective/public `closed` at cutoff;
3. persisted `closed` after worker cycle;
4. selected evidence after a due simulator/manual ingestion;
5. `resolving` while batches remain; and
6. `resolved` or `voided` after the final release.

Version advances on every trade, evidence pointer change, suspension, close, result fixation, and terminal transition.

## Application will not start

### Missing configuration

Errors name missing `DATABASE_URL`, quote secret, or administrator token. Load the intended environment without printing secret values.

### Invalid secrets

The quote/admin values must each be at least 32 characters and differ. Replace weak development values only before issuing outstanding quotes; rotation is not implemented.

### Demo bind rejection

Demo mode can bind only to a loopback IP. Use `127.0.0.1:8000`, or disable demo mode against a separate database for a controlled non-demo evaluation.

### Mode mismatch

If startup says the database was initialized in another mode or contains a demo offset, do not edit settings manually. Select/create the correct separate database.

### Migration failure

Stop and retain the error plus a database backup. Verify which SQLx migrations are recorded and whether the binary matches the schema. Never modify an applied migration or delete a database containing acknowledged v2 activity to force startup.

## Port 8000 is occupied

If the workspace verification preview is recorded, run `./scripts/stop-preview.ps1`. It validates the recorded executable before stopping it. Otherwise identify the listener using platform diagnostics and decide which application should own the port; do not terminate unrelated processes blindly.

## A trade response was interrupted

Do not create a new idempotency key. The browser's pending-trade notice should reopen the original instance and retry the saved body/key.

Interpret the result:

- receipt: the original trade committed exactly once;
- 409/400/404/422: definitive rejection; the current UI clears the pending request;
- another connection/500 failure: retain and retry later with the exact same data.

If browser storage was cleared before recovery, the backend has no endpoint to search idempotency receipts by user-visible key. Preserve account token and pending request information privately during incident handling.

## A market is closed but not settling

Check, in order:

1. authoritative `server_time_ms` against finalization/deadline;
2. persisted/effective instance state;
3. whether selected evidence exists and has an evaluated result;
4. whether the observation is complete/final and matches source/window;
5. application logs for worker errors;
6. one authenticated manual worker tick; and
7. reconciliation.

A closed manual instance with no complete evidence should wait until its published deadline, then void. Do not invent or backdate evidence to accelerate it.

## Evidence is rejected

Common causes are:

- observation window has not ended;
- manual deadline has passed;
- source or exact window differs from publication;
- event ID reused with changed content;
- source revision already exists;
- observation identifiers/units do not match the rule;
- evidence is incomplete but was expected to resolve; or
- result is already fixed.

Use a new higher revision for a genuine correction. Never change content under an existing event ID.

## Reconciliation returns `ok:false`

Treat this as an incident and stop/suspend new trading before attempting repair.

1. Save the complete reconciliation response, time, application version, and relevant logs.
2. Take a PostgreSQL backup/snapshot without discarding current data.
3. Identify which count is nonzero: account, inventory, reserve, or net units.
4. Query ledger transfers/entries, accounts, positions, instance inventory/result, and claims in one consistent snapshot.
5. Determine whether the mismatch is data corruption, manual database modification, migration incompatibility, or an application bug.
6. Design a reviewed forward repair preserving append-only history, normally through a compensating migration/transfer rather than UPDATE/DELETE.
7. Re-run reconciliation and the relevant integration reproduction before reopening.

The current application does not implement automated repair. Do not disable immutable triggers or nonnegative constraints as a first response.

## Settlement repeatedly fails

Look for reserve-invariant or database errors. Because a claim batch is atomic, a failed batch should not partially credit accounts. Existing committed claims remain safe and the next tick resumes unclaimed accounts.

If reserve reconciliation fails, follow the incident process above. If a transient lock/connection failure occurs, restore database availability and allow the worker to retry.

## Worker appears inactive

The default worker logs a cycle-level error and continues after one second. Per-instance simulator/settlement errors are logged and other rows continue. There is no persisted heartbeat.

Confirm the process is the expected binary, logs are advancing, database time is correct, due rows meet the actionable query, and manual `worker/tick` produces the same behaviour. Multiple processes may run workers safely at the data level, but production connection budgets and orchestration are not established.

## SSE or stale UI

The UI polls even when SSE is unavailable. Check instance snapshot directly, then event endpoint connectivity and logs. A reconnecting client should use the last event ID and refetch a snapshot.

Outbox rows are never compacted. Monitor table size and index health. Do not delete rows until a retention window, reconnect contract, backup policy, and migration are agreed.

## Backup and restore

Back up the complete PostgreSQL database with a method appropriate to the target deployment. The backup must preserve migrations, settings/simulation secret, all accounts/ledger, instances, positions, trades, idempotency responses, evidence, claims, events, and audit.

Restore into an isolated database first, start a matching binary/mode/secrets, run reconciliation, and test receipt lookup plus unfinished settlement. Quote secret/account tokens are external secrets and require their own secure recovery plan.

The repository has no automated backup schedule or restore test script. The legacy SQLite file is not a v2 backup.

## Secret loss

- Lost quote secret: outstanding quote tokens cannot be verified. Existing committed trades/receipts remain in PostgreSQL, but safe rotation/multiple active signing keys are not implemented.
- Lost administrator token: no in-application recovery. Replace deployment configuration only through a controlled operator process.
- Lost account token: no account recovery or token reset.
- Lost simulation secret: future demo observations may change after restore if the settings row is not preserved.

Document these limitations before relying on persistent user access.

## External data interruption

Live feeds are not implemented. When they are added, missing or questionable data must follow the published completeness/deadline policy rather than being filled with simulator output. Current planning assumes suitable normalized observations can be obtained; provider-specific operations should be documented with the adapter.

## Safe shutdown and deployment

Ctrl+C gracefully stops Axum and then aborts the worker task. Database transactions either commit or roll back. For deployment, stop routing new requests, allow in-flight HTTP work to finish, retain the database, start the compatible binary, confirm health/time/reconciliation, and observe worker progress.

Never roll back by replacing PostgreSQL with the archived SQLite database.

## Missing production capabilities

Before public operation, add monitoring/alerts, structured correlation fields, audit viewing/retention, backup automation and restore drills, controlled migrations, secrets rotation, incident ownership, worker health, rate limits, TLS/proxy policy, and capacity testing in the target environment.
