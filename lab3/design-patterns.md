# Design patterns - Lab 3

Pattern names follow the standard GoF catalogue, with Repository and Unit of Work taken from the enterprise-patterns literature.

## Pattern catalogue

### Strategy

- **Problem and forces:** three resolution authorities (administrator, creator signature, external resolver) accept evidence under different rules, yet all must feed one shared lifecycle - the same instance states, evidence recording, audit, outbox events, and settlement. Authority is fixed at publication and must not be bypassable, even by an administrator with full API access.
- **Participants:** `ResolutionAuthority` (interface) implemented by `CreatorSignedAuthority` and `ExternalResolverAuthority`; `MarketLifecycleControl` selects by the series' authority and delegates. `ExternalResolverAuthority` also depends on `ResolverGateway` for its outbound call.
- **Implementation anchor:** the `resolution_authority` column (`creator`, `resolver`) in [store.rs](../backend/src/store.rs) `create_series`; `record_creator_resolution` and `record_resolver_evidence` in [resolution.rs](../backend/src/resolution.rs), each re-checking its own authority and refusing the other. The two implementations are entered from two places: the creator resolution route in [api.rs](../backend/src/api.rs), and the automatic resolver call in [worker.rs](../backend/src/worker.rs), which gates on the `resolver` value.
- **Consequences and trade-offs:** the lifecycle machinery stays in one place and a wrong-authority submission is rejected inside the method, not by the caller. The cost is duplication: each method repeats the lock, dedupe, record, version, audit, and outbox steps as inlined statements.
- **Adding a variant:** a third authority needs a CHECK-constraint value, a `ResolutionSpec` variant with validation, a `create_series` mapping arm, a new ingestion method, and a route or worker branch.

### Adapter

- **Problem and forces:** the settlement worker speaks only the fixed external-resolver contract (`{"outcome_id": ...}` or `{"pending": true}`), but the NTU bus timing source is not a platform outcome: the live Omnibus feed is watched by the external Python provider, and the fallback is a deterministic `Rule::Bus` observation that names no outcome id until evaluated.
- **Participants:** `NtuBusAdapter` (`<<boundary>>`) serves the contract that `ResolverGateway` and `MarketLifecycleControl` already target; the provider and the simulated rule evaluation are the adapted sources.
- **Implementation anchor:** `ntu_bus_answer` in [resolver.rs](../backend/src/resolver.rs), served at `POST /api/v2/resolvers/ntu-bus` by the handler in [api.rs](../backend/src/api.rs). It answers pending before the observation window ends or for ineligible instances, retries the provider three times, and on fallback derives an observation (`rule.simulated`), evaluates it, and converts the winning index into a published outcome id.
- **Consequences and trade-offs:** bus series resolve through the same contract as any other series, so settlement has no bus-specific branch, and a future data source is adopted by pointing a series at a new endpoint. The costs are an extra loopback HTTP hop in demo mode and a fallback policy that applies only to eligible simulated bus instances - generic resolver failures retry to the deadline and void instead of falling back.
- **Adding a variant:** a new data source means a new endpoint plus conversion code outside the core; no settlement or evidence code changes, which is the point of the contract.

### Observer

- **Problem and forces:** browsers must learn of market changes (trades, closure, evidence, resolution, suspension) without polling, while the durable outbox must stay authoritative so a lost notification delays an event rather than dropping it.
- **Participants:** `EventBus` and `EventRuntime` (`<<infrastructure>>`) form the subject side; the SSE streams opened by the market pages are the observers, each an `EventSource` created in [MarketPage.jsx](../frontend/src/pages/MarketPage.jsx). The instance cache is a second, internal observer of the same notifications.
- **Implementation anchor:** `EventBus::subscribe`/`publish` in [events.rs](../backend/src/events.rs) - one broadcast channel per instance, created on first subscription and removed when a later publish finds no receivers left; `run_listener` fans PostgreSQL notifications out; `store::event` writes the outbox row and notification in one statement; the SSE handler in [api.rs](../backend/src/api.rs) replays from the outbox, de-duplicates by cursor, and re-reads on a 30-second catch-up pass.
- **Consequences and trade-offs:** observers attach and detach per instance with no registration lists in domain code, and outbox replay makes delivery delay-tolerant across processes. A lagged receiver is dropped by the broadcast channel and must fall back to the outbox pass, and trade events carry inventory so the cache can update without a read - a coupling between one observer's needs and the event payload.
- **Adding a variant:** a new transport (webhooks, for example) needs fan-out changes in the listener, because `publish` only knows broadcast channels; a new subscriber kind that can read the outbox or notifications needs none.

### Factory Method

- **Problem and forces:** a recurring series must produce a correctly timed, funded instance for every grid slot - same rule, source, liquidity, fee choice, and creator, but its own windows, reserve account, and inventory - while one-time publication and direct administrator creation need the same funded creation with different timing.
- **Participants:** `MarketSeries.bracketSpec()` builds one slot's definition; `MarketCreationControl.createFundedInstance()` is the shared creation operation, used by one-time publication in `create_series`, `MarketLifecycleControl.spawnDueBrackets()`, `AdminControl.createDirectInstance()`, and demo seeding.
- **Implementation anchor:** `Series::bracket_spec` in [market.rs](../backend/src/market.rs) and `NewSeries::instance_spec` with `Schedule::bracket_timing` (stored windows for `Once`, computed windows for `Recurring`); `Store::create_instance` in [store.rs](../backend/src/store.rs) validates, funds from the treasury, and inserts; `Store::spawn_series_brackets` drives the rolling grid.
- **Consequences and trade-offs:** what varies per slot is isolated in the spec builders, and the funding, validation, and audit invariants live once in `create_instance` for all callers. There is no polymorphic factory hierarchy; the claim covers spec construction plus shared creation, which is what the source supports.
- **Adding a variant:** a new instance shape means editing the shared `NewInstance` struct and its builders; a new schedule kind means a new `Schedule` variant with a `bracket_timing` arm and validation - extension by modification, since the enum is closed.

## Patterns considered and rejected

| Pattern | Reason |
|---|---|
| State | Lifecycle transitions (`open -> closed -> resolving -> resolved/voided`), version advance, finality, and the closed-market inventory freeze are enforced by the `protect_instance` trigger in [0005_trade_fees.sql](../backend/migrations/0005_trade_fees.sql); `Instance::tradable` is a predicate over stored state, suspension, and time. An in-code state machine would duplicate the database constraint without adding a transition. |
| Singleton | `Store`, `AppState`, and `EventBus` derive `Clone` over an Arc pool, Arc secrets, and an Arc channel map; `main` constructs and passes them explicitly, and the worker clones `Store` per settlement task. The one `LazyLock` static in [store.rs](../backend/src/store.rs) holds a fixed login-timing hash, not a shared service, and documented multi-process operation rules out a process-wide instance. |
| Repository | `Store` mixes service rules with SQL in the same methods - `record_creator_resolution` validates, verifies a signature, audits, and queries - and [worker.rs](../backend/src/worker.rs) and [api.rs](../backend/src/api.rs) embed SQL too. The shared helpers (`transfer`, `audit`, `event`, `balance`) are functions called inside service transactions, not a persistence layer. |
| Unit of Work | Every service method opens and commits its own transaction inline; no object accumulates changes for one commit, and the outbox write shares the transaction instead. One-time publication is deliberately two transactions: `create_series` commits the series before funding the instance, with cleanup on failure. |
| Facade | `Store` is one struct holding a pool with service methods, not an interface hiding subsystem objects; no subsystem classes sit behind it. The "logical facade" wording for `MarketLifecycleControl` in the class responsibilities describes grouping of design responsibilities, not the GoF structure. |
| Builder | `NewInstance`, `NewSeries`, and `ResolverRequest` are plain data built in single expressions by `bracket_spec`, `instance_spec`, `demo_specs`, and `ResolverRequest::of`. No step-wise assembly, director, or alternative representation exists to justify the pattern. |
| Template Method | The two authority paths repeat the same skeleton (lock, dedupe, insert, version, audit, outbox, commit) as inlined statements in separate methods, with no inherited algorithm and no hook methods; Rust here offers no override hierarchy to hang it on. The variation is carried by the strategy above. |

## Patterns in the diagram

The strategy participants are the only pattern boxes: the `<<strategy>>` package in the [diagram source](../docs/diagrams/class-diagram.puml) shows the `ResolutionAuthority` interface with its two implementations, the `MarketLifecycleControl ..> ResolutionAuthority` delegation arrow, and `ExternalResolverAuthority ..> ResolverGateway`. The adapter and observer appear as ordinary stereotypes - `NtuBusAdapter` as a `<<boundary>>` behind `ResolverGateway`, `EventBus`/`EventRuntime` as `<<infrastructure>>` - because their participants are runtime structures rather than interchangeable behaviours; stereotyping them as patterns would suggest a code refactor that is not proposed. The factory-method roles are operations on existing classes (`MarketSeries.bracketSpec()`, `MarketCreationControl.createFundedInstance()`), so no new box was introduced for them either. This keeps the diagram's class counts unchanged from the maintained source.

## Pattern-to-design-class map

| Pattern | Diagram stereotypes and classes | Source anchor |
|---|---|---|
| Strategy | `<<strategy>>` `ResolutionAuthority` with `CreatorSignedAuthority`, `ExternalResolverAuthority`; `MarketLifecycleControl` delegates, `ExternalResolverAuthority` uses `ResolverGateway` | `resolution_authority` column; `record_creator_resolution` and `record_resolver_evidence` in [resolution.rs](../backend/src/resolution.rs) |
| Adapter | `<<boundary>>` `NtuBusAdapter`; `ResolverGateway` calls it over the resolver contract | `ntu_bus_answer` in [resolver.rs](../backend/src/resolver.rs); handler in [api.rs](../backend/src/api.rs) |
| Observer | `<<infrastructure>>` `EventBus`, `EventRuntime`; SSE streams opened by `MarketPage` | `EventBus::subscribe`/`publish` in [events.rs](../backend/src/events.rs); SSE handler in [api.rs](../backend/src/api.rs) |
| Factory Method | `<<entity>>` `MarketSeries.bracketSpec()`; `MarketCreationControl.createFundedInstance()`; `MarketLifecycleControl` funds bracket creation through it | `Series::bracket_spec` in [market.rs](../backend/src/market.rs); `Store::create_instance` in [store.rs](../backend/src/store.rs) |

## Consequences summary

| Pattern | Benefit in this system | Cost or limitation |
|---|---|---|
| Strategy | One lifecycle for both authorities; authority fixed at publication and enforced inside each method | Shared steps duplicated per method; no trait or enum the compiler checks when an authority is added |
| Adapter | Any data source adopted by configuration; no bus branch in settlement | Extra loopback hop; a fallback policy specific to eligible simulated bus instances |
| Observer | No polling; durable outbox keeps delivery delay-tolerant across processes | New transports need fan-out changes; lagged receivers fall back to outbox polling |
| Factory Method | Per-slot variation isolated; funding and validation invariants shared by all creators | Extension of the instance shape or schedule kind is by modification, not addition |

## References

- [Class responsibilities and pattern assessment](../docs/developer/class-diagram.md) - maintained detail: the diagram legend, container classes, and the pattern table.
- [Design class diagram source](../docs/diagrams/class-diagram.puml) - the `<<strategy>>` package and the collaboration arrows.
- [Architecture write-up](architecture.md) - the layered style and runtime tasks these patterns sit in.
