# Migration and legacy policy

Status: **current policy for the completed options-to-outcome-market cutover**

## What changed

The original FastAPI/SQLite options implementation was moved to `legacy/python-backend/`. The active backend is `backend/Cargo.toml`, `backend/src/`, and the PostgreSQL migrations. It never imports or executes the archive.

The old React call/put controls, Greeks, Monte Carlo panels, and contract blotter were replaced by outcome selection, share quantity, exact quote preview, buy/sell confirmation, positions, settlement credits, and durable receipt recovery.

## Preserved files

- The archive contains 34 `.py` files from the tracked Python backend plus its requirements and documentation.
- The original SQLite database remains at `backend/polyntu.db` locally.
- A pre-rebuild copy remains at `.local/legacy-backup/polyntu-before-rust.db`.
- At the latest review, both database files had SHA-256 `5657FC4A8101682F3C324EB2434BACCF410E388E11325656F338217110B496F0`.
- The container excludes `legacy/`, local databases, `.local/`, and `.tools/`.

The previous wording “35 Python files” was incorrect: there are 34 files with the `.py` extension. Counts that include requirements or documentation must name those file types explicitly.

## Why legacy contracts were not converted

The Python contracts do not contain authenticated account ownership and balanced funded ledger movements compatible with v2. Treating them as new shares would invent financial history and settlement obligations. They remain historical records only.

V2 creates fresh accounts, explicit ledger grants, funded instance reserves, and new positions. PostgreSQL becomes authoritative after the first accepted v2 trade.

## Cutover procedure for another environment

1. Stop writes to the old application.
2. Create and verify a read-only backup of the SQLite database.
3. Provision a separate PostgreSQL database; never overwrite the archive.
4. Configure persistent distinct quote/admin secrets and the desired data mode.
5. Start the Rust service and allow SQLx migrations to complete.
6. Build and deploy the matching React frontend.
7. Provision test accounts and instances through authenticated endpoints.
8. Verify quote, buy, sell, close, evidence, settlement, receipt recovery, and reconciliation.
9. Disable old clients and creation routes before allowing v2 use.

## Secrets and continuity

Preserve the quote secret across restarts so outstanding signed quotes remain verifiable and stored idempotency responses remain reachable through the same request. Preserve the administrator token separately. Account tokens are returned once and require private external retention if browser storage may be cleared.

Key rotation and account recovery are not implemented. Document and test them before a public deployment.

## Failure and rollback

After v2 accepts a trade, do not replace its PostgreSQL database with SQLite or delete it to revert application code. If deployment fails:

1. stop accepting new trades;
2. retain and back up the current PostgreSQL database;
3. diagnose migrations and application compatibility;
4. restore a compatible Rust build against that same data, or restore a verified PostgreSQL backup; and
5. run reconciliation before reopening trading.

Historical source availability is not permission to resume unfunded option contracts as v2 positions.

## Documentation history

The first AI-generated migration narrative and implementation plan are retained in [`docs/archive/ai-agent-rewrite-progress/`](archive/ai-agent-rewrite-progress/). They explain the rewrite process but are not operational instructions.

