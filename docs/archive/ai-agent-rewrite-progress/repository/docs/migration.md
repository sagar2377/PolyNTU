Migration from the options prototype to Rust outcome markets.

The original Python source has been moved into `legacy/python-backend/`. Its existing requirements and tests remain there for historical inspection. The active backend is `backend/Cargo.toml` plus `backend/src/`; it never imports or executes that archive. The old React calls/puts, Greeks and contract blotter have been replaced by outcome trading and a private portfolio.

The existing `backend/polyntu.db` file was retained. A pre-rebuild copy is also stored locally at `.local/legacy-backup/polyntu-before-rust.db`. Both files are ignored by Git. Their contracts lack authenticated ownership and funded balance movements, so they are not converted into new shares, credits, or settlement obligations. New PostgreSQL accounts receive an explicit ledger grant. No original database file was deleted.

Archive verification compared all 35 moved Python files against their original Git contents (normalizing line endings). The SQLite file and backup have matching SHA-256 hashes. The archive is retained for inspection; it is excluded from the active container build.

For another checkout, stop writes to the old application, back up its SQLite database, keep that backup read-only, and create a separate PostgreSQL database. Start the Rust service to apply migrations, choose explicit data mode and signing secrets, build the React frontend, and verify a complete quote/buy/sell/resolve flow before switching clients. Run reconciliation and the integration tests described in `development.md`.

Preserve the quote signing secret across restarts so in-flight quotes can still be verified and successful retries can retrieve stored receipts. Preserve the administrator credential separately. Bearer tokens are only returned during account provisioning; retain them privately if account access must survive clearing browser storage. Signing-key rotation and account recovery are not yet implemented.

After v2 accepts trades, its ledger is authoritative. Reverting application binaries must not discard that database or replace it with the legacy SQLite file. If a rollout fails, stop accepting new trades and restore a compatible Rust build against the same PostgreSQL data, or recover a verified PostgreSQL backup. Historical source availability is not permission to resume unfunded options contracts as v2 positions.
