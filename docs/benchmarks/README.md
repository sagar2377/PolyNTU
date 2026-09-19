# Retained benchmark artifacts

These JSON files are machine-readable outputs from the local verification performed on 8 September 2026. The maintained interpretation, environment, measurement boundaries, and limitations are in the [verification record](../verification.md).

| Artifact | Meaning |
|---|---|
| [`2026-09-09-http.json`](2026-09-09-http.json) | Clean ten-minute HTTP/SSE baseline |
| [`2026-09-09-settlement.json`](2026-09-09-settlement.json) | Settlement workload with concurrent quotes, trades, and SSE clients |
| [`2026-09-09-build-contention.json`](2026-09-09-build-contention.json) | Non-baseline run affected by concurrent compilation and skipped client work |

The short throughput KPI workload (`scripts/benchmark-kpi.mjs`) writes its report under `.local/` instead, because it is a regression gate rather than a retained measurement; see the [verification record](../verification.md#throughput-kpi-benchmark).

Keep the raw artifacts immutable. Add new dated files for later measurements and update `docs/verification.md` with the exact environment and interpretation.
