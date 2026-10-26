# Persistent data design - Lab 3

This document maps the ten design entities of the [class model](../docs/developer/class-diagram.md) onto the implemented PostgreSQL schema, records how each design association and value type is stored, and lists which invariants the database enforces itself. The maintained references are the [database reference](../docs/developer/database.md) and the transaction and retention sections of [architecture.md](../docs/architecture.md); this submission write-up links to them rather than duplicating their detail.

## Design classes and their storage

The mapping follows domain identity rather than one-table-per-class: each of the ten entities that has independent identity and its own lifecycle gets a table, and Outcome is embedded because its identity is only meaningful inside its instance. The `id` is a TEXT key throughout, generated as a UUID by the application for user-visible rows.

| Design entity | Storage | Identity |
|---|---|---|
| Account | `accounts` table | `id`: UUID for users, `reserve:<instance>`, or a fixed platform ID |
| VerificationRequest | `verification_requests` table | `id` |
| MarketSeries | `market_series` table | `id`, which is also the template ID of its brackets |
| MarketInstance | `instances` table | `id` |
| Outcome | Embedded in `instances`: the `outcomes` JSONB array of id/label objects plus the parallel `inventory` BIGINT[] of outstanding shares. There is no outcomes table | Instance ID plus array index; `outcomeId` is unique within the instance |
| Position | `positions` table | Composite primary key `(account_id, instance_id, outcome_index)` |
| Trade | `trades` table | `id`; `UNIQUE(instance_id, instance_version)` additionally identifies the consumed market version |
| Evidence | `evidence` table | `id`; `(instance, source, event_id)` and `(instance, source, source_revision)` are also unique |
| SettlementClaim | `settlement_claims` table | Composite primary key `(instance_id, account_id)` |
| LedgerTransfer | `ledger_transfers` table | `id`; the business `reference` is unique |

Two supporting tables sit outside the entity boxes: `templates` provides the browse grouping an instance references through `template_id`, and the singleton `settings` row holds the demo clock offset, mode, and simulation secret. The settings row is constrained to exist exactly once by a singleton primary key and CHECK, and initialization locks it to serialize competing process bootstrap. Position rows with zero quantity may remain after a complete sale, so the stored set is a superset of the open holdings the read model exposes. The materialized `balance_micros` on each account is a derived value kept in storage: the ledger remains the accounting authority, and reconciliation compares the two.

## What is not persistent

- The three containers are design classes over rows, not tables: `OutcomeSet` groups the embedded outcomes and inventory of one instance, while `PositionBook` and `TradeHistory` group the `positions` and `trades` rows of one account.
- Boundary and control classes have no storage. The nine boundaries are React components, HTTP routes, or the resolver contract; the controls are handler and Store groupings.
- A Quote is an ephemeral HMAC-signed value. Quoting writes nothing: not the inventory, not an idempotency row. Execution re-validates every claim inside its transaction, so a stale cache entry can only produce a quote that execution rejects.
- Read models are computed, not stored: portfolio, private trade history, price history, day probabilities, and the `ledger_entries` view that expands each transfer into a negative source and a positive destination entry.
- The instance cache and the SSE fan-out are runtime state only; the durable side of events is the outbox, which the notification listener replays after a restart.
- The outbox is persistent but is an operational record, not a domain entity: a `BIGSERIAL` resumable event cursor whose global sequence order drives SSE resumption. The same applies to `idempotency`, `admin_audit`, and `settings`.

Operational records differ from domain rows in their mutability rules. The append-only triggers cover the ledger, trades, evidence, claims, and audit. Idempotency rows are the intentional exception: one is inserted before the trade and updated once with the durable response before commit. Outbox rows carry no immutability trigger; the application only inserts them, with the bracket purge as the sole deleter, so manual updates and deletes there remain unsupported.

## Associations and referential design

| Design association | Realisation in the schema |
|---|---|
| Account submits VerificationRequest | Foreign key `verification_requests.account_id`. |
| Account creates MarketSeries (0..1) | Nullable foreign key `market_series.creator_account_id`; null means platform-owned. |
| MarketSeries groups MarketInstance | Nullable foreign key `instances.series_id` plus the immutable `bracket_start_ms` slot. The scheduler treats `(series_id, bracket_start_ms)` as its spawn uniqueness boundary; no schema constraint covers that pair, so duplicate-slot protection rests on the application existence check and `UNIQUE(template_id, close_ms)`. |
| MarketInstance holds one reserve Account | `instances.reserve_account_id`: NOT NULL, UNIQUE, foreign key. Reserve accounts survive purging, so the link is not composition. |
| MarketInstance creator Account (optional attribute) | Nullable foreign key `instances.creator_account_id`, immutable under `protect_instance`. |
| MarketInstance owns OutcomeSet/Outcome | Embedded columns; a CHECK ties `jsonb_array_length(outcomes)` to `cardinality(inventory)`. |
| MarketInstance receives Evidence | Foreign key `evidence.instance_id` for the revisions; the selected revision is the reverse pointer `instances.evidence_id`. The pair is circular and deferrable since migration 0011 so the purge can delete both tables in one transaction. |
| MarketInstance settles SettlementClaim | Foreign key `settlement_claims.instance_id`; the composite primary key makes the account/instance pair unique. |
| Account owns PositionBook/Position | Foreign key `positions.account_id`; the composite key leads with it. |
| Account owns TradeHistory/Trade | Foreign key `trades.account_id`, with an account/time index for paged history. |
| Outcome held or traded in Position/Trade | Embedded `outcome_index`, not a foreign key, because outcomes have no table. `positions` bounds the index to 0-7 by CHECK; `trades.outcome_index` carries no schema bound, so validity there relies on the transaction path. |
| LedgerTransfer from/to Account (two roles) | Two foreign keys, `from_account` and `to_account`, with `CHECK (from_account <> to_account)`; the `apply_transfer` trigger locks both rows in sorted ID order. |
| Browse grouping | Foreign key `instances.template_id`; a published series also owns a `templates` row under its ID so spawned brackets group together. |
| Administrator action on an instance | Logical reference: `admin_audit.instance_id` is deliberately not a foreign key, so an attempted action is retained even when no instance row is available. |
| Idempotency claim | Foreign key to `accounts`; operational, not a domain association. |

## Value-type encoding

| Design value type | Stored form |
|---|---|
| Money | BIGINT microcredits (`balance_micros`, `amount_micros`, `fee_micros`, `credit_micros`); 1 unit = 1,000,000. |
| Quantity | BIGINT millishares (`quantity_millis`, `inventory` elements); 1 share = 1,000, bounded 0-1,000,000,000. |
| Timestamp | BIGINT Unix milliseconds (`*_ms`); the demo offset is persisted in `settings` and added by the database clock read. |
| Enumerations | TEXT with CHECK ... IN (...): account kind and role, verification status, recurrence, series state, instance state, authority kind, data mode, trade side, transfer kind. |
| MarketRule, Outcome list, Resolution, EvidencePayload | JSONB: `rule`, `outcomes`, `result`/`evaluated_result`, `payload`. |
| Audit detail, idempotency receipt, active days | JSONB: `admin_audit.detail`, `idempotency.response`, `market_series.active_days` (ISO day numbers 1-7, migration 0012). |

No floating-point column appears anywhere in the schema: exact integer arithmetic is a deliberate choice for money and shares, with microcredit and millishare granularity fixed so the application-level rounding rules (fee share and void payout rounding down to the microcredit) stay representable. Timestamps are stored as plain integers rather than PostgreSQL timestamp types because the demo clock adds a persisted offset to the database wall clock, and all cutoff comparisons happen on that adjusted value.

## Invariants enforced in the schema

| Invariant | Mechanism | Migration |
|---|---|---|
| Non-issuance balances never negative | CHECK | 0001 |
| A token hash exactly on user accounts, never on platform or reserve accounts | CHECK | 0001 |
| Transfers are balanced movements: nonnegative amount, distinct endpoints | CHECK plus the `apply_transfer` trigger | 0001 (lock mode 0003) |
| Published instance definition, funding, creator, fee, and series fields immutable | `protect_instance` trigger | 0002 (creator 0005; fee/series/bracket 0009) |
| Published series definition, schedule, fee flag, and authority immutable | `protect_series` trigger | 0009 (authority 0010; active days 0012) |
| Instance version advances exactly once per update | `protect_instance` trigger | 0002 |
| Lifecycle order open -> closed -> resolving -> resolved/voided; terminal rows and fixed results immutable; inventory frozen after close | `protect_instance` trigger | 0002 |
| Ledger, trades, evidence, settlement claims, and admin audit append-only | `immutable_record` triggers; the only delete path is the purge transaction setting `polyntu.purge = 'on'` | 0001 (guarded delete 0011) |
| One trade per consumed instance version | UNIQUE `(instance_id, instance_version)` | 0001 |
| One final credit per account/instance, zero credit included | Composite PRIMARY KEY | 0001 |
| One reserve account per instance and one instance per reserve | NOT NULL UNIQUE foreign key | 0001 |
| Inventory 2-8 entries, each 0-1,000,000,000, matching the outcomes array | CHECKs | 0001 |
| Window ordering close <= observation_start < observation_end <= finalize_after < evidence_deadline | CHECK | 0001 |
| One instance per template and close time | UNIQUE `(template_id, close_ms)` | 0001 |
| One pending verification request per account | Partial unique index `WHERE status = 'pending'` | 0007 |
| NTU email: lowercase NTU pattern, unique, present exactly with password hash and role | CHECKs plus UNIQUE | 0006 (admin role 0008) |
| Series schedule and authority coupling: interval null exactly for one-time, recurring window minutes, creator authority needs key and creator, resolver authority needs endpoint | CHECKs | 0009, 0010 |
| One idempotency key per account | Composite PRIMARY KEY `(account_id, key)` | 0001 |

Two design invariants are enforced in the transaction path rather than the schema: an idempotency key is bound to one exact serialized request by the `request_hash` comparison inside the trade transaction, and bracket spawn uniqueness on `(series_id, bracket_start_ms)` rests on the application's existence check. The schema backstop for the second is `UNIQUE(template_id, close_ms)`, whose conflict the scheduler treats as a lost race.

## Transactions and locking

Each state change owns one short PostgreSQL transaction; the shared resource order is documented in the [locking model](../docs/developer/database.md#locking-model), and the paths below are specified in [architecture.md](../docs/architecture.md#transaction-boundaries).

| Path | Transaction scope and lock order |
|---|---|
| Quote | Read-only: cached instance and clock estimate, no writes and no idempotency row. Execution re-validates every claim. |
| Trade | One transaction: claim the account-scoped idempotency row (replaying a committed response on conflict), lock the instance row `FOR UPDATE`, then lock the participant and reserve accounts in sorted ID order `FOR NO KEY UPDATE`, recheck state, cutoff, quote, balance, holdings, and reserve coverage, insert the balanced transfer, and apply the remaining writes in one data-modifying statement before commit. Deadlock, serialization, and lock-timeout errors retry up to three times; clients keep the same idempotency key. |
| Instance creation | One transaction: validate the immutable specification, create the reserve account, transfer the subsidy, insert the instance, and write the `opened` event; direct creation also appends an audit row. |
| Series publication | Deliberately two: the series row, its template row, and the audit record commit first, then a one-time schedule publishes its bracket through instance creation; if funding fails, the series row is removed again. The model does not claim one transaction across both stages. |
| Evidence | Instance lock; validation, duplicate checks, append, selected-revision pointer, version advance, audit, and event in one transaction. Creator-signed resolutions and validated resolver answers enter through their own recorders under the same instance lock. |
| Settlement | Per instance, in batches of at most 100 unsettled accounts; locks the selected accounts together with the reserve, treasury, and creator, credits unique claims, and commits. Terminal state and reserve release happen when no claims remain. The worker settles independent instances concurrently in bounded chunks of eight; each instance still settles serially under its row lock. |
| Bracket purge | Batches of 20 per worker tick; sets the session-local `polyntu.purge = 'on'` flag and defers constraints, deletes the subtree, and both settings revert at commit. |
| Read models | Portfolio and reconciliation run in a single `REPEATABLE READ READ ONLY` transaction so the rows they join come from one consistent snapshot; they take no locks. |

Connection setup imposes a two-second lock timeout and a fifteen-second statement timeout on every pool connection, bounding how long any of these transactions can wait. Worker paths share the same rules with the addition of `FOR UPDATE SKIP LOCKED` batch closing of up to 100 due open instances per tick, so competing processes divide closure work rather than queue behind each other. Position rows take no explicit lock in execution; the instance row lock serializes every trade that could modify positions or inventory for that market, while the participant account lock separately serializes spending across markets.

## Recovery expectations

The persisted design is what makes the recovery model workable, since the database is the durable authority for every state-changing decision: a committed trade is recoverable by repeating its exact body and idempotency key; an uncommitted transaction leaves no partial ledger, position, trade, receipt, or event effects; evidence records and final results survive worker restarts; settlement resumes from accounts without an existing claim; and reconciliation detects divergence between materialized balances and the ledger, positions and inventory, or reserve coverage. A migration or deployment failure must be recovered forward against the same database; the archived SQLite prototype is never a valid rollback target.

## Retention

One rule covers current retention behaviour: terminal brackets of recurring series are disposable after a day, everything else stays. A bracket in state `resolved` or `voided` whose evidence deadline passed more than 24 hours ago is purged with its whole subtree - outbox rows, positions, trades, settlement claims, evidence, and the instance. One-time markets are never purged.

The append-only accounting history survives because every ledger transfer is shared between two accounts: drained reserve accounts and their ledger trails remain, as do admin audit and idempotency rows, neither of which references instances by foreign key. A reserve that still holds units blocks the purge, so a settlement anomaly surfaces instead of vanishing. No general compaction exists for the rows that survive, so outbox growth from one-time markets must be monitored before public deployment.

## Migration history as implementation evidence

| Migration | Design concern it records |
|---|---|
| `0001_outcome_markets.sql` | Initial schema: settings, accounts and ledger, templates and instances, positions and trades, evidence, settlement, outbox, audit, and the append-only triggers. |
| `0002_immutable_rules.sql` | `protect_instance`: published fields, version advance, lifecycle order, and the `release` transfer kind. |
| `0003_account_lock_mode.sql` | `FOR NO KEY UPDATE` account locking to avoid foreign-key lock-upgrade deadlocks. |
| `0004_private_simulation_seed.sql` | Private persisted simulator secret in `settings`. |
| `0005_trade_fees.sql` | Per-trade fees, the `fee` transfer kind, and immutable creator attribution. |
| `0006_ntu_accounts.sql` | NTU email, argon2 password hash, and role columns with coupling checks. |
| `0007_creator_verification.sql` | `verification_requests` and the one-pending-per-account index. |
| `0008_admin_role.sql` | The `admin` role value. |
| `0009_market_series.sql` | `market_series`, the instance series/bracket/fee columns, `protect_series`, and the extended `protect_instance`. |
| `0010_resolution_authority.sql` | Resolution authority fixed at series creation and added to the protected definition. |
| `0011_bracket_retention.sql` | Purge-guarded `immutable_record`, the deferrable circular evidence constraint, and the purge index. |
| `0012_bus_operating_days.sql` | `active_days` operating-day restriction on recurring series, folded into `protect_series`. |

The policy is to never edit an applied migration and to add a numbered file instead. SQLx records a checksum of each applied migration file in its migrations table, so an edited file no longer matches its recorded checksum and the migration run fails: the history is append-only by mechanism, not only by convention.

The schema is not squashed into a single file for three reasons. Applied databases hold the checksums of the twelve files, so a replacement file would fail verification. Existing databases upgrade by applying only the new files, while a squashed file describes only a fresh database and provides no upgrade path. And the sequence stays traceable: [ADR 0004](../docs/decisions/0004-trade-fees.md) cites migration 0005, [ADR 0005](../docs/decisions/0005-ntu-accounts-and-creator-roles.md) cites 0006-0008, [ADR 0006](../docs/decisions/0006-market-series-and-recurrence.md) cites 0009, and [ADR 0007](../docs/decisions/0007-resolution-authority.md) cites 0010. The repeated `CREATE OR REPLACE FUNCTION` pattern - `protect_instance` in 0002, 0005, 0009 and `protect_series` in 0009, 0010, 0012 - shows how a definition grows one migration at a time: each replacement widens the protected column set, and each step remains reproducible on a database that applied only the earlier files. Any change is verified against both fresh creation and upgrade.

## Production gaps

Current limitations recorded in the maintained references:

- One database role: application processes use a single database URL rather than separate least-privilege migration and runtime roles ([security and privacy](../docs/developer/security-and-privacy.md#database-controls)).
- No idempotency retention: idempotency rows are not expired or compacted, so the table grows with every distinct trade key.
- No general compaction: outbox rows outside the bracket purge are never removed, and operator monitoring is manual.
- Backup sensitivity: a useful backup must include the entire database, and backups carry token hashes, password hashes, public evidence payloads, the simulation secret, and operational history, so they must be protected as sensitive.
- No general ledger correction: corrections require a new compensating transfer, and no correction endpoint currently exists.
- Fixed categories: the `templates` category check must be migrated when a genuinely new category is added.

## References

- [Database reference](../docs/developer/database.md) - authoritative table-by-table and migration reference.
- [Architecture](../docs/architecture.md) - transaction boundaries, lifecycle and immutability, data retention, recovery model.
- [Class diagram and responsibilities](../docs/developer/class-diagram.md) - the ten entities, containers, and value types mapped onto storage.
- [Security and privacy](../docs/developer/security-and-privacy.md) - database controls and backup sensitivity.
- [Architecture write-up](architecture.md) and [design notes](steps-1-4-handoff.md) - the other Lab 3 contributions.
