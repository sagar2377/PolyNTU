# PostgreSQL database reference

PostgreSQL stores all active v2 state. SQLx embeds and applies the numbered migrations during application connection. SQLite is used only by the archived Python prototype.

## Migration history

| Migration | Purpose |
|---|---|
| `0001_outcome_markets.sql` | Initial settings, account/ledger, market, position/trade, evidence, settlement, outbox, and audit schema. |
| `0002_immutable_rules.sql` | Protect published instance fields, enforce version/lifecycle updates, and allow reserve-release transfers. |
| `0003_account_lock_mode.sql` | Replace balance trigger account locks with `FOR NO KEY UPDATE` to avoid foreign-key lock-upgrade deadlocks. |
| `0004_private_simulation_seed.sql` | Add private database-persisted simulator secret. |

Never edit an applied migration. Add a numbered migration and verify both fresh creation and upgrade.

## Relationship overview

```text
settings

accounts <----- ledger_transfers -----> accounts
   ^                    |
   |                    +--> ledger_entries view
   |
templates <----- instances -----> reserve account
                    |
                    +--> positions ----> participant account
                    +--> trades -------> participant account
                    +--> evidence
                    +--> settlement_claims -> participant account
                    +--> outbox
                    +--> admin_audit (logical optional reference)

idempotency -> participant account
```

`admin_audit.instance_id` is intentionally not declared as a foreign key, allowing an attempted action to be retained even when no referenced instance row is available.

## `settings`

Exactly one row is allowed by the `singleton` primary key/check.

| Column | Meaning |
|---|---|
| `clock_offset_ms` | Nonnegative persisted demo-time offset. |
| `demo_mode` | Mode fixed when the database is initialized. |
| `simulation_secret` | Private random value initialized once; never returned by public views. |

Initialization locks this row to serialize competing process bootstrap.

## `accounts`

Account kinds are `issuance`, `treasury`, `user`, and `reserve`.

| Column | Notes |
|---|---|
| `id` | Text primary key; UUID for users, `reserve:<instance>`, or fixed platform ID. |
| `display_name` | Human-readable name. |
| `kind` | Constrained account role. |
| `token_hash` | Unique SHA-256 hex for users; null for non-users. |
| `balance_micros` | Materialized ledger balance. Nonnegative except issuance. |
| `created_ms` | Database wall-clock default for direct inserts; application normally provides transfer times separately. |

The constraint `(kind='user') = (token_hash IS NOT NULL)` prevents credentials on platform/reserve accounts and requires them on users.

## Ledger

### `ledger_transfers`

Each row represents one balanced movement. Required fields are source, destination, nonnegative amount, kind, unique reference, and time. Source and destination must differ.

Kinds are `issuance`, `grant`, `subsidy`, `buy`, `sell`, `resolution`, `release`, and `fee`.

The `apply_transfer` trigger locks both account rows in sorted ID order with `FOR NO KEY UPDATE`, subtracts from the source, and adds to the destination. Account constraints abort the entire transaction if a non-issuance balance becomes negative.

`immutable_ledger` rejects updates and deletes. Corrections require a new compensating transfer with a new unique business reference; no general correction endpoint currently exists.

Indexes support account/time reads by source and destination.

### `ledger_entries` view

The view expands each transfer into one negative source entry and one positive destination entry. Reconciliation sums these entries and compares them with materialized account balances.

## Market definitions

### `templates`

Templates have a stable text ID, one of five category keys, and a title. They organize instances and provide the uniqueness boundary for recurring close times. The initial schema category check must be migrated when a genuinely new category is added.

### `instances`

One row owns a concrete market occurrence:

- immutable identity, template/category/title/criterion/rule/outcomes/source/data mode;
- lifecycle and suspension state;
- five absolute times;
- fixed liquidity;
- millishare inventory array and version;
- unique reserve-account reference;
- optional immutable creator-account reference, used to split the settled fee pot; and
- selected result/evidence references.

Important constraints:

- state is `open`, `closed`, `resolving`, `resolved`, or `voided`;
- data mode is `simulated` or `manual`;
- liquidity is 10–100,000;
- inventory contains 2–8 nonnegative values at most 1,000,000,000;
- outcome-array length equals inventory cardinality;
- `close <= observation_start < observation_end <= finalize_after < evidence_deadline`; and
- `(template_id, close_ms)` is unique.

The partial due index covers nonterminal states; the browse index covers template and reverse close time.

The `protect_instance` trigger introduced in migration 0002 (creator attribution added in migration 0005):

1. rejects any change to published definition/funding/creator fields;
2. requires `version = old.version + 1` for every update;
3. rejects updates to terminal rows;
4. keeps a fixed result immutable;
5. rejects inventory changes after open state; and
6. permits only `open->closed->resolving->{resolved|voided}` lifecycle transitions.

Suspension, evidence pointer, and state can change only through version-advancing updates.

## Trading state

### `positions`

Primary key: `(account_id, instance_id, outcome_index)`. Quantity is 0–1,000,000,000 millishares and outcome index is 0–7. Application reads normally filter to positive quantities, but zero rows may remain after a complete sale.

An instance/account index supports settlement selection.

### `idempotency`

Primary key: `(account_id, key)`. `request_hash` binds a key to one exact serialized `TradeRequest`. `response` is null while the transaction is in progress and becomes the durable JSON receipt before commit.

Rows are not currently expired or compacted. Their foreign key to accounts participates in the lock behaviour addressed by migration 0003.

### `trades`

Trades record account, instance, outcome index, side, positive quantity, nonnegative all-in amount, the fee component (`fee_micros`, migration 0005), resulting instance version, engine version, and time. `(instance_id, instance_version)` is unique, enforcing one trade per consumed version. The settled fee pot of an instance is `sum(fee_micros)`.

The table is append-only. Account/time index supports private history.

## Evidence and resolution

### `evidence`

Evidence stores source/event/revision identities, receive time, parser version, normalized payload JSON, payload hash, and optional evaluated result.

Unique constraints prevent:

- another payload under the same `(instance, source, event_id)`; and
- another event under the same `(instance, source, source_revision)`.

The table is append-only. `instances.evidence_id` points to the highest accepted revision selected by the application.

### `settlement_claims`

Primary key `(instance_id, account_id)` guarantees one final credit per account/instance. It stores the exact credited microcredits and time and is append-only. A zero credit is still a processed claim.

### `outbox`

`BIGSERIAL id` is the resumable event cursor. Each row carries instance ID, resulting instance version, event type, and time. The index `(instance_id,id)` supports SSE polling.

Outbox records are durable but not immutable by trigger, and no retention/compaction job exists. Application code only inserts. Treat manual updates/deletes as unsupported until a documented retention policy exists.

### `admin_audit`

Append-only record with generated ID, action, optional instance ID, JSON detail, and time. It covers instance/resolution/clock operations described in the security guide, but not every administrator endpoint.

## Reserve reconciliation query

`backend/queries/reconcile_reserves.sql` counts underfunded instances differently by state/result:

- unresolved: reserve must cover the maximum outstanding outcome inventory;
- winner fixed: reserve must cover unclaimed winning positions; and
- void fixed: reserve must cover account-aggregated fractional credits for all unclaimed positions.

`Store::reconcile` combines that count with account-ledger mismatches, inventory-position mismatches, and nonzero total account balance.

## Locking model

| Resource | Lock/use |
|---|---|
| Settings | `FOR UPDATE` during initialization; atomic update during clock advance |
| Idempotency row | `FOR UPDATE` first in trade transaction |
| Instance row | `FOR UPDATE` for trade/evidence/suspension/settlement; `SKIP LOCKED` batch closing |
| Account rows | Sorted `FOR NO KEY UPDATE` before balance-sensitive actions |
| Position row | No explicit lock in execution; protected by the instance lock |

Keep transactions short and acquire shared resource classes in the documented order. Database connection setup imposes two-second lock and fifteen-second statement timeouts.

## Backup priorities

A useful v2 backup must include the entire PostgreSQL database, not selected application tables. The settings secret/offset, idempotency receipts, evidence, claims, outbox, migrations table, and ledger are all required for consistent recovery. Protect backups as sensitive because they contain token hashes, public evidence payloads, private simulation state, and operational history.
