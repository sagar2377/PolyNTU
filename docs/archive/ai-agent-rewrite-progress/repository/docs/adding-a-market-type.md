Adding a market type to the Rust backend.

Market categories provide rules and evidence evaluation. They never implement balance movements, pricing, ownership checks, quote signing or settlement credits.

1. Add a typed variant to `Rule` in `backend/src/market.rs`, with fixed domain identifiers and units. Define its category, validation and mutually exclusive outcomes. Binary questions should use the shared Yes/No outcomes. Categorical questions must contain 2–8 distinct, exhaustive outcomes.
2. Add the matching `Observation` variant and implement `Rule::evaluate`. Return a winning outcome only when the source and values match the published rule and coverage is sufficient. Return `None` for incomplete/non-final evidence. Use `Resolution::Void` only for the prepublished cancellation/no-winner rule.
3. Define exact half-open observation boundaries, source identity, correction/finalization grace and evidence deadline in a `NewInstance`. Trading must close before the observation window. All fields become immutable on creation.
4. Add deterministic demo observations through `Rule::simulated`, and add a demo specification if the category should appear in development. Simulation must depend on persisted instance identity, not read order or request RNG. Never expose future demo evidence from a quote endpoint.
5. A real adapter should run outside the quote/execution path, preserve immutable source records, normalize units/windows, deduplicate deliveries and submit monotonically increasing source revisions through the evidence service. A new category does not justify resetting market prices from a provider estimate.
6. Test threshold boundaries, wrong identifiers, missing coverage, negative/invalid values, out-of-order revisions, cancellation and the common settlement lifecycle. Add an integration case demonstrating that ledger and execution code need no category-specific changes.
7. If adding a new category key, update the database category constraint with a new migration and the frontend's category label map. Update `docs/decisions/0003-evidence-resolution.md` and the change record with the data-source assumptions.

For unique attendance, the current source contract requires an already-deduplicated aggregate count. A future raw check-in adapter must enforce uniqueness itself before producing an observation. Merely labeling registrations or repeated scans as attendance would violate the contract.

The archived Python market-type interface required option transforms and Greeks. Those methods have no role in the Rust interface; the old checklist is preserved only in the legacy archive.
