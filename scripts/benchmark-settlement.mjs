// Isolated HTTP settlement workload with active trading on separate instances.
import { randomUUID } from 'node:crypto';
import { writeFile } from 'node:fs/promises';
import { performance } from 'node:perf_hooks';
const base = process.env.BENCH_URL || 'http://127.0.0.1:18000';
const admin = process.env.POLYNTU_ADMIN_TOKEN;
if (!admin) throw new Error('POLYNTU_ADMIN_TOKEN is required');
const run = randomUUID();
async function call(path, body, token, key, isAdmin = false) {
  const headers = { 'Content-Type': 'application/json' };
  if (token) headers.Authorization = `Bearer ${token}`;
  if (key) headers['Idempotency-Key'] = key;
  if (isAdmin) headers['X-Admin-Token'] = admin;
  const start = performance.now();
  const response = await fetch(`${base}/api/v2${path}`, { method: body === undefined ? 'GET' : 'POST', headers, body: body === undefined ? undefined : JSON.stringify(body) });
  return { status: response.status, value: await response.json(), ms: performance.now() - start };
}
async function required(...args) {
  const r = await call(...args);
  if (r.status !== 200) throw new Error(`HTTP ${r.status}: ${JSON.stringify(r.value)}`);
  return r.value;
}
async function parallel(items, count, action) {
  let index = 0;
  await Promise.all(Array.from({ length: count }, async () => { while (index < items.length) { const i = index++; await action(items[i], i); } }));
}
const now = (await required('/config')).server_time_ms;
const close = now + 90000, end = close + 1000, final = end + 1000;
const accounts = [], markets = [], trading = [];
console.log('Preparing 10000 settlement claims across 50 markets and 200 accounts…');
await parallel(Array.from({ length: 200 }), 5, async (_, i) => { accounts[i] = await required('/admin/accounts', { display_name: `Settlement ${i}` }, null, null, true); });
await parallel(Array.from({ length: 52 }), 5, async (_, i) => {
  const c = i < 50 ? close : now + 3600000;
  const market = await required('/admin/instances', {
    template_id: `settlement-${run}-${i}`, title: `Settlement benchmark ${i}`,
    resolution_criterion: 'Yes if the station records at least 0.2 mm during the fixed window. Complete final evidence is required; otherwise void at the deadline.',
    rule: { kind: 'weather', station_id: 'benchmark', threshold_milli_mm: 200 },
    source_id: 'benchmark-observer', data_mode: 'manual', close_ms: c,
    observation_start_ms: c, observation_end_ms: c + 1000, finalize_after_ms: c + 2000,
    evidence_deadline_ms: c + 120000, liquidity_units: 100,
  }, null, null, true);
  if (i < 50) markets.push(market); else trading.push(market);
});
await parallel(markets, 10, async (market) => {
  for (const account of accounts) {
    const q = await required('/quotes', { instance_id: market.id, outcome_id: 'yes', side: 'buy', quantity_millis: 1000 }, account.token);
    await required('/trades', { quote_token: q.quote_token, limit_micros: q.amount_micros }, account.token, randomUUID());
  }
});
const initial = await required('/admin/reconcile', undefined, null, null, true);
if (!initial.ok) throw new Error('Initial reconciliation failed');
if ((await required('/config')).server_time_ms >= close) throw new Error('Setup exceeded the 90-second opening window');
const controller = new AbortController();
let connected = 0;
const streams = accounts.map(async (_, i) => {
  try {
    const response = await fetch(`${base}/api/v2/instances/${trading[i % 2].id}/events`, { signal: controller.signal });
    if (!response.ok) throw new Error(`SSE ${response.status}`);
    connected++;
    for await (const _ of response.body) { /* consume */ }
  } catch (e) { if (!controller.signal.aborted) throw e; }
});
const stats = { quote: [], trade: [], conflicts: 0, errors: 0, submittedQuotes: 0, submittedTrades: 0 };
const inFlight = new Set(), holdings = accounts.map(() => [0, 0]);
let quoteIndex = 0, tradeIndex = 0, measuring = false;
const loadStart = performance.now();
function schedule(task) { const p = task().catch(() => { stats.errors++; }).finally(() => inFlight.delete(p)); inFlight.add(p); }
const quoteTimer = setInterval(() => { while (quoteIndex < Math.floor((performance.now() - loadStart) / 10)) schedule(async () => {
  const i = quoteIndex++, record = measuring;
  if (record) stats.submittedQuotes++;
  const r = await call('/quotes', { instance_id: trading[i % 2].id, outcome_id: 'yes', side: 'buy', quantity_millis: 1000 }, accounts[i % 200].token);
  if (r.status !== 200) stats.errors++; else if (record) stats.quote.push(r.ms);
}); }, 5);
const tradeTimer = setInterval(() => { while (tradeIndex < Math.floor((performance.now() - loadStart) / 50)) schedule(async () => {
  const i = tradeIndex++, user = i % 200, m = i % 2, record = measuring;
  if (record) stats.submittedTrades++;
  const side = holdings[user][m] ? 'sell' : 'buy';
  const q = await required('/quotes', { instance_id: trading[m].id, outcome_id: 'yes', side, quantity_millis: 1000 }, accounts[user].token);
  const r = await call('/trades', { quote_token: q.quote_token, limit_micros: q.amount_micros }, accounts[user].token, randomUUID());
  if (r.status === 200) { holdings[user][m] = side === 'buy' ? 1000 : 0; if (record) stats.trade.push(r.ms); }
  else if (r.status === 409) { if (record) stats.conflicts++; } else stats.errors++;
}); }, 5);
console.log('Trading and 200 SSE streams active; waiting for the fixed observation window…');
while ((await required('/config')).server_time_ms < end) await new Promise((r) => setTimeout(r, 1000));
measuring = true;
const started = performance.now();
await parallel(markets, 5, (market) => required(`/admin/instances/${market.id}/evidence`, {
  source_id: 'benchmark-observer', event_id: `final-${run}`, source_revision: 1,
  window_start_ms: close, window_end_ms: end,
  observation: { kind: 'weather', station_id: 'benchmark', total_milli_mm: 250, complete: true },
  reference: 'Synthetic benchmark aggregate: complete rainfall coverage',
}, null, null, true));
const evidenceReadyMs = performance.now() - started;
const ids = new Set(markets.map((m) => m.id));
let completed = 0;
while (completed < 50 && performance.now() - started < 90000) {
  const list = await required('/instances?limit=100');
  completed = list.filter((m) => ids.has(m.id) && m.state === 'resolved').length;
  if (completed < 50) await new Promise((r) => setTimeout(r, 500));
}
const elapsed = performance.now() - started;
clearInterval(quoteTimer); clearInterval(tradeTimer);
await Promise.allSettled([...inFlight]); controller.abort(); await Promise.allSettled(streams);
const reconciliation = await required('/admin/reconcile', undefined, null, null, true);
let actualClaims = 0, invalidCredits = 0;
await parallel(accounts, 5, async (account) => {
  const portfolio = await required('/me/portfolio', undefined, account.token);
  const claims = portfolio.settlements.filter((c) => ids.has(c.instance_id));
  actualClaims += claims.length;
  invalidCredits += claims.filter((c) => c.credit_micros !== '1000000').length;
});
function summary(values) {
  const sorted = [...values].sort((a, b) => a - b);
  const p = (q) => sorted.length ? Number(sorted[Math.min(sorted.length - 1, Math.floor(sorted.length * q))].toFixed(2)) : null;
  return { samples: sorted.length, p50_ms: p(.5), p95_ms: p(.95), p99_ms: p(.99) };
}
const report = { timestamp: new Date().toISOString(), expected_claims: 10000, markets_completed: completed, sse_clients: connected,
  actual_claims: actualClaims, invalid_credits: invalidCredits,
  seconds_from_evidence_submission_start: elapsed / 1000, evidence_upload_ms: evidenceReadyMs,
  measurement: 'Real HTTP and fixed wall-clock windows; completion polled every 500 ms; includes evidence upload and finalization wait. Quote/trade samples cover settlement interval only.',
  nominal_finalize_ms: final, ...stats, quote: summary(stats.quote), trade: summary(stats.trade), reconciliation };
await writeFile(process.env.BENCH_REPORT || '.local/settlement-performance.json', JSON.stringify(report, null, 2) + '\n');
console.log(JSON.stringify(report, null, 2));
if (!reconciliation.ok || completed !== 50 || connected !== 200 || elapsed > 60000 || stats.errors || actualClaims !== 10000 || invalidCredits) process.exitCode = 1;
