# Architectural decision records

Architectural decision records explain important choices, the alternatives considered, and the consequences maintainers inherit. An accepted record describes the current design decision; it does not override the running code, database constraints, or an approved later record.

| ADR | Status | Decision |
|---|---|---|
| [0001](0001-rust-backend.md) | Accepted and implemented | Build the active backend as one Rust application |
| [0002](0002-lmsr-accounting.md) | Accepted and implemented | Use LMSR outcome shares with micro-unit accounting and reserve-backed issuance |
| [0003](0003-evidence-resolution.md) | Accepted and implemented | Resolve immutable market definitions from revisioned evidence, then settle in resumable batches |
| [0004](0004-trade-fees.md) | Accepted and implemented | Charge a 25 bps per-trade fee and split the settled pot with the market creator |

Add a new numbered record when a change materially alters system boundaries, trust assumptions, accounting, pricing, resolution, persistence, or operational guarantees. Do not rewrite an accepted historical decision to hide a later change; supersede it with a new record and link both ways.
