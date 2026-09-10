# ADR 0001: Build the active backend in Rust

Status: **accepted and implemented**  
Decision date: **8 September 2026**

## Context

Replacing option contracts with direct outcome shares changed the central pricing, execution, accounting, and settlement model. The project owner chose to build that replacement directly in Rust rather than first implementing a Python engine and later considering a C++ arithmetic kernel.

## Decision

- Keep the existing React frontend technology.
- Build one Rust application using Axum/Tokio for HTTP and background work, SQLx for PostgreSQL, and a pure Rust market-math module.
- Keep market categories inside one codebase instead of creating network services per category.
- Require Rust 1.88 and use SQLx 0.8.6 because the next SQLx generation requires a newer compiler.
- Commit `Cargo.lock` and use locked builds.
- Forbid `unsafe` code in the application crate and keep integer overflow checks enabled in release builds.
- Use Python only to generate independent high-precision numerical fixtures; production has no Python or C++ dependency.

## Consequences

Benefits include compiler-checked types and ownership, one deployable backend, and no cross-language production boundary. Costs include a full migration and a Rust learning curve.

Rust does not prove transaction correctness, numerical accuracy, or safe product rules. Those properties remain dependent on schema constraints, carefully defined invariants, tests, reconciliation, and review.

## Alternatives considered

- Modify the existing Python options backend in place.
- Implement the outcome engine in Python first and optimize arithmetic in C++ only after profiling.
- Split market categories or workers into independent services.

These were rejected for the active rebuild. The original analysis remains in the [archived AI plan](../archive/ai-agent-rewrite-progress/repository/docs/direct-outcome-markets-plan.md).

## Evidence

- `backend/Cargo.toml` records the compiler floor and dependencies.
- `backend/src/lib.rs` contains `#![forbid(unsafe_code)]`.
- `.github/workflows/verify.yml` installs Rust 1.88 and runs locked tests/clippy.
- [Verification](../verification.md) records the measured results and limitations.
