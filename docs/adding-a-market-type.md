# Adding a market type

Status: **current Rust extension guide**

Market types define questions and translate normalized observations into outcomes. They must not implement pricing, quote signing, authentication, ledger movements, ownership checks, transaction retries, or settlement credits.

## Decide whether a new type is necessary

Reuse an existing rule when the new question differs only by identifiers, threshold, title, source, or time window. Add a new `Rule`/`Observation` variant only when validation, outcome structure, or evidence evaluation genuinely differs.

Queue length, crowd occupancy, and unique attendance reuse `Rule::Count` with distinct `Metric` values. A new template does not require new Rust code.

## Implementation checklist

1. **Define the immutable rule.** Add a `Rule` variant in `backend/src/market.rs`. Use bounded identifiers, explicit units, and only fields that are known when the instance opens.
2. **Map the category.** Extend `Rule::category`. If the category key is new, add a numbered migration updating the `templates.category` constraint and update the frontend label map.
3. **Validate the rule.** Reject empty/oversized identifiers, invalid values, ambiguous outcome sets, or unsafe real-person use. Validation must run before funding or publication.
4. **Derive outcomes.** Extend `Rule::outcomes`. Binary rules should reuse `yes`/`no`; categorical rules need 2–8 distinct, exhaustive labels with stable IDs.
5. **Define normalized evidence.** Add the matching `Observation` variant with `deny_unknown_fields` behaviour. Include completeness/finality explicitly; never infer a negative result from missing coverage.
6. **Evaluate evidence.** Extend `Rule::evaluate`. Confirm identifiers and units match the published rule. Return `Ok(None)` for incomplete/non-final evidence, `Winner` for a supported final result, and `Void` only for a prepublished cancellation/no-winner policy.
7. **Specify timing.** Set close, half-open observation interval, earliest finalization, and evidence deadline in `NewInstance`. Trading must close no later than observation start.
8. **Add demo behaviour if needed.** Extend `Rule::simulated` and `demo_specs`. Simulation must be deterministic from the private per-database seed/instance identity and generated only after the window.
9. **Design the real adapter boundary.** A future adapter runs outside quote/execution, retains the provider record, normalizes to `EvidenceInput`, uses monotonic revisions, and never places secrets or personal data in the publicly returned payload.
10. **Update the frontend.** Add a category label only for a new category key. Existing outcome rendering and trading should work without category-specific execution code.
11. **Test rule boundaries.** Cover exact threshold, just below/above, wrong identifiers, negative values, missing coverage, interval endpoints, excessive arrays, cancellation, and invalid categorical labels.
12. **Test the shared lifecycle.** Add an integration case proving quote, buy/sell, evidence, finalization/void, claim, and reconciliation work without category-specific ledger changes.
13. **Update documentation.** Revise the evidence ADR, API rule/observation examples, overview category table, glossary if needed, traceability matrix, and change record.

## Evidence design rules

- Use absolute timestamps and exact, published units.
- Treat `[start, end)` consistently: the start counts and the end does not.
- Preserve source identity and revision history.
- Separate completeness from a zero/empty measurement.
- Keep raw credentials and personally identifying data outside `EvidenceInput`.
- Make cancellation/void policy explicit before opening.
- Never allow provider estimates or simulator truth to reset AMM inventory or prices.

## Current worked patterns

| Pattern | Code example | Important edge case |
|---|---|---|
| Numeric threshold | Weather | Missing readings are not zero. |
| Event-in-window | Bus | Arrival at start counts; arrival at end does not. No requires complete coverage. |
| Small categorical | Fictional election | Candidate labels must be distinct and final null winner voids. |
| Aggregate threshold | Count metrics | Queue, occupancy, and unique attendance are different measurements. |

## Attendance privacy

The current attendance contract accepts only an already deduplicated aggregate. A future check-in adapter must deduplicate before submission and must not put attendee identities in observations, references, logs, public instance responses, or developer fixtures.

## Completion gate

A new type is complete only when:

- its rule and evidence contract are unambiguous;
- database and frontend category constraints agree;
- category-specific unit tests and shared integration tests pass;
- reconciliation passes after settlement;
- no pricing/accounting code branches on the new category; and
- current documentation describes its source assumptions and failure policy.

The archived Python extension checklist describes option transforms and Greeks and does not apply to the Rust application.
