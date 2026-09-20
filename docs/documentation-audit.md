# Documentation audit report

Status: **completed baseline audit**  
Audit date: **9 September 2026**  
Repository audited: **current working tree of PolyNTU**

## Purpose

This report records how the documentation was checked and rewritten so a human maintainer can distinguish implemented behaviour, historical context, measurements, assumptions, and future work. It is an audit of documentation accuracy and coverage, not a security certification or formal proof of the application.

## Baseline and scope

The audit used the complete working tree rather than only Git `HEAD`. This matters because the Rust rewrite was still uncommitted: `HEAD` identified the earlier Python application at `ae15584`, while the Rust backend, PostgreSQL migrations, revised React client, scripts, infrastructure files, tests, and earlier AI-generated rewrite notes existed as working-tree changes.

The review covered:

- all repository-authored Markdown under the root, `docs/`, `frontend/`, and `legacy/`;
- every active Rust module under `backend/src/`;
- all four PostgreSQL migrations and the reserve-reconciliation query;
- the React entry point, pages, trade component, API helper, and styles;
- Cargo/npm configuration, Docker/Compose, CI, and local/benchmark scripts;
- unit, numerical, and PostgreSQL integration test source;
- the 384 numerical fixtures and three retained benchmark JSON reports; and
- the archived Python source and SQLite-preservation statements needed to verify migration documentation.

Third-party dependency documentation under tool caches, virtual environments, package directories, and build output was excluded because the project does not maintain it.

## Audit method

1. Inventory existing documents and record their intended audience and status.
2. Read source, migrations, scripts, tests, and raw retained results before rewriting claims.
3. Trace HTTP routes, environment variables, data structures, transaction order, lifecycle rules, accounting, and worker behaviour from implementation to documentation.
4. Treat benchmark numbers as measured evidence only when they matched the retained JSON.
5. Label unverified operations, product gaps, provider assumptions, and historical proposals explicitly.
6. Preserve the earlier AI rewrite/progress documentation before replacing the active files.
7. Validate relative links, headings, fenced blocks, JSON syntax, and Git whitespace after the rewrite.

## Documentation model implemented

The active documentation now has two depths:

- [General overview](overview.md) and the repository [README](../README.md) explain the product, user journey, architecture shape, lifecycle, safeguards, and current limitations.
- The [developer guide](developer-guide.md) and [detailed developer index](developer/README.md) lead to code-level references for the backend, database, API, trading/accounting, evidence/settlement, frontend, security/privacy, operations, testing, traceability, and terminology.

The [documentation register](README.md) assigns one primary maintained explanation to each subject and records audience, status, and maintainer role. “Canonical for” means “the first maintained document to update and consult for this subject”; it does not mean the file is unquestionable or more authoritative than observable code, schema constraints, or approved requirements.

## Material corrections

| Earlier issue | Audited documentation |
|---|---|
| Trade locking described a position-row lock that does not exist | Records idempotency `FOR UPDATE`, then instance `FOR UPDATE`, then sorted account `FOR NO KEY UPDATE`; the instance lock serializes position changes. |
| Administrator auditing was described too broadly | Lists the covered lifecycle/evidence actions and explicitly identifies account creation, reconciliation calls, and the worker-tick call as not independently audited. |
| The migration archive was said to contain 35 Python files | Records 34 files with the `.py` extension and separates requirements/documentation from that count. |
| Administrator account creation appeared demo-only | Records that `/admin/accounts` works in either mode; public `/auth/demo` is demo-only. |
| Portfolio pagination could be read as one combined result stream | Explains that the same clamped `limit` and `offset` are independently applied to positions and settlement claims. |
| Quote calculation was described as wholly deterministic | Separates deterministic calculation for fixed inputs from time-dependent expiry/server timestamps and HMAC token output. |
| Public evidence privacy was understated | States that selected normalized evidence and its reference are public and must not contain identities, tokens, private URLs, or raw attendance records. |
| Historical proposals and current behaviour were intermixed | Marks the old AI material and Python backend non-canonical and links current claims to code, migrations, tests, or evidence. |

## Coverage by relevant category

| Category | Primary documentation | Audit outcome |
|---|---|---|
| Product purpose and concepts | [Overview](overview.md) | Current direct-outcome model and simulated-unit boundary documented. |
| Architecture and lifecycle | [Architecture](architecture.md) | Runtime, ownership boundaries, transactions, worker, events, and recovery documented. |
| Local setup and configuration | [Development](development.md) | Toolchain, all six `POLYNTU_*` variables, database, scripts, checks, containers, and CI documented. |
| HTTP contract | [API](api.md) | All 20 active routes plus three explicit retired routes, auth, errors, request/response shapes, and SSE documented. |
| Backend source | [Backend reference](developer/backend.md) | Every active Rust module and important call relationship documented. |
| Database | [Database reference](developer/database.md) | Migrations, tables, constraints, triggers, indexes, locks, reconciliation, and backup scope documented. |
| Pricing, trading, and accounting | [Trading reference](developer/trading-and-accounting.md) | Units, LMSR, rounding, funding, quotes, idempotency, locking, ledger flows, settlement, and reconciliation documented. |
| Evidence and settlement | [Evidence reference](developer/evidence-and-settlement.md) | Typed observations, revision rules, finalization, voids, batches, simulator, and provider boundary documented. |
| Frontend | [Frontend reference](developer/frontend.md) | Pages, API helpers, state, polling/SSE, local storage, and uncertain-trade recovery documented. |
| Security and privacy | [Security reference](developer/security-and-privacy.md) | Trust, tokens, signed quotes, public evidence, current controls, threats, and production gaps documented. |
| Operations | [Operations reference](developer/operations.md) | Startup, health limits, incidents, reconciliation, settlement, backup/restore, secret loss, and safe deployment documented. |
| Tests and measurements | [Verification record](verification.md) | Test inventory, raw reports, environment, boundaries, reproduction commands, and unestablished claims documented. |
| Extension process | [Adding a market type](adding-a-market-type.md) | Domain, schema, UI, evidence, privacy, tests, operations, and documentation checklist provided. |
| Migration and history | [Migration policy](migration.md) | Python archive, database preservation, cutover, continuity, and forward recovery documented. |
| Architectural rationale | [Decision records](decisions/README.md) | Rust, LMSR/accounting, and evidence/settlement decisions indexed with status. |
| Human verification | [Verification record](verification.md) | Requirements mapped to implementation, database enforcement, tests, and known gaps. |

## External-data treatment

The active build has no live provider adapters. Candidate sources named during planning, including NEA and OmniBus, are mentioned only as future inputs. Their access, identifiers, coverage, correction, and reliability challenges are acknowledged without designing provider-specific contracts. The current documentation follows the agreed working assumption that an authorized adapter can obtain and normalize the required evidence later.

## Preserved previous documentation

The earlier documentation written by the AI development agent to plan and track its rewrite is preserved under [`archive/ai-agent-rewrite-progress/`](archive/ai-agent-rewrite-progress/). Its previous repository paths are mirrored beneath `repository/`, and copies of its three referenced benchmark reports keep historical links usable. The archive is intentionally non-canonical: it is evidence of the rewrite process, not the current API or operations manual.

The retired Python prototype and its original detailed notes remain separately under [`../legacy/python-backend/`](../legacy/python-backend/).

## Validation results

- Every maintained relative Markdown link and every archived relative Markdown link resolves.
- Every active Markdown document has an H1 heading.
- All fenced code blocks are balanced.
- All active and archived benchmark JSON parses successfully.
- Archived benchmark copies have the same SHA-256 hashes as the active raw artifacts.
- `git diff --check` reports no whitespace errors; Git reports only the repository's expected Windows line-ending conversion warnings.
- The route table matches the Axum router: 20 active endpoints, including health, plus three explicit retired endpoints.
- The configuration table covers all six application `POLYNTU_*` environment variables in `main.rs`.

No application code was changed during this documentation pass, and application tests were not rerun solely for Markdown changes. The [verification record](verification.md) distinguishes earlier recorded results from checks that still require reproduction.

## Remaining review boundaries

The audit could not establish visual browser correctness, a successful hosted CI run, Docker runtime behaviour on this host, backup restoration, public-production security, campus-network performance, long-duration saturation, or live provider accuracy/availability. These are labelled as gaps rather than implied guarantees.

Re-audit the documentation after the Rust rewrite is committed, after schema/API changes, when a live provider adapter is introduced, or before any public deployment.
