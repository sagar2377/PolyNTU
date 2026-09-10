Decision 0003 — Immutable rules and evidence-based resolution.

Status: implemented with simulated and authenticated manual evidence adapters.

Every instance publishes an absolute close time, a half-open observation window `[start,end)`, earliest finalization, evidence deadline, fixed source, and typed rule. Times are stored as UTC Unix milliseconds and displayed in Singapore time. Trading closes before observations begin. A request waiting on a lock must still pass the cutoff after obtaining that lock.

| Category | Rule and observation contract |
|---|---|
| Weather | Named station, rainfall threshold in thousandths of a millimetre, total over the window and explicit completeness. Missing readings are never treated as zero. |
| Bus | Exact route, direction and stop; actual arrival timestamps. Arrival at the start counts; arrival at the end does not. No arrival means No only with complete coverage. ETA is not resolution evidence. |
| Elections | Fixed distinct fictional candidates and a final sole winner. Non-final results wait. Ties, runoff, withdrawal with no sole winner, or cancellation use the published void policy. The existing fictional-only constraint remains. |
| Queue/crowd | Separate queue-length or occupancy metric, fixed location, threshold and trusted end-of-window count. Waiting time is a different metric. |
| Attendance | Fixed event/lecture identifier, threshold and source-provided unique attendance count. The source must deduplicate check-ins; the backend receives aggregates and does not store attendee identities. Registrations are not attendance. |

Evidence is source-scoped and append-only. Each record contains event ID, monotonic source revision, exact observation window, source reference, parser version, payload hash and evaluated result. A duplicate event with the same content succeeds without another record; changed content under the same event ID is rejected. Corrections require a higher revision. Out-of-order deliveries cannot replace a newer revision. Finalization uses the highest revision, including when that revision retracts completeness.

Incomplete evidence waits until the deadline, then voids. A void credits every share at `1/n` units regardless of entry price. Aggregate per-account credit rounds down to a microcredit. This is a published fractional redemption policy, not reversal of historical trades. Once a result is fixed, late submissions cannot change it; attempts after finalization receive an audit record and a conflict response.

The demo worker creates observations only after their window ends. A forward jump in the simulated clock replays due demo observations at their prescribed logical time, then settles. Simulation combines the instance ID with a private random database seed and does not run when users request prices. Public responses expose neither the seed nor future observations. Clock offset, seed and instances persist through restarts.

Manual ingestion requires the administrator credential. The current deployment has one combined administrator/resolver credential, with all actions audited; separate institutional roles and evidence approvals can be added later. Validation checks source identity and payload shape, but it cannot independently verify a human-submitted observation. Live provider access and source-quality validation remain necessary before presenting a market as live.

Candidate future adapters: [NEA rainfall](https://data.gov.sg/datasets/d_6580738cdd7db79374ed3152159fbd69/view); permitted actual-arrival telemetry for [NTU shuttles](https://www.ntu.edu.sg/about-us/visiting-ntu/internal-campus-shuttle); institution-provided crowd and attendance aggregates. No live-feed access was assumed or configured in this rebuild.
