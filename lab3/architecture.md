# PolyNTU system architecture - Lab 3

The maintained detailed explanation is [docs/architecture.md](../docs/architecture.md). The [class diagram](class-diagram.png), [editable source](class-diagram.puml), [class responsibilities](../docs/developer/class-diagram.md), and [handoff](steps-1-4-handoff.md) provide the matching design view.

![Functional modules](architecture-overview.png)

![Runtime architecture](architecture.png)

## Architectural style

PolyNTU uses a **layered client-server architecture with a monolithic Rust backend**. A React-based frontend communicates with an Axum HTTP API and receives public Server-Sent Events (SSE). PostgreSQL stores durable state. The Rust executable serves the built frontend and runs the lifecycle worker and event tasks in the same process. Configured external resolvers and the optional Python NTU bus provider are separate integration endpoints.

## Responsibility allocation

| Logical responsibility | Implementation | Design model |
|---|---|---|
| Presentation | [React source](../frontend/src) and its API helper | AccountPanel, MarketBrowsePage, CreateMarketPage, MarketPage, PortfolioPage. |
| Inbound HTTP | [api.rs](../backend/src/api.rs) | ApiRouter and AdminApi; browser-to-control arrows summarize HTTP paths. |
| Application coordination | [store.rs](../backend/src/store.rs), [execution.rs](../backend/src/execution.rs), [resolution.rs](../backend/src/resolution.rs), API handlers, and [worker.rs](../backend/src/worker.rs) | Seven logical use-case controls. |
| Domain rules and helpers | [market.rs](../backend/src/market.rs), [amm.rs](../backend/src/amm.rs), [fee.rs](../backend/src/fee.rs), [auth.rs](../backend/src/auth.rs) | Rule/value types and calculations used by controls. |
| Persistence | Store, SQLx, and PostgreSQL | Shared Store infrastructure and persisted entities. |
| Startup and task plumbing | [main.rs](../backend/src/main.rs), worker functions, and [events.rs](../backend/src/events.rs) | StartupControl, BackgroundWorker, EventRuntime, EventBus, and AppState. |
| External integration | [resolver.rs](../backend/src/resolver.rs) and [provider documentation](../provider/ntubus/README.md) | ResolverGateway; external providers remain outside the core process. |

These are logical layers. Store currently contains both service behaviour and SQL access; API/worker code also contains some SQL. The diagram allocates responsibilities without claiming a strict repository layer or seven separately instantiated controller structs.

## Rationale and design principles

- **Deployment and coordination:** one backend process keeps the academic application straightforward to run and allows request/background paths to share database coordination. Background tasks are concurrent activities within the monolith.
- **Existing technology choices:** Rust/Axum and Tokio handle concurrent requests and tasks; React provides reusable interactive components; PostgreSQL supplies the transactions, locks, and constraints relied on by trading and settlement. This justifies the existing stack without conducting the separate AI technology-stack exercise.
- **Cohesion:** each use-case control groups related coordination. MarketLifecycleControl groups scheduling, evidence/resolution, and settlement/cleanup around the same lifecycle; StartupControl handles process initialization separately.
- **Coupling:** pure pricing/fee functions avoid database and HTTP dependencies. ResolverGateway confines the external resolver contract; the provider acquires provider-specific data outside core accounting.
- **Encapsulation:** browser input checks support interaction; authoritative trading, state, evidence, and accounting decisions belong to backend transaction paths and database constraints. The current Store/API mixture remains an implementation limitation, rather than a claimed layer refactor.

## Startup class and runtime

`StartupControl <<control>>` models the free `main` function. Its single static `main()` operation summarizes existing behaviour:

1. Configure tracing; read environment configuration; parse the bind address and reject a public demo-mode bind.
2. Call Store::connect: create the pool, run embedded migrations, initialize system accounts/settings, and set the cached clock offset.
3. Apply the optional bus-provider override; construct EventBus and AppState, validate distinct secrets, and share the bus.
4. Seed demo markets when enabled; build CORS/API/static routing and bind the listener.
5. Spawn the lifecycle worker, PostgreSQL event listener, and clock refresher. The refresher returns immediately outside demo mode.
6. Serve HTTP with Ctrl+C graceful shutdown. After a normal return, abort all three task handles.

Initialization errors propagate through Result values. Some validation occurs after database initialization, and the normal shutdown description does not imply comprehensive error-path cleanup. Startup wiring uses existing functions and structs; it creates no new controller objects.

## Related design artifacts

The class diagram covers the class model, containers, patterns, dialog maps and detailed operation signatures. The persistence and access-control design are in [persistent data](persistent-data.md) and [access control](access-control.md), and the pattern assessment in [design patterns](design-patterns.md). See the [design notes](steps-1-4-handoff.md) for the manual mapping and refresh instructions.
