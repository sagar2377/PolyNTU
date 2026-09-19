// Short closed-loop throughput benchmark used as a performance KPI and as a
// CI regression gate. Measures maximum sustainable quote and trade capacity,
// not compliance with a target arrival rate. Run only against a dedicated
// benchmark database. Exits non-zero when a KPI gate regresses.
import { randomUUID } from 'node:crypto';
import { mkdir, writeFile } from 'node:fs/promises';
import { dirname } from 'node:path';
import { performance } from 'node:perf_hooks';

const base = process.env.BENCH_URL || 'http://127.0.0.1:18000';
const admin = process.env.POLYNTU_ADMIN_TOKEN;
if (!admin) throw new Error('POLYNTU_ADMIN_TOKEN is required');
// Per-phase measurement duration; capped so a stray BENCH_SECONDS cannot turn
// the KPI run into a long soak.
const phaseSeconds = Math.min(Number(process.env.BENCH_SECONDS || 15), 120);
const reportPath = process.env.BENCH_REPORT || '.local/kpi.json';
const gates = {
  minQuoteRps: Number(process.env.KPI_MIN_QUOTE_RPS || 100),
  minTradeRps: Number(process.env.KPI_MIN_TRADE_RPS || 15),
  maxQuoteP99Ms: Number(process.env.KPI_MAX_QUOTE_P99_MS || 250),
  maxTradeP99Ms: Number(process.env.KPI_MAX_TRADE_P99_MS || 500),
};
const quoteConcurrency = Number(process.env.KPI_QUOTE_CONCURRENCY || 16);
const tradeConcurrency = Number(process.env.KPI_TRADE_CONCURRENCY || 8);
const marketCount = 20;
const accountCount = Math.max(tradeConcurrency, quoteConcurrency);
const run = randomUUID().slice(0, 8);

async function call(path, body, token, key, isAdmin = false) {
  const headers = { 'Content-Type': 'application/json' };
  if (token) headers.Authorization = `Bearer ${token}`;
  if (key) headers['Idempotency-Key'] = key;
  if (isAdmin) headers['X-Admin-Token'] = admin;
  const start = performance.now();
  const response = await fetch(`${base}/api/v2${path}`, {
    method: body === undefined ? 'GET' : 'POST',
    headers,
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const value = await response.json();
  return { status: response.status, value, ms: performance.now() - start };
}
async function required(...args) {
  const r = await call(...args);
  if (r.status !== 200) throw new Error(`Setup failed: ${r.status} ${JSON.stringify(r.value)}`);
  return r.value;
}
async function parallel(items, count, action) {
  let index = 0;
  await Promise.all(Array.from({ length: count }, async () => {
    while (index < items.length) { const i = index++; await action(items[i], i); }
  }));
}
function summary(values) {
  const sorted = [...values].sort((a, b) => a - b);
  const percentile = (p) => sorted.length
    ? Number(sorted[Math.min(sorted.length - 1, Math.floor(sorted.length * p))].toFixed(2))
    : null;
  return { samples: sorted.length, p50_ms: percentile(.5), p95_ms: percentile(.95), p99_ms: percentile(.99) };
}
// Closed-loop phase: `count` workers each iterate as fast as requests complete.
// The action receives the worker's fixed index and resolves to 'ok',
// 'conflict' (an expected 409), or 'error', plus the request latency to
// record for successes.
async function phase(seconds, count, action) {
  const latencies = [];
  let ok = 0, conflicts = 0, errors = 0;
  const deadline = performance.now() + seconds * 1000;
  await Promise.all(Array.from({ length: count }, async (_, worker) => {
    while (performance.now() < deadline) {
      try {
        const { outcome, ms } = await action(worker);
        if (outcome === 'ok') { ok++; latencies.push(ms); }
        else if (outcome === 'conflict') { conflicts++; }
        else { errors++; }
      } catch { errors++; }
    }
  }));
  return {
    rps: Number((ok / seconds).toFixed(1)),
    conflicts,
    errors,
    latency: summary(latencies),
  };
}

console.log(`KPI benchmark against ${base}: ${marketCount} markets, ${accountCount} accounts, ${phaseSeconds}s per phase`);
const config = await required('/config');
const now = config.server_time_ms;
const accounts = [], markets = [];
console.log('Preparing accounts and markets…');
await parallel(Array.from({ length: accountCount }), 5, async (_, i) => {
  accounts[i] = await required('/admin/accounts', { display_name: `KPI ${run} ${i}` }, null, null, true);
});
await parallel(Array.from({ length: marketCount }), 5, async (_, i) => {
  markets[i] = await required('/admin/instances', {
    template_id: `kpi-${run}-${i}`, title: `KPI rainfall ${i}`,
    resolution_criterion: 'Yes when the benchmark station records at least 0.2 mm in the fixed observation window. Missing complete evidence voids the market.',
    rule: { kind: 'weather', station_id: 'kpi-benchmark', threshold_milli_mm: 200 },
    source_id: 'kpi-observer', data_mode: 'manual', close_ms: now + 7200000,
    observation_start_ms: now + 7200000, observation_end_ms: now + 10800000,
    finalize_after_ms: now + 10860000, evidence_deadline_ms: now + 14400000, liquidity_units: 100,
  }, null, null, true);
});
const before = await required('/admin/reconcile', undefined, null, null, true);
if (!before.ok) throw new Error(`Initial reconciliation failed: ${JSON.stringify(before)}`);

// A few SSE consumers exercise the event fan-out path during both phases.
const controller = new AbortController();
let sseConnected = 0;
const streams = accounts.slice(0, 20).map(async (_, i) => {
  try {
    const response = await fetch(`${base}/api/v2/instances/${markets[i % marketCount].id}/events`, { signal: controller.signal });
    if (!response.ok) throw new Error(`SSE ${response.status}`);
    sseConnected++;
    for await (const _ of response.body) { /* Drain the stream. */ }
  } catch (e) { if (!controller.signal.aborted) throw e; }
});
await new Promise((resolve) => setTimeout(resolve, 2000));

console.log(`Phase 1: closed-loop quotes at concurrency ${quoteConcurrency}…`);
const quotes = await phase(phaseSeconds, quoteConcurrency, async () => {
  const result = await call('/quotes', {
    instance_id: markets[Math.floor(Math.random() * marketCount)].id,
    outcome_id: 'yes', side: 'buy', quantity_millis: 1000,
  }, accounts[0].token);
  return { outcome: result.status === 200 ? 'ok' : 'error', ms: result.ms };
});

console.log(`Phase 2: closed-loop trade workflows at concurrency ${tradeConcurrency}…`);
const holdings = new Map();
const trades = await phase(phaseSeconds, tradeConcurrency, async (worker) => {
  // One fixed worker per (account, market) pair: each market has a single
  // sequential trader, so the phase measures capacity, not version conflicts.
  const market = markets[worker % marketCount];
  const account = accounts[worker];
  const key = `${account.id}:${market.id}`;
  const owned = holdings.get(key) || 0;
  const side = owned >= 1000 ? 'sell' : 'buy';
  const q = await call('/quotes', { instance_id: market.id, outcome_id: 'yes', side, quantity_millis: 1000 }, account.token);
  if (q.status !== 200) return { outcome: 'error', ms: 0 };
  const trade = await call('/trades', { quote_token: q.value.quote_token, limit_micros: q.value.amount_micros }, account.token, randomUUID());
  if (trade.status === 200) {
    holdings.set(key, owned + (side === 'buy' ? 1000 : -1000));
    return { outcome: 'ok', ms: trade.ms };
  }
  return { outcome: trade.status === 409 ? 'conflict' : 'error', ms: 0 };
});
controller.abort();
await Promise.allSettled(streams);
const after = await required('/admin/reconcile', undefined, null, null, true);

const failures = [];
if (quotes.rps < gates.minQuoteRps) failures.push(`quote throughput ${quotes.rps}/s < ${gates.minQuoteRps}/s`);
if (quotes.latency.p99_ms !== null && quotes.latency.p99_ms > gates.maxQuoteP99Ms) failures.push(`quote p99 ${quotes.latency.p99_ms}ms > ${gates.maxQuoteP99Ms}ms`);
if (trades.rps < gates.minTradeRps) failures.push(`trade throughput ${trades.rps}/s < ${gates.minTradeRps}/s`);
if (trades.latency.p99_ms !== null && trades.latency.p99_ms > gates.maxTradeP99Ms) failures.push(`trade p99 ${trades.latency.p99_ms}ms > ${gates.maxTradeP99Ms}ms`);
if (quotes.errors > 0) failures.push(`${quotes.errors} quote errors`);
if (trades.errors > 0) failures.push(`${trades.errors} trade errors`);
if (!after.ok) failures.push(`reconciliation failed: ${JSON.stringify(after)}`);

const report = {
  timestamp: new Date().toISOString(),
  measurement: 'Closed-loop maximum capacity: every worker iterates as fast as its requests complete. Quote latency covers the quote endpoint; trade latency covers the trade endpoint only, excluding its preview quote.',
  environment: { url: base, node: process.version, phase_seconds: phaseSeconds, quote_concurrency: quoteConcurrency, trade_concurrency: tradeConcurrency, markets: marketCount, accounts: accountCount, sse_clients: sseConnected },
  gates,
  quotes,
  trades,
  reconciliation: after,
  passed: failures.length === 0,
  failures,
};
await mkdir(dirname(reportPath), { recursive: true });
await writeFile(reportPath, JSON.stringify(report, null, 2) + '\n');
console.log(JSON.stringify(report, null, 2));
if (failures.length > 0) {
  console.error(`KPI regression: ${failures.join('; ')}`);
  process.exitCode = 1;
} else {
  console.log(`KPI passed: ${quotes.rps} quotes/s (p99 ${quotes.latency.p99_ms}ms), ${trades.rps} trades/s (p99 ${trades.latency.p99_ms}ms)`);
}
