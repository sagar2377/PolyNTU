# Detailed developer documentation

This directory is the code-level companion to the concise [project overview](../overview.md). It is intended for engineers who need to review implementation details, trace behaviour to source, or safely change the application.

Start with the repository-level [developer guide](../developer-guide.md), then use the relevant reference:

| Document | Purpose |
|---|---|
| [Backend](backend.md) | Rust modules, service composition, domain types, and error flow |
| [Database](database.md) | PostgreSQL tables, constraints, triggers, indexes, and migration order |
| [Trading and accounting](trading-and-accounting.md) | LMSR arithmetic, quote semantics, transaction order, ledger entries, and reconciliation |
| [Evidence and settlement](evidence-and-settlement.md) | Observation normalization, finalization, resolution, voids, and claim processing |
| [API implementation map](api-reference.md) | Route-to-handler-to-service traceability; pair with the public [API contract](../api.md) |
| [Frontend](frontend.md) | React pages, client state, browser persistence, polling, and receipt recovery |
| [Security and privacy](security-and-privacy.md) | Trust boundaries, authentication, authorization, sensitive data, and deployment gaps |
| [Operations](operations.md) | Process model, scheduler, observability, reconciliation, backup, and incident checks |
| [Testing and verification](testing-and-verification.md) | Test inventory, invariant coverage, fixture generation, benchmarks, and reproduction commands |
| [Code traceability](code-traceability.md) | Human-review matrix from product behaviour to source, schema, tests, and documentation |
| [Use case model](use-cases.md) | Actors, use case register, business rules, and detailed flows for the planned platform |
| [Glossary](glossary.md) | Domain, pricing, accounting, evidence, and lifecycle terms |

These documents describe the working tree audited on 9 September 2026. The [documentation register](../README.md) explains authority, maintenance, and the uncommitted-baseline caveat. The [use case model](use-cases.md) is the exception to the working-tree rule: it describes the planned platform and marks each use case as existing, partial, or new.
