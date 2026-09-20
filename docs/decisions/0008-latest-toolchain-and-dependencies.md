# ADR 0008: Track the latest stable Rust toolchain and latest registry dependencies

Status: **accepted and implemented**; supersedes the toolchain and SQLx pins of [ADR 0001](0001-rust-backend.md)  
Decision date: **20 September 2026**

## Context

[ADR 0001](0001-rust-backend.md) required Rust 1.88 and used SQLx 0.8.6 because the next SQLx generation needed a newer compiler. The 20 September dependency upgrade moved the backend to the latest registry versions, SQLx 0.9 among them, and SQLx 0.9 requires at least Rust 1.94. The CI workflow kept installing the pinned 1.88.0, so every run that built the backend failed on the toolchain version rather than on the code. The retained verification session did not execute the workflow on this host, so the failures went unnoticed there.

## Decision

- CI installs the latest stable Rust toolchain in every job that builds Rust, instead of a pinned version.
- `backend/Cargo.toml` declares `rust-version = "1.94"`, so a too-old local toolchain fails with a clear cargo message instead of a compiler error inside a dependency.
- Dependencies track the latest registry versions resolved by the committed `Cargo.lock`; no dependency is held back to preserve an old compiler floor.
- Locked builds stay, as in ADR 0001.

## Consequences

A current toolchain is now a prerequisite: the README and [development prerequisites](../development.md) state Rust 1.94 or newer. CI builds with the toolchain the code is actually maintained on, and a future minimum-version raise in a dependency surfaces as a build failure instead of being masked by a stale pin. Tracking the registry means occasional compile-time churn when dependencies move, traded against silent version drift.

Everything else in ADR 0001 stands: one Rust application with Axum/Tokio, SQLx for PostgreSQL, a pure market-math module, no `unsafe` code, and integer overflow checks in release builds.

## Alternatives considered

- Pin a newer fixed toolchain version: rejected; it re-creates the same failure mode where the pin goes stale the next time a dependency raises its floor.
- Stay on SQLx 0.8.6 to keep Rust 1.88: rejected; the project tracks the latest registry versions rather than holding dependencies back for an old compiler.

## Evidence

- `backend/Cargo.toml` declares `rust-version = "1.94"` and `sqlx = "0.9"`.
- `.github/workflows/verify.yml` runs `rustup toolchain install stable` in every job that builds Rust.
- [Development](../development.md) describes the CI job set; [changes](../changes.md) records the fix and the previously failing runs.
