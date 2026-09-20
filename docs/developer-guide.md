# PolyNTU developer guide

This guide is the entry point for engineers who need to understand, review, modify, or operate the complete active application. It explains relationships between files; the linked topic documents contain the detailed contracts.

## Recommended reading order

1. [Architecture](architecture.md) for service boundaries and transaction ownership.
2. [Backend code reference](developer/backend.md) for every active Rust module.
3. [Database reference](developer/database.md) for tables, triggers, indexes, and migrations.
4. [Trading and accounting](developer/trading-and-accounting.md) for LMSR arithmetic, quotes, locking, ledger transfers, and idempotency.
5. [Evidence and settlement](developer/evidence-and-settlement.md) for category evaluation and worker behaviour.
6. [API contract](api.md) and [API implementation map](developer/api-reference.md).
7. [Frontend reference](developer/frontend.md) for React state, browser storage, polling, and receipt recovery.
8. [Security and privacy](developer/security-and-privacy.md).
9. [Operations](developer/operations.md) and [development setup](development.md).
10. [Verification and traceability](verification.md) for the test inventory, workloads, evidence record, and the requirements-to-code matrix.
11. [Use case model](developer/use-cases.md): actors, the use case register, business rules, and detailed flows for every registered use case, all implemented in the current build.

Use the [glossary](developer/glossary.md) when a domain or accounting term is unfamiliar.

## Active code boundaries

| Area | Primary files |
|---|---|
| Process startup | `backend/src/main.rs` |
| HTTP routing | `backend/src/api.rs` |
| Authentication/signing | `backend/src/auth.rs` |
| LMSR arithmetic | `backend/src/amm.rs` |
| Domain rules | `backend/src/market.rs` |
| Quote/trade execution | `backend/src/execution.rs` |
| Persistence and read models | `backend/src/store.rs` |
| Evidence and settlement | `backend/src/resolution.rs` |
| Scheduling | `backend/src/worker.rs` |
| Error translation | `backend/src/error.rs` |
| Database invariants | `backend/migrations/*.sql` |
| Browser client | `frontend/src/` |

The application crate forbids `unsafe` code. Category rules never move balances or calculate prices. The frontend never decides whether a trade or result is valid.

## Non-negotiable invariants

- Outcome sets contain 2–8 mutually exclusive entries.
- User, reserve, and treasury balances remain nonnegative.
- Instance inventory equals the sum of positions for each outcome.
- Each trade uses one market version and commits at most once.
- The reserve covers the largest possible unresolved obligation.
- Every balance change is represented by a balanced ledger transfer.
- Published definitions and final outcomes do not change.
- One account receives at most one settlement claim per instance.
- The sum of all account balances remains zero because issuance is the only negative balancing account.

If a proposed change weakens one of these invariants, it requires an explicit architectural decision, schema review, migration, tests, and documentation update.

## Common change paths

| Change | Start here | Also review |
|---|---|---|
| Add a category | `market.rs` and [market-type guide](adding-a-market-type.md) | migration category constraint, frontend label, tests, evidence docs |
| Change pricing limits | `amm.rs` | ADR 0002, fixtures, execution, API, UI validation |
| Add an endpoint | `api.rs` | request type, store/service method, API docs, auth tests |
| Change accounting | migrations and `store.rs` | execution, settlement, reconciliation, concurrency tests |
| Change resolution | `market.rs` and `resolution.rs` | worker queries, immutable rules, settlement tests |
| Change frontend flow | `frontend/src/` | API contract, storage recovery, accessibility, build/lint |
| Change configuration | `main.rs` and scripts | development, container configuration, security docs |

## Source-of-truth convention

The current implementation tells reviewers what the application does today. Approved requirements tell them what it should do. When those differ, document both and open an implementation issue; do not silently rewrite requirements to match a bug. Historical proposals and the archived AI rewrite notes explain how the project arrived here but do not override the current contract.

