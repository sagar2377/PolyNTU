# Evidence and settlement internals

Evidence is intentionally separate from pricing and trading. A provider observation can resolve a market after its window, but it cannot modify LMSR inventory, rewrite trades, or inject a provider probability.

## Published timing contract

Each instance fixes five times:

```text
close_ms <= observation_start_ms < observation_end_ms
observation_end_ms <= finalize_after_ms < evidence_deadline_ms
```

- Trading requires database time strictly before `close_ms`.
- Evidence requires the observation window to have ended.
- Manual evidence requires time strictly before the deadline.
- A complete result may be fixed at/after `finalize_after_ms`.
- Missing/incomplete evidence voids at/after the deadline.

The observation interval is half-open `[start,end)`. This matters explicitly for bus arrivals.

## Rules and observations

| Rule | Matching observation | `None`/wait condition | Final result |
|---|---|---|---|
| Weather station/threshold | Same station, nonnegative rainfall total, `complete` | `complete=false` | Yes when total `>=` threshold, otherwise No |
| Bus route/direction/stop | Same identifiers, at most 10,000 arrival timestamps, `complete` | `complete=false` | Yes if any timestamp is `>=start && <end`, otherwise No |
| Fictional candidates | `is_fictional=true`, optional winner, `is_final` | `is_final=false` | Named candidate wins; final null winner voids |
| Count metric/location/threshold | Same metric/location, nonnegative value, `complete` | `complete=false` | Yes when value `>=` threshold, otherwise No |

A mismatched observation is invalid; it is never converted to a losing result.

## Evidence input and fingerprint

`EvidenceInput` contains source ID, event ID, revision, exact window, typed observation, and reference. The recorder serializes the complete input and hashes those bytes with SHA-256.

Because the hash covers the full JSON representation accepted by Serde, changing the reference or any observation/envelope field changes the event content. Clients should send a stable serialization model and use a new event ID/revision for corrections.

## Ingestion sequence

`record_evidence` performs:

1. event/revision/reference/payload-size validation;
2. transaction start and instance `FOR UPDATE` lock;
3. authoritative time read;
4. exact event-ID duplicate lookup;
5. terminal-state late rejection with committed audit record;
6. manual/simulator mode authorization;
7. observation-end and manual-deadline checks;
8. exact source/window match;
9. typed rule evaluation;
10. source-revision uniqueness check;
11. immutable evidence insertion;
12. instance pointer update to highest revision and one version increment;
13. audit and outbox insertion; and
14. commit.

An identical event duplicate commits no new evidence, instance version, audit, or outbox row and returns the existing ID.

## Revision behaviour

Revision numbers need not arrive in increasing network order. Revision 2 may be stored before revision 1; when revision 1 later arrives, the instance still points to revision 2. Reusing revision 2 for another event conflicts.

The highest revision controls finalization even when it retracts completeness. For example, a newer `complete:false` correction causes the market to wait or eventually void rather than falling back to an older complete result.

## Public-data rule

The selected normalized payload is returned by the public instance-detail endpoint and rendered inside a frontend disclosure. Therefore:

- references must be safe for public display;
- observations must contain only aggregate/non-sensitive values;
- tokens, provider secrets, raw access URLs, personal identifiers, and individual check-ins must remain outside the payload;
- future adapters should retain raw/private source material in a separate controlled system and submit only an immutable public reference; and
- logs should identify event/evidence IDs rather than dump bodies.

The backend schema does not redact the reference. Documentation and adapter review are currently the primary guardrails.

## Closing

`close_due` selects at most 100 persisted `open` rows whose `close_ms <= now`, orders them by close/ID, and locks with `FOR UPDATE SKIP LOCKED`. Each becomes `closed`, advances one version, and emits a closure event.

Public instance views independently report `closed` when cutoff has passed, even before this persisted transition. Trade execution checks time directly, so worker delay never extends trading.

## Actionable worker selection

After closing and series bracket spawning, `worker::tick` selects rows that can make progress:

- every `resolving` instance with unfinished claims;
- due simulated instances missing evidence;
- closed instances past finalization with an evaluated result; or
- closed instances past the evidence deadline.

Closed rows merely waiting for evidence are excluded. This prevents a large older set from occupying the 100-row worker batch and starving ready results.

## Demo evidence

The singleton settings row stores a private random simulation secret. For a due instance, the worker computes a hash over `<secret>:<instance_id>` and passes it to `Rule::simulated`.

The resulting event:

- uses source `polyntu-simulator-v1`;
- has revision 0 and stable event ID `simulated:<instance>`;
- records the logical receive time as the observation end;
- is generated only after the window; and
- is clearly referenced as deterministic demo evidence without a live source.

The audit/outbox time is the current authoritative time at processing. Public instance IDs alone cannot reveal the future generated result.

## Result fixation

When a closed instance reaches `finalize_after_ms`, `settle_batch` loads the highest source revision.

- Evaluated winner/void present: persist it as `result` and move to `resolving`.
- No evaluated result and deadline reached: create the standard missing-evidence void.
- No result and deadline not reached: commit no changes and return zero.

Fixation advances the version and appends `resolving` event plus audit. The database trigger makes the result immutable.

## Claim calculation

The batch selects up to 100 distinct accounts with positive positions and no existing claim.

### Winning result

Only the winning outcome quantity counts:

```text
credit_micros = winning_quantity_millis * 1000
```

### Void result

All positive outcome quantities are aggregated for the account:

```text
credit_micros = floor(total_quantity_millis * 1000 / outcome_count)
```

For 1.001 shares (`1,001` millishares) in a three-outcome void:

```text
floor(1001 * 1000 / 3) = 333,666 microcredits
```

The residual remains in the reserve and returns to treasury after completion.

## Claim transaction and completion

The batch locks selected user accounts, reserve, and treasury in sorted ID order. For each account it checks reserve balance, transfers the credit with unique reference `claim:<instance>:<account>`, and inserts the primary-key claim.

If any operation fails, the entire batch rolls back. A later tick selects the same accounts again. After a committed batch, those claims are excluded, so restart continues with the remainder.

When no unclaimed positive-position account remains, the transaction transfers the entire unused reserve to treasury with reference `release:<instance>` and marks the instance `resolved` or `voided`.

## Suspension

Suspension is an independent boolean available only while persisted state is open. It stops new quotes/trades without altering close or evidence times. Every change requires a 5–1,000 character reason, advances the version, and appends audit/event records.

Suspension is not cancellation. Unless an administrator supplies valid evidence or the market reaches its missing-evidence deadline, normal resolution rules still apply.

## External-provider assumption

The implementation currently assumes a future trusted adapter can obtain the required observations. Provider-specific contracts—including candidate sources such as NEA and OmniBus—are outside the current build. The main integration challenges are authorization, stable source identifiers, complete window coverage, later corrections, and distinguishing estimated from actual events. These do not change the internal evidence contract; adapters must normalize into it and preserve the published missing-data behaviour.

## Failure modes

| Failure | Behaviour |
|---|---|
| Worker stops | Trading cutoffs remain enforced; closing/settlement resume after restart. |
| Duplicate delivery | Exact event returns existing ID; changed content conflicts. |
| Out-of-order revision | Stored but cannot replace higher selected revision. |
| Incomplete evidence | Waits until correction or deadline void. |
| Evidence after finalization | Audit attempt, reject conflict, never change result. |
| Failure inside claim batch | Full transaction rollback; safe retry. |
| Failure after a committed batch | Existing claims exclude credited accounts on restart. |
| Reserve invariant failure | Internal error and rollback; reconcile/incident investigation required. |

## Change checklist

Changes to evidence or settlement require review of `market.rs`, `resolution.rs`, worker selection, schema constraints/triggers, public payload privacy, API examples, reserve reconciliation, missing-data policy, and integration tests for revision order and recovery.
