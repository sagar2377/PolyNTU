# ADR 0003: Use immutable rules and evidence-based resolution

Status: **accepted and implemented for simulated and authenticated manual evidence**

## Context

Market prices can only be settled safely when every instance publishes an objective question, observation interval, data source, correction period, deadline, and missing-data policy before trading begins.

## Decision

Each instance fixes:

- an absolute trading close;
- a half-open observation interval `[start, end)`;
- an earliest finalization time;
- an evidence deadline;
- one source ID and data mode;
- one typed rule and derived outcome set; and
- a uniform fractional void policy.

Times are UTC Unix milliseconds and the frontend displays them in Asia/Singapore time. Trading closes no later than the observation start. Every execution rechecks authoritative database time after acquiring locks.

## Category contracts

| Category | Required rule and observation semantics |
|---|---|
| Weather | Fixed station and rainfall threshold; complete nonnegative total for the exact window. Missing data never means zero rainfall. |
| Bus | Fixed route, direction, and stop; actual arrival timestamps and complete interval coverage. The start counts and the end does not. An ETA is not arrival evidence. |
| Fictional election | Two to eight distinct fictional candidates and a final sole winner. A final null winner applies the published tie/runoff/cancellation void policy. |
| Queue/crowd | Fixed queue-length or occupancy metric, location, threshold, and complete nonnegative count. |
| Attendance | Fixed unique-attendance metric, event/lecture identifier, threshold, and complete deduplicated aggregate count. |

## Evidence records

Evidence is append-only and scoped to the instance's fixed source. Each input includes a bounded event ID, nonnegative source revision, exact window, typed observation, and human-readable reference. The stored record also adds receive time, parser version, payload hash, and evaluated result.

- Repeating an event ID with identical content returns the existing evidence ID.
- Reusing it with different content is rejected.
- Reusing a source revision is rejected.
- An older unused revision may arrive after a newer one, but cannot replace it; the instance points to the highest revision.
- Incomplete/non-final evidence produces no result and waits.
- Evidence after finalization is rejected and the attempted event hash is audited.

The public instance response exposes the selected normalized evidence payload. Evidence references and observations must therefore contain no secret credentials, personal identifiers, private URLs, raw attendance records, or other sensitive data.

## Finalization and voiding

After `finalize_after_ms`, the highest revision with an evaluated result can be fixed. If no complete result exists at `evidence_deadline_ms`, the instance is voided.

A void redeems the account's total shares at `1/n` units per share, independent of trade price. Credits are aggregated by account and rounded down to a microcredit. This is fractional redemption, not reversal or refund of historical trades.

Once fixed, the result is immutable. Settlement uses a unique instance/account claim and batches at most 100 accounts per transaction. When every claim is present, unused reserve is returned and the instance becomes terminal.

## Demonstration evidence

Demo mode persists a private random secret. The worker hashes that secret with the instance ID and generates a deterministic observation only after the observation window. This permits repeatable results across restarts without exposing future outcomes through public IDs. Simulator evidence is clearly labelled and is not a live forecast or provider feed.

## External feeds

Live weather, shuttle, crowd, and attendance adapters are deferred. Real integrations will need permission, stable identifiers, coverage checks, corrections, and source-quality monitoring. For current development, assume an authorized adapter can obtain and normalize the required observation. Generated observations must never be substituted silently for missing live data.

## Authorization and audit scope

Manual evidence and resolution-affecting administrator endpoints require the shared administrator token. Instance creation, evidence receipt/rejection, suspension, finalization, simulated evidence, and demo clock changes create administrator audit records. Administrator account provisioning creates a ledger grant but does not currently add an `admin_audit` row; therefore the system must not be described as auditing every administrator request.

Separate administrator and institutional resolver roles, multi-party evidence approval, and provider verification remain future work.

## Evidence

- `backend/src/market.rs`
- `backend/src/resolution.rs`
- `backend/src/worker.rs`
- `backend/migrations/0001_outcome_markets.sql`
- `backend/migrations/0002_immutable_rules.sql`
- [Evidence and settlement guide](../developer/evidence-and-settlement.md)
