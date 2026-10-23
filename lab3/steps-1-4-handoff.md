# Lab 3 handoff - original checklist steps 1-4

Date: **23 October 2026**. Scope: design-model draft, architecture, startup class, and control responsibility allocation. Application source and behaviour are unchanged. This is an internal handoff, not an extra deliverable prescribed by the manual.

## Read this first

The class diagram is an **intermediate draft**, not the completed Lab 3 design. Finish the remaining design work before using it for the Lab 3.3 AI exercises. Keep the maintained sources under `docs/`; refresh the diagram copies under `lab3/` after changes. The original Lab 2 transcript and diagram provenance have not been recovered or reconstructed by this work.

## Scope and manual mapping

The source of the academic requirements is the five-page **Lab 3 Manual-1.pdf** supplied by the team. The scope here follows the team's original numbered checklist, rather than interpreting "steps 1-4" as all four numbered subsections of manual 3.1.

| Original checklist step | Manual requirement | Work in this contribution |
|---|---|---|
| 1. Draft design model | 3.1.1 | Refine the existing `docs/diagrams/class-diagram.puml`; retain the ten entities/associations and explain their implementation mapping. |
| 2. Determine architecture | 3.1.2 | Name and justify the layered client-server architecture with a monolithic Rust backend. |
| 3. Add startup class | First bullet of 3.1.3 | Model existing main initialization and runtime wiring as StartupControl. |
| 4. Add controls/distribute logic | Second bullet of 3.1.3 | Retain seven use-case controls and clarify ownership/collaboration against the code. |

Manual 2.3 asks for encapsulation, loose coupling, and high cohesion; the design rationale addresses these. Manual 2.5 asks for traceability and justification of technology choices; this contribution supplies use-case/source mappings and existing test pointers, not a new technology-selection exercise or full test audit. Manual 3.1.3 refers to Fox pp. 341-345; this contribution applies the startup/control bullets reproduced in the manual and does not claim an independent review of those textbook pages.

Manual 3.1.4 is dialog-map refinement and detailed class-model work, which remain outside our scope. The remaining bullets of 3.1.3 (containers/patterns), all of 3.2 and 3.3, and the 3.4 implementation audit are also left to their owners.

## Files and source of truth

| File | Role |
|---|---|
| [docs/diagrams/class-diagram.puml](../docs/diagrams/class-diagram.puml) | Maintained editable design diagram. |
| [docs/diagrams/class-diagram.png](../docs/diagrams/class-diagram.png) | Maintained rendered diagram. |
| [docs/developer/class-diagram.md](../docs/developer/class-diagram.md) | Maintained entity/boundary/control explanations, runtime mappings, and use-case coverage. |
| [docs/architecture.md](../docs/architecture.md) | Maintained detailed architecture, with style/startup/provider corrections. |
| [lab3/class-diagram.puml](class-diagram.puml) | Submission snapshot of the maintained diagram source. |
| [lab3/class-diagram.png](class-diagram.png) | Submission snapshot of its rendered image. |
| [lab3/architecture.md](architecture.md) | Concise architecture submission write-up, with links to maintained details. |
| [lab3/steps-1-4-handoff.md](steps-1-4-handoff.md) | This handoff and remaining-work boundary. |

The manual's section 4 asks for deliverables under `lab3/` in GitHub before Lab 4. These files prepare only this contribution; final packaging, commits, and upload remain with the team. References to canonical artifacts elsewhere in this repository are navigation aids, not a claim that the full submission package is already assembled.

## Step 1 - design model draft

The original ten entities remain Account, VerificationRequest, MarketSeries, MarketInstance, Outcome, Position, Trade, Evidence, SettlementClaim, and LedgerTransfer. Existing entity attributes, associations, multiplicities, and composition are preserved. The seven existing boundaries remain; ApiRouter is added for startup wiring. No new collection container classes, pattern claims, entity operations, or detailed signatures have been introduced.

The updated model contains **31 classes**: **10 entities, 8 boundaries, 8 controls, 4 infrastructure classes, and 1 active runtime abstraction**. The controls are the original seven use-case controls plus StartupControl. Operation additions/renames express coordination responsibilities; further signature/visibility refinement is left for the later design task.

MarketInstance/MarketSeries map to Rust Instance/Series. Outcome combines its published ID/label with its instance inventory entry; probability is derived. Account's domain fields are broader than its public API struct. Several other entities are persisted SQL records returned through read models/JSON, rather than separate Rust structs. React function components and browser helpers implement the user boundaries. The companion document explains these mappings so the diagram does not imply a class-per-file refactor.

## Step 2 - architecture

**Style: layered client-server architecture with a monolithic Rust backend.** React handles presentation; Axum and application/domain functions handle requests and coordination; PostgreSQL stores durable state. The Rust process also serves frontend files and runs background processing. External resolvers and the optional Python bus provider are separate integrations.

The layers are logical: Store mixes service behaviour and persistence, and some API/worker code performs SQL. Pure pricing/fee helpers and the resolver contract provide useful responsibility boundaries without pretending the implementation has a strict repository architecture.

The existing detailed architecture was corrected to include fee helpers, all three launched background tasks, and the optional bus-provider path. Only style/startup/provider-related sections received the current review; other detailed sections retain their previous review baseline. The bus adapter's provider-first/simulated-fallback behaviour is specific to eligible simulated bus instances. Generic resolver failures still follow retry/deadline voiding. This source review makes no live-provider reliability claim.

## Step 3 - startup

StartupControl models [backend/src/main.rs](../backend/src/main.rs). It reads configuration, enforces the demo bind restriction, delegates pool/migration/account/settings setup to Store::connect, configures the provider, builds EventBus/AppState, conditionally seeds demos, builds routing/static delivery, and binds the listener. It then launches the worker, notification listener, and clock refresher, serves HTTP, and aborts all three handles after normal graceful shutdown. The refresher exits immediately outside demo mode.

Runtime support consists of Store, AppState, ApiRouter, BackgroundWorker, EventBus, and EventRuntime. Store/AppState/EventBus are existing structs. ApiRouter, BackgroundWorker, EventRuntime, and StartupControl group existing functions/modules; they are not newly implemented Rust classes. EventBus is existing event plumbing, not an answer to the later domain-container exercise. Startup does not instantiate seven controller objects.

Secrets are validated by AppState::new after database initialization. Initialization errors propagate as Results; this documentation does not claim all validation happens before side effects or that the normal shutdown cleanup covers every error path.

## Step 4 - control responsibilities

| Control | Responsibility and key decision | Implementation entry points |
|---|---|---|
| AccountControl | Registration/welcome gift, authentication/session rotation, current-account read, creator-request submission/status. Administrator decisions are separate. | Store::register_account, login, account_by_id, create_verification_request, verification_request; auth helpers and API me handler. |
| MarketQueryControl | Public discovery, market/series snapshots, history, probabilities/day view. Browser category filtering stays in the boundary. | Store::instances, instance_detail, instance_history, series_view; amm calculations. |
| MarketCreationControl | Validate definitions/authority, publish series, create funded instances. `createFundedInstance()` replaces `fundOneTimeInstance()` because recurring/admin paths reuse the same capability. | Store::create_series/create_instance; market validation. |
| TradingControl | Signed quotes, execution, fees, holdings/balances, creator self-trade rejection, idempotency/receipt replay. | Store::quote/execute in execution.rs; amm/auth/fee helpers. |
| PortfolioControl | Private holdings, credits, and trade history; read-only assembly. | Store::portfolio/trades. |
| MarketLifecycleControl | Scheduled spawning/closure, evidence/resolution, suspension mutation, settlement/fee split, deadline voiding, expired-bracket cleanup. | worker::tick/process_instance; Store::spawn_due_brackets/purge_expired_brackets; resolution.rs and fee helpers. |
| AdminControl | Privileged entry-point gate, creator decisions, provisioning/grants, reconciliation, and coordination of direct creation/evidence/suspension. | AppState::require_admin and admin handlers; Store decision/provision/reconcile methods and lifecycle methods. |

New explicit logical dependencies are AdminControl -> MarketCreationControl, AdminControl -> MarketLifecycleControl, MarketLifecycleControl -> MarketCreationControl, and BackgroundWorker -> MarketLifecycleControl. The first three describe responsibility allocation currently implemented through shared Store methods, not new controller instances or calls added by this contribution.

The seven controls remain cohesive around use cases. Lifecycle is the broadest but coordinates the same market across scheduling, evidence/resolution, and settlement/cleanup. Pure calculations/verification remain helpers, and external calls remain behind ResolverGateway. Mutation ownership and transaction checks provide encapsulation; shared contracts/helpers reduce coupling. Physical separation is limited by the existing Store/API SQL mix.

Important interpretation details:

- Browser boundaries prepare/sign creator resolutions; the backend verifies the submitted authority/signature. No new backend signing-key ownership is introduced.
- `recoverReceipt()` reuses the same trade body and idempotency key through execute; no new recovery endpoint exists.
- Registration and its welcome gift are atomic. One-time series publication commits the series first, then creates/funds the instance, attempting cleanup on failure; the two stages are not one database transaction.
- Direct administrator creation/evidence, demo clock/tick utilities, and retention are existing implementation paths outside the numbered use-case register. They do not silently add new numbered requirements.
- Existing security, accounting, and retention behaviour is referenced to allocate responsibilities; the later persistence/access-control/pattern tasks are not marked complete by those references.

## Traceability and existing test pointers

The complete use-case-to-boundary/control table is in [class-diagram.md](../docs/developer/class-diagram.md#use-case-coverage), based on the existing [use-case descriptions](../docs/developer/use-cases.md) and [diagram](../docs/diagrams/use-case.puml). Those artifacts were not edited.

The following test names were found in [backend/tests/integration.rs](../backend/tests/integration.rs). These are navigation examples, not an exhaustive test matrix or evidence that the tests were run in this pass.

| Requirement/design path | Existing test examples |
|---|---|
| UC-1/2/3 -> AccountControl | `registration_creates_a_member_account_with_the_welcome_gift`; `login_rotates_the_session_token`. |
| UC-5/19/21 -> MarketQueryControl | `instance_lists_filter_by_browse_class`; `instance_history_buckets_prices_and_volume`; `series_day_view_weights_live_brackets_by_units_bet`. |
| UC-8 -> MarketCreationControl | `creators_publish_one_time_series_with_their_single_instance`. |
| UC-12 -> worker/lifecycle/creation | `recurring_series_keep_the_rolling_horizon_filled`. |
| UC-13/14/17 -> lifecycle/gateway | `resolver_authority_settles_from_the_external_endpoint`; `creators_resolve_their_markets_with_signed_statements`; `resolver_authority_voids_when_answers_stay_invalid_or_unreachable`. |
| UC-16 -> lifecycle/fee split | `trading_fees_are_charged_and_split_with_the_creator`. |
| UC-20 -> TradingControl/receipt replay | `concurrent_duplicate_requests_have_one_effect_and_survive_restart`; `creators_cannot_trade_in_their_own_markets`. |
| UC-7 -> AdminControl | `approval_grants_the_creator_role_permanently`; `rejection_records_a_reason_and_allows_reapplication`. |
| Existing retention implementation | `old_recurring_brackets_purge_but_one_time_markets_stay`. |

Full test traceability and any coverage gaps, including portfolio/admin operational paths and startup, remain for the verification owner. No backend/frontend tests or builds are required by the documentation-only edits, and none are represented here as newly passed.

## Remaining work for teammates

| Owner/work area | Remaining task and boundary |
|---|---|
| Collection/container design | Original step 5 / remaining container bullet of 3.1.3: assess outcomes, positions, and trades collections; model appropriate containers without assuming each is a database table. |
| Pattern assessment | Original steps 6/11 / 3.1.3 and 3.2.2: assess and justify appropriate patterns. Existing helper/gateway names here are not a completed pattern analysis. |
| Dialog map | Original step 7 / 3.1.4: revisit and refine the existing dialog maps. They are unchanged by this contribution. |
| Detailed class model | Original steps 8/12 / 3.1.4 and 3.2.3: refine attributes, operation signatures, visibility, value types, and relationships as needed. Retained attributes and responsibility-level operations are a starting point. |
| Persistent data and access control | Original steps 9/10 / 3.2.1: perform the schema/migration and authorization design review. Existing behaviour references above do not complete that work. |
| AI technology-stack comparison | Original steps 13/14 / 3.3.1-2: obtain the actual Lab 2 prompt/response/model details and original diagram, plus the Lab 1 description; then use the team's completed updated diagram. |
| AI skeleton, evaluation, and reporting | Original steps 15-18 / 3.3.3-5: generate the TODO-only skeleton separately, evaluate it, retain prompts/responses, prepare the short PDF, and package its distinct ZIP. |
| Implementation audit | Original steps 19-22 / 3.4: verify the existing application against the design, consider stereotype/folder mapping, audit class/key-public-method documentation, and check source availability in GitHub. No application code/refactor/audit was completed here. |
| Submission assembly | Section 4: include the complete use-case diagram/descriptions, completed class diagram and dialog map, architecture, full application skeleton/prototype with clear structure, AI PDF, and separately identifiable AI skeleton ZIP. Arrange artifacts under `lab3/` and upload before Lab 4. |

Manual clarifications for later owners:

- Reusing the same AI model/copied prompt in a fresh chat is requested **if possible** (3.3.1). If language/framework preferences changed, the manual permits prompting with the new information. Do not fabricate missing Lab 2 transcripts or claim a controlled comparison without the evidence.
- With unchanged prompt/preferences, explain whether differences between the two class diagrams could explain changed recommendations (3.3.2(1)). With changed preferences, ask the AI to compare old/new choices and whether it agrees with the change, and retain that follow-up (3.3.2(2)).
- The AI skeleton must contain only TODO comments, with no implementation code; critically assess its choices, omissions, and instruction violations. The PDF needs both exercises' prompts/responses and the required answers. Its ZIP must remain distinguishable from the real application skeleton.
- Folder organization by stereotypes is suggested by 3.4.2 rather than mandated. Document the actual Rust/React mapping and any mismatch; do not refactor working code solely because UML uses class boxes.
- The complete use-case model is explicitly a section 4 deliverable, even though it was not a separate item in the team's daily checklist.
- Sequence diagrams are explicitly **not required**. The manual allows an in-progress prototype when its overall skeleton structure is clear; implementation completion continues in Lab 4.

## Refresh and verification

Edit the canonical PlantUML first, keep its companion explanations/counts synchronized, render, inspect the image, then refresh both submission snapshots from the repository root:

```powershell
.\scripts\render-diagrams.ps1 -Diagram 'docs\diagrams\class-diagram.puml'
Copy-Item -LiteralPath 'docs\diagrams\class-diagram.puml' -Destination 'lab3\class-diagram.puml'
Copy-Item -LiteralPath 'docs\diagrams\class-diagram.png' -Destination 'lab3\class-diagram.png'
```

The renderer requires Java and a PlantUML JAR; the repository script downloads the JAR into ignored `.tools/` when absent. Update the concise architecture write-up if later design changes alter its responsibilities. Keep both diagram snapshots marked draft until the remaining design work is complete.

Verification performed on **23 October 2026**:

- Rendered the canonical diagram with PlantUML **1.2026.9** using the repository script and the local JDK; visually inspected the resulting **2832 x 2722 PNG**. Keep the image at full resolution when reviewing its class text and associations.
- Confirmed all **31** class definitions, stereotype counts, and dependency endpoints; confirmed the seven original use-case controls plus StartupControl.
- Compared the entity section with the pre-change source: all ten entity definitions and their associations/multiplicities are unchanged. Presentation-only line breaks separate the parallel ledger-transfer role labels and multiplicities in the rendering.
- Confirmed submission PUML/PNG snapshots are byte-identical to their canonical counterparts.
- Checked relative link targets, referenced Markdown anchors, and code-fence pairing across the four written/updated Markdown documents; resolved all **16** existing test-name pointers without executing the tests.
- Reviewed the scoped diff and whitespace; application source/configuration is unchanged. The two pre-existing modified files (`docs/developer/README.md` and `legacy/README.md`) retain their original content hashes and were left untouched. An existing whitespace issue in the latter is outside this change.
- The PolyNTU application, migrations, builds, backend/frontend test suites, and live-provider checks were not run; no commits or pushes were made. These documentation checks are not new functional-test evidence.
