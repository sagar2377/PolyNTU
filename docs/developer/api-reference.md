# API implementation map

This document maps external endpoints to Axum handlers and service code. The stable client-facing shapes are documented in [the HTTP API](../api.md).

## Router layers

`api::router` builds:

1. a nested `/api/v2` router;
2. an API fallback returning `Error::NotFound`;
3. `/health` outside the version prefix;
4. three explicitly retired option routes;
5. a 65,536-byte default body limit;
6. configured CORS allowing GET/POST and content-type, authorization, idempotency, and administrator headers;
7. Tower HTTP request tracing; and
8. shared `AppState`.

`main.rs` adds static `ServeDir` as the outer fallback after constructing this router.

## Handler map

| Method/path | Handler | Authentication | Service/query |
|---|---|---|---|
| `GET /health` | `health` | None | `SELECT 1` |
| `GET /api/v2/config` | `config` | None | `Store::now` plus constants |
| `POST /api/v2/auth/demo` | `demo_account` | Demo mode | `Store::create_account` |
| `POST /api/v2/auth/register` | `register` | None | `Store::register_account` |
| `POST /api/v2/auth/login` | `login` | None | `Store::login` |
| `POST /api/v2/admin/accounts` | `admin_account` | `require_admin` | `Store::create_account` |
| `GET /api/v2/me` | `me` | `AppState::account` | `Store::account_by_id` for a fresh balance |
| `GET /api/v2/me/portfolio` | `portfolio` | Account | `Store::portfolio` |
| `GET /api/v2/me/trades` | `trades` | Account | `Store::trades` |
| `GET /api/v2/markets` | `markets` | None | `Store::templates` |
| `GET /api/v2/markets/{id}/instances` | `template_instances` | None | `Store::instances(Some(id))` |
| `POST /api/v2/series` | `create_series` | Creator role | `Store::create_series` |
| `GET /api/v2/series` | `list_series` | None | `Store::series_list` |
| `GET /api/v2/series/{id}` | `series_detail` | None | `Store::series_view` |
| `GET /api/v2/instances` | `instances` | None | `Store::instances(None)` |
| `GET /api/v2/instances/{id}` | `instance` | None | `Store::instance_detail` |
| `GET /api/v2/instances/{id}/history` | `instance_history` | None | `Store::instance_history` |
| `GET /api/v2/instances/{id}/events` | `events` | None | outbox polling stream |
| `POST /api/v2/instances/{id}/resolution` | `creator_resolution` | Series creator | `Store::record_creator_resolution` |
| `POST /api/v2/quotes` | `quote` | Account | `Store::quote` |
| `POST /api/v2/trades` | `trade` | Account + header | `Store::execute` |
| `POST /api/v2/verification-requests` | `request_verification` | Account | `Store::create_verification_request` |
| `GET /api/v2/verification-requests` | `my_verification_request` | Account | `Store::verification_request` |
| `GET /api/v2/admin/verification-requests` | `verification_list` | Administrator | `Store::verification_requests` |
| `POST /api/v2/admin/verification-requests/{id}/decision` | `verification_decision` | Administrator | `Store::decide_verification_request` |
| `POST /api/v2/admin/instances` | `create_instance` | Administrator | `Store::create_instance` + `instance_view` |
| `POST /api/v2/admin/instances/{id}/evidence` | `evidence` | Administrator | `Store::ingest_evidence` |
| `POST /api/v2/admin/instances/{id}/suspension` | `suspend` | Administrator | `Store::suspend` |
| `POST /api/v2/admin/clock/advance` | `advance` | Administrator | `Store::advance_demo_clock`, then `worker::tick` |
| `POST /api/v2/admin/worker/tick` | `tick` | Administrator | `worker::tick` |
| `GET /api/v2/admin/reconcile` | `reconcile` | Administrator | `Store::reconcile` |
| retired paths | `retired` | None | fixed 410 response |

## Request extraction

Axum owns path/query/header/body extraction. JSON structs with `deny_unknown_fields` include account input, quote/trade, new instance, evidence, suspension, and clock advance. `Page` and event cursor do not deny unknown query parameters.

Headers are parsed as visible ASCII strings. Missing/malformed bearer headers map to unauthorized; missing/malformed administrator headers map to forbidden. The bearer scheme is case-sensitive and requires the exact prefix `Bearer `.

## Authentication flow

`AppState::account` extracts the bearer token and calls `Store::auth_account_for_token`, which:

- rejects tokens longer than 200 characters;
- hashes the token with SHA-256;
- resolves a `kind='user'` account through the read-through token cache; and
- returns unauthorized if none exists.

`require_admin` accepts either the configured shared `x-admin-token` (compared with fixed-message HMAC tags) or the bearer session of an account holding the admin role. Administrator actions are not attributed to an individual admin identity in audit rows.

## Business-error flow

Handlers return `Result<Json<T>>` or the SSE equivalent. `Error::IntoResponse` maps caller errors to the JSON envelope. Database/internal errors are logged with details and return one generic 500 message.

Axum JSON/body-limit rejections occur before the handler and may not use the application envelope. Frontend code therefore falls back to response text.

## Public read models

Instances are not serialized directly from the database struct. `instance_view` calculates current display probabilities and `tradable`, substitutes an effective `closed` state after cutoff, includes human void policy, and hides reserve account/inventory internals except outcome outstanding quantities.

`instance_detail` adds the selected evidence source/event/time and normalized payload. It does not expose all revisions, parser version, payload hash, administrator audit, ledger entries, or reserve balance.

## SSE implementation

The handler first proves the instance exists. Cursor priority is:

1. parse `Last-Event-ID` header;
2. otherwise use `?after=`;
3. otherwise zero;
4. clamp negative values to zero.

Every loop fetches up to 100 rows where global outbox ID is greater than cursor, ordered ascending. Each event uses the row ID as SSE ID and JSON with instance/version/type/time. The loop sleeps one second after each fetch, and keep-alives are configured for 15 seconds.

A database error logs and ends the stream. The handler has no per-client database transaction or dedicated connection while sleeping.

## API compatibility rules

- `/api/v2` is the current namespace.
- `ENGINE_VERSION` protects signed quote compatibility independently from HTTP API version.
- Adding optional response fields is generally compatible; changing meanings, units, IDs, or required request fields requires API review.
- Retired routes exist only to make the options cutover explicit; do not add new logic to them.
- The frontend relies on current error statuses, storage retry semantics, and integer-string amounts.

## Missing APIs by design

There is no endpoint for drafts, editing published definitions, deleting markets, retrieving all ledger/audit/evidence rows, account recovery, live provider setup, outbox administration, or general database repair. Session tokens rotate on every login; password recovery remains absent by design.

## Endpoint change checklist

For a new or changed endpoint:

1. define a bounded typed request/response;
2. choose account/admin/public authorization explicitly;
3. keep state mutation in a service transaction rather than the handler;
4. add route and error-path integration tests;
5. update `docs/api.md` and this map;
6. update frontend retry/storage assumptions when applicable; and
7. update traceability and changes.
