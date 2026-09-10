// Real HTTP + PostgreSQL workload. Run only against a dedicated benchmark database.
import { randomUUID } from 'node:crypto';
import { writeFile } from 'node:fs/promises';
import { performance } from 'node:perf_hooks';
const base = process.env.BENCH_URL || 'http://127.0.0.1:18000';
const admin = process.env.POLYNTU_ADMIN_TOKEN;
if (!admin) throw new Error('POLYNTU_ADMIN_TOKEN is required');
const seconds = Number(process.env.BENCH_SECONDS || 600);
const reportPath = process.env.BENCH_REPORT || '.local/performance.json';
const run = randomUUID().slice(0, 8);
async function call(path, body, token, key, isAdmin = false) {
  const headers = { 'Content-Type': 'application/json' };
  if (token) headers.Authorization = `Bearer ${token}`;
  if (key) headers['Idempotency-Key'] = key;
  if (isAdmin) headers['X-Admin-Token'] = admin;
  const start = performance.now();
  const response = await fetch(`${base}/api/v2${path}`, { method: body === undefined ? 'GET' : 'POST', headers, body: body === undefined ? undefined : JSON.stringify(body) });
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
  await Promise.all(Array.from({ length: count }, async () => { while (index < items.length) { const i = index++; await action(items[i], i); } }));
}
const accounts = [], markets = [];
const config = await required('/config');
const now = config.server_time_ms;
console.log('Preparing 100 markets, 200 accounts, and 10000 positions…');
await parallel(Array.from({ length: 200 }), 5, async (_, i) => {
  accounts[i] = await required('/admin/accounts', { display_name: `Benchmark ${run} ${i}` }, null, null, true);
});
await parallel(Array.from({ length: 100 }), 5, async (_, i) => {
  markets[i] = await required('/admin/instances', {
    template_id: `benchmark-${run}-${i}`, title: `Benchmark rainfall ${i}`,
    resolution_criterion: 'Yes when the benchmark station records at least 0.2 mm in the fixed observation window. Missing complete evidence voids the market.',
    rule: { kind: 'weather', station_id: 'benchmark', threshold_milli_mm: 200 },
    source_id: 'benchmark-observer', data_mode: 'manual', close_ms: now + 7200000,
    observation_start_ms: now + 7200000, observation_end_ms: now + 10800000,
    finalize_after_ms: now + 10860000, evidence_deadline_ms: now + 14400000, liquidity_units: 100,
  }, null, null, true);
});
const holdings = accounts.map(() => Array.from({ length: 100 }, (_, i) => i < 50 ? 1000 : 0));
await parallel(markets.slice(0, 50), 10, async (market) => {
  for (const account of accounts) {
    const q = await required('/quotes', { instance_id: market.id, outcome_id: 'yes', side: 'buy', quantity_millis: 1000 }, account.token);
    await required('/trades', { quote_token: q.quote_token, limit_micros: q.amount_micros }, account.token, randomUUID());
  }
});
const before = await required('/admin/reconcile', undefined, null, null, true);
if (!before.ok) throw new Error(`Initial reconciliation failed: ${JSON.stringify(before)}`);
const controller = new AbortController();
let sseConnected = 0;
const streams = accounts.map(async (_, i) => {
  try {
    const response = await fetch(`${base}/api/v2/instances/${markets[i % 100].id}/events`, { signal: controller.signal });
    if (!response.ok) throw new Error(`SSE ${response.status}`);
    sseConnected++;
    for await (const _ of response.body) { /* Consume updates so connections exercise the real stream. */ }
  } catch (e) { if (!controller.signal.aborted) throw e; }
});
// Allow initial event replay to finish before measuring steady traffic.
await new Promise((resolve) => setTimeout(resolve, 3000));
if (sseConnected !== 200) throw new Error(`Only ${sseConnected}/200 SSE clients connected`);
const phases = ['spread', 'hot_80_percent'];
const stats = Object.fromEntries(phases.map((phase) => [phase, { quote: [], trade: [], conflicts: 0, errors: 0, skipped: 0, committed: 0, submittedQuotes: 0, submittedTrades: 0 }]));
const inFlight = new Set();
let tradeIndex = 0, quoteIndex = 0;
const start = performance.now();
const phaseNow = () => performance.now() - start < seconds * 500 ? phases[0] : phases[1];
const marketIndex = (index, phase) => phase === phases[1] && index % 5 !== 0 ? 0 : index % 100;
function schedule(task, phase) {
  if (inFlight.size >= 200) { stats[phase].skipped++; return; }
  const promise = task().catch(() => { stats[phase].errors++; }).finally(() => inFlight.delete(promise));
  inFlight.add(promise);
}
const quoteTimer = setInterval(() => {
  while (quoteIndex < Math.floor((performance.now() - start) / 10)) {
  const phase = phaseNow(), index = quoteIndex++, market = markets[marketIndex(index, phase)], account = accounts[index % 200];
  stats[phase].submittedQuotes++;
  schedule(async () => {
    const result = await call('/quotes', { instance_id: market.id, outcome_id: 'yes', side: 'buy', quantity_millis: 1000 }, account.token);
    if (result.status === 200) stats[phase].quote.push(result.ms); else stats[phase].errors++;
  }, phase);
  }
}, 5);
const tradeTimer = setInterval(() => {
  while (tradeIndex < Math.floor((performance.now() - start) / 50)) {
  const phase = phaseNow(), index = tradeIndex++, user = index % 200, m = marketIndex(index, phase), account = accounts[user];
  stats[phase].submittedTrades++;
  schedule(async () => {
    const side = holdings[user][m] >= 1000 ? 'sell' : 'buy';
    const q = await call('/quotes', { instance_id: markets[m].id, outcome_id: 'yes', side, quantity_millis: 1000 }, account.token);
    if (q.status !== 200) { stats[phase].errors++; return; }
    const result = await call('/trades', { quote_token: q.value.quote_token, limit_micros: q.value.amount_micros }, account.token, randomUUID());
    if (result.status === 200) {
      stats[phase].trade.push(result.ms); stats[phase].committed++;
      holdings[user][m] += side === 'buy' ? 1000 : -1000;
    } else if (result.status === 409) stats[phase].conflicts++; else stats[phase].errors++;
  }, phase);
  }
}, 5);
const progress = setInterval(() => console.log(`Benchmark ${Math.round((performance.now() - start) / 1000)}s/${seconds}s; committed ${Object.values(stats).reduce((n, x) => n + x.committed, 0)} trades; ${inFlight.size} in flight; errors ${Object.values(stats).reduce((n, x) => n + x.errors, 0)}; skipped ${Object.values(stats).reduce((n, x) => n + x.skipped, 0)}`), 30000);
await new Promise((resolve) => setTimeout(resolve, seconds * 1000));
clearInterval(quoteTimer); clearInterval(tradeTimer); clearInterval(progress);
await Promise.allSettled([...inFlight]);
controller.abort(); await Promise.allSettled(streams);
const elapsed = (performance.now() - start) / 1000;
const after = await required('/admin/reconcile', undefined, null, null, true);
function summary(values) {
  const sorted = [...values].sort((a, b) => a - b);
  const percentile = (p) => sorted.length ? Number(sorted[Math.min(sorted.length - 1, Math.floor(sorted.length * p))].toFixed(2)) : null;
  return { samples: sorted.length, p50_ms: percentile(.5), p95_ms: percentile(.95), p99_ms: percentile(.99) };
}
const report = { timestamp: new Date().toISOString(), duration_seconds: elapsed, client: process.version, markets: 100, accounts: 200, seeded_positions: 10000, sse_clients: sseConnected,
  measurement: 'HTTP round-trip including response parsing; development machine, local PostgreSQL; elapsed-time arrival scheduling targets 100 quote/s and 20 trade workflows/s',
  phases: Object.fromEntries(Object.entries(stats).map(([phase, s]) => [phase, { ...s, quote: summary(s.quote), trade: summary(s.trade) }])), reconciliation: after };
await writeFile(reportPath, JSON.stringify(report, null, 2) + '\n');
console.log(JSON.stringify(report, null, 2));
if (!after.ok || Object.values(stats).some((s) => s.errors > 0 || s.skipped > 0)) process.exitCode = 1;
