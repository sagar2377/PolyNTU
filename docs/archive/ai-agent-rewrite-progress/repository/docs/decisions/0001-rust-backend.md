Decision 0001 — Rebuild the active backend in Rust.

Status: accepted by the user on 8 September 2026.

Removing options replaces the central pricing and contract model. Build the replacement once in Rust instead of building a Python trading engine followed by a C++ optimization. The frontend remains React. Use Axum/Tokio for HTTP and asynchronous work, SQLx for PostgreSQL, and a pure Rust library for market math. Keep one application rather than introducing network services for individual categories.

Rust provides compiler-checked ownership and concurrency support; it does not prove transaction correctness, settlement rules, or numerical accuracy. Those need explicit invariants and tests. `unsafe` code is forbidden in the application crate. Dependency internals are outside that guarantee. Release builds retain integer overflow checks.

The current toolchain minimum is Rust 1.88. SQLx 0.8.6 is used because the newer 0.9 release requires a newer compiler. `Cargo.lock` records the resolved dependencies. No C++ code or Python extension is required. Python is used only to generate independent high-precision test fixtures; the committed fixtures allow Rust tests without Python.

The costs are a Rust learning curve and a full backend migration. We accept those costs because the user explicitly chose Rust and the market mechanism already needed replacement. Performance claims must come from measured end-to-end workloads; see `docs/verification.md`.

References: [Rust ownership](https://doc.rust-lang.org/book/ch04-01-what-is-ownership.html), [Axum](https://docs.rs/axum/latest/axum/), [SQLx 0.8.6](https://docs.rs/sqlx/0.8.6/sqlx/).
