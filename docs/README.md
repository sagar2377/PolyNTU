# PolyNTU documentation

This index separates concise project documentation from the detailed material intended for maintainers and reviewers. When documents disagree, use the active implementation and database migrations to establish current behaviour, then record any difference from approved product requirements as an implementation gap.

## Choose a documentation depth

- [General overview](overview.md) explains the product, terminology, lifecycle, user journey, major safeguards, and known limitations without requiring Rust or PostgreSQL knowledge.
- [Developer guide](developer-guide.md) is the entry point for the detailed code, database, API, security, operations, and verification documentation.

## Documentation register

“Canonical for” identifies the primary maintained explanation of a subject. It does not imply that a document is infallible; each technical claim should remain traceable to code, migrations, tests, or retained evidence.

| Document | Canonical for | Audience | Status | Maintainer role |
|---|---|---|---|---|
| [`README.md`](../README.md) | Repository entry point and quick start | Everyone | Current | Project maintainer |
| [`overview.md`](overview.md) | Product-level system explanation | Reviewers and new contributors | Current | Project maintainer |
| [`developer-guide.md`](developer-guide.md) | Detailed documentation reading path | Developers | Current | Project maintainer |
| [`documentation-audit.md`](documentation-audit.md) | Audit scope, corrections, coverage, and verification boundaries | Reviewers and maintainers | Completed baseline audit | Documentation maintainer |
| [`architecture.md`](architecture.md) | Component boundaries and transaction model | Developers | Current | Backend maintainer |
| [`api.md`](api.md) | External HTTP contract | Frontend/backend developers | Current | Backend maintainer |
| [`development.md`](development.md) | Setup, configuration, and local commands | Contributors | Current | Project maintainer |
| [`verification.md`](verification.md) | Test and performance evidence | Reviewers and developers | Current evidence record | Verification owner |
| [`adding-a-market-type.md`](adding-a-market-type.md) | Extending rules and observations | Backend developers | Current | Backend maintainer |
| [`migration.md`](migration.md) | v1 archive and v2 cutover policy | Maintainers | Current | Project maintainer |
| [`changes.md`](changes.md) | User-facing implementation changes | Everyone | Current | Project maintainer |
| [`decisions/`](decisions/) | Accepted and proposed architectural decisions | Developers and reviewers | Current | Project maintainer |
| [`developer/`](developer/) | Code-level reference and traceability | Human developers | Current | Relevant component maintainer |
| [`developer/use-cases.md`](developer/use-cases.md) | Actors, use case register, and business rules for the platform | Reviewers and maintainers | Current; every use case is implemented | Project maintainer |
| [`diagrams/use-case.puml`](diagrams/use-case.puml) | Use case diagram source and rendered image | Reviewers | Current; matches the use case model | Project maintainer |
| [`archive/ai-agent-rewrite-progress/`](archive/ai-agent-rewrite-progress/) | Earlier AI agent planning/progress documentation | Historical reviewers | Archived; non-canonical | None |
| [`../legacy/python-backend/`](../legacy/python-backend/) | Retired Python prototype | Historical reviewers | Archived; non-canonical | None |

## Verification baseline

This documentation was prepared against the current working tree on 9 September 2026. At the time of writing, the Rust rebuild was not committed: Git `HEAD` still identified the earlier Python implementation at `ae15584`, while the active Rust files and migrations were working-tree changes. After the rebuild is committed, replace this note with the identifying commit and re-run the checks in [verification](verification.md).

## Documentation rules

1. Describe implemented behaviour as implemented, not as originally proposed.
2. Label proposals, future work, simulator behaviour, and historical material explicitly.
3. Keep one canonical document for each detailed contract; summaries should link to it instead of copying every detail.
4. Treat Rust request types, Axum routes, SQL migrations, and transaction code as implementation evidence—not as substitutes for approved product requirements.
5. Update requirements, implementation documentation, tests, and the traceability matrix together when behaviour changes.
6. Do not place tokens, passwords, private provider references, personal identifiers, or real attendee data in documentation or example evidence.
7. Record the command, environment, date, and retained artifact for performance or verification claims.

## Archived AI rewrite notes

The previous documentation pass was created by an AI agent to plan and track the options-to-outcome-market rewrite. It is preserved intact in [`archive/ai-agent-rewrite-progress/`](archive/ai-agent-rewrite-progress/) because it contains useful design history, benchmark narrative, and migration reasoning. It should not be used as the current API or operations manual.
