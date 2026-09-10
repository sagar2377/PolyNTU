# Previous AI rewrite documentation

Status: **archived progress material**  
Preserved: 9 September 2026

This folder preserves the documentation produced by the AI development agent while it planned, implemented, tested, and measured the Rust outcome-market rewrite. The files are retained intact so maintainers can reconstruct the agent's reasoning, intermediate claims, migration notes, and reported verification results.

These files are not the canonical documentation for the current application. Some statements were progress-oriented, several documents lacked navigable Markdown structure, and a small number of descriptions did not exactly match the final working tree. Use the active [documentation index](../../README.md) for current guidance.

The most useful historical items are:

- `repository/docs/direct-outcome-markets-plan.md`: the original mechanism and migration proposal. Its Python-first path was superseded by the user's decision to build directly in Rust.
- `repository/docs/changes.md`: the AI agent's implementation progress record.
- `repository/docs/verification.md`: the original narrative accompanying the retained benchmark reports.
- `repository/docs/architecture.md`, `api.md`, and `development.md`: the first documentation pass over the Rust rebuild.
- `repository/docs/decisions/`: the first versions of the architectural decision records.

The `repository/` directory mirrors the previous documentation paths so its internal relative links remain meaningful. The archived Python prototype has its own documentation under `legacy/python-backend/`; that code is historical product source, not part of this AI-progress snapshot.
