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
| [Verification and traceability](../verification.md) | Repository-level test inventory, workloads, evidence record, reproduction commands, and the requirements-to-code matrix |
| [Use case model](use-cases.md) | Actors, use case register, business rules, and detailed flows for every registered use case |
| [Glossary](glossary.md) | Domain, pricing, accounting, evidence, and lifecycle terms |

These documents describe the working tree audited on 9 September 2026. The [documentation register](../README.md) explains authority, maintenance, and the uncommitted-baseline caveat. The [use case model](use-cases.md) describes the implemented platform: every registered use case is marked existing and carries a detailed flow.
