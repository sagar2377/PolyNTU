// End-to-end browser test over CDP (Chrome DevTools Protocol) against the real
// app. Drives the actual React UI in headless Chrome: the whole demonstration
// runbook in docs/demo-runbook.md, plus the two password-prompt paths that a
// human demo does not normally reach.
//
// Covered: all three account types, the seeded demo price history, registration
// and the welcome gift, a signed quote and trade, the price/volume chart, the
// creator verification workflow through the administrator console, publishing a
// creator-authority market with the cached password-derived key, the creator's
// self-trading ban, the signed resolution, the portfolio settlement credit,
// reconciliation, the browse collapse, both password prompts, and the live
// instance: its hidden footer controls, its bus-only seeding, and a bracket
// settling from the external provider rather than the simulator.
//
// Prerequisites:
//   - simulated instance (demo mode) at E2E_BASE_URL, default 127.0.0.1:8000
//   - live instance (demo + live mode) at E2E_LIVE_URL, default 127.0.0.1:8001,
//     on its own database, pointed at a bus provider. Set E2E_SKIP_LIVE=1 to
//     run only the simulated half locally.
//   - an administrator token in POLYNTU_ADMIN_TOKEN, or the one in
//     .local/dev-secrets.json
//   - a Chrome with --remote-debugging-port on a dedicated --user-data-dir
//     (Chrome 136+ ignores the debug port on the default profile) reachable at
//     E2E_CDP_URL, default 127.0.0.1:9223
// Locally: scripts/run-sim.ps1, scripts/run-live-demo.ps1, a provider (the real
// provider/ntubus/service.py or scripts/ntubus-stub.mjs), and
//   chrome --headless=new --remote-debugging-port=9223 \
//     --user-data-dir=.local/chrome-profile about:blank
// A local run leaves its markets in the shared demo databases; run
// scripts/e2e-cleanup.ps1 afterwards to keep the demo clean (CI runs against
// its own throwaway databases).
const BASE = process.env.E2E_BASE_URL ?? "http://127.0.0.1:8000";
const LIVE = process.env.E2E_LIVE_URL ?? "http://127.0.0.1:8001";
const CDP = process.env.E2E_CDP_URL ?? "http://127.0.0.1:9223";
const SKIP_LIVE = process.env.E2E_SKIP_LIVE === "1";
const PASSWORD = "correct e2e password";
const log = (step, detail) => console.log(`[${String(step).padStart(2, "0")}] ${detail}`);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function waitForServer(base) {
  for (let i = 0; i < 120; i++) {
    try { await fetch(`${base}/health`); return; } catch { await sleep(1500); }
  }
  throw new Error(`app server never became ready: ${base}`);
}

async function connect() {
  const targets = await (await fetch(`${CDP}/json/list`)).json();
  const page = targets.find((t) => t.type === "page");
  if (!page) throw new Error("no CDP page target");
  const ws = new WebSocket(page.webSocketDebuggerUrl);
  await new Promise((res, rej) => { ws.onopen = res; ws.onerror = () => rej(new Error("CDP websocket failed")); });
  let nextId = 0;
  const pending = new Map();
  ws.onmessage = (event) => {
    const message = JSON.parse(event.data);
    if (message.id && pending.has(message.id)) { pending.get(message.id)(message); pending.delete(message.id); }
  };
  const send = (method, params = {}) => new Promise((resolve, reject) => {
    const id = ++nextId;
    pending.set(id, (m) => (m.error ? reject(new Error(`${method}: ${m.error.message}`)) : resolve(m.result)));
    ws.send(JSON.stringify({ id, method, params }));
  });
  return { ws, send };
}

async function evaluate(cdp, expression) {
  const result = await cdp.send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
  if (result.exceptionDetails) throw new Error(`page exception: ${result.exceptionDetails.exception?.description || result.exceptionDetails.text}`);
  return result.result.value;
}

// Navigate and wait for the NEW document to finish loading (a marker on the
// old context distinguishes it), then install the helpers there.
async function navigate(cdp, url) {
  await evaluate(cdp, "window.__nav = 1").catch(() => {});
  await cdp.send("Page.navigate", { url });
  let ready = false;
  for (let i = 0; i < 100 && !ready; i++) {
    await sleep(200);
    try { ready = await evaluate(cdp, "document.readyState === 'complete' && !window.__nav"); } catch { /* navigation in flight */ }
  }
  if (!ready) throw new Error(`page did not finish loading: ${url}`);
  await evaluate(cdp, HELPERS);
}

// React-controlled inputs need the native value setter plus a bubbled event.
const HELPERS = `window.__set = (selector, value) => {
  const el = document.querySelector(selector);
  if (!el) throw new Error("missing " + selector);
  const proto = el.tagName === "SELECT" ? HTMLSelectElement.prototype : el.tagName === "TEXTAREA" ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
  Object.getOwnPropertyDescriptor(proto, "value").set.call(el, value);
  el.dispatchEvent(new Event(el.tagName === "SELECT" ? "change" : "input", { bubbles: true }));
};
window.__submit = (selector) => { const el = document.querySelector(selector); const f = el?.closest("form"); if (!f) throw new Error("missing form for " + selector); f.requestSubmit(); };
window.__clickText = (selector, text) => {
  const el = [...document.querySelectorAll(selector)].find((e) => e.textContent.includes(text));
  if (!el) throw new Error("no " + selector + " containing " + text);
  el.click();
};
window.__has = (selector) => document.querySelector(selector) !== null;
window.__bodyHas = (text) => document.body.textContent.includes(text);
window.__footerHas = (text) => { const f = document.querySelector("footer"); return !!f && f.textContent.includes(text); };
window.__chipHas = (text) => (document.querySelector('.account-chip')?.textContent || "").includes(text);`;

async function waitFor(cdp, description, expression, timeout = 20000) {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    if (await evaluate(cdp, expression)) return;
    await sleep(300);
  }
  // Diagnose timeouts: what the page actually shows when the wait fails.
  const text = await evaluate(cdp, `document.body.textContent.replace(/\\s+/g, " ").slice(0, 300)`).catch(() => "unavailable");
  throw new Error(`timeout waiting for ${description}; page text: ${text}`);
}

const apiFor = (base) => ({
  get: async (path, { token, admin } = {}) => {
    const response = await fetch(base + "/api/v2" + path, { headers: { ...(token ? { authorization: `Bearer ${token}` } : {}), ...(admin ? { "x-admin-token": admin } : {}) } });
    const data = await response.json();
    if (!response.ok) throw new Error(`GET ${path} -> ${response.status}`);
    return data;
  },
  post: async (path, body, { token, admin, key } = {}) => {
    const response = await fetch(base + "/api/v2" + path, {
      method: "POST",
      headers: { "content-type": "application/json", ...(token ? { authorization: `Bearer ${token}` } : {}), ...(admin ? { "x-admin-token": admin } : {}), ...(key ? { "idempotency-key": key } : {}) },
      body: JSON.stringify(body ?? {}),
    });
    const data = await response.json().catch(() => null);
    if (!response.ok) throw new Error(`POST ${path} -> ${response.status} ${JSON.stringify(data)}`);
    return data;
  },
});
const api = apiFor(BASE);
const liveApi = apiFor(LIVE);

// The shared administrator token. A login would rotate the account's single
// session token and sign the browser out mid-test, so the token is used.
async function adminToken() {
  if (process.env.POLYNTU_ADMIN_TOKEN) return process.env.POLYNTU_ADMIN_TOKEN;
  const { readFile } = await import("node:fs/promises");
  // configure-dev.ps1 writes this file with a UTF-8 BOM, which JSON.parse
  // rejects, so the marker is stripped first.
  const text = await readFile(new URL("../.local/dev-secrets.json", import.meta.url), "utf8");
  return JSON.parse(text.replace(/^﻿/, "")).admin;
}

const pad = (n) => String(n).padStart(2, "0");
const localInputValue = (epochMs) => {
  const d = new Date(epochMs);
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
};

// Advance the demo clock into one instance's resolvable window: past its real
// observation end (read from the server, because the create form's
// datetime-local input truncates the close to the whole minute) and with at
// least 30 seconds left before the evidence deadline. Whole-minute advances
// preserve the clock's sub-minute phase, so the phase at the moment of the
// advance decides where inside the 60-second window the clock lands; wait for
// a phase that leaves room, or the deadline can pass while the test signs
// back in. Returns the number of minutes to advance.
async function advanceIntoResolveWindow(instanceId) {
  const instance = await api.get(`/instances/${instanceId}`);
  const observationEnd = instance.observation_end_ms;
  for (let attempt = 0; attempt < 90; attempt++) {
    const now = (await api.get("/config")).server_time_ms;
    const remaining = observationEnd - now;
    if (remaining <= 0) return 0; // the window is already open
    if (remaining >= 30000 && remaining % 60000 >= 30000) {
      return Math.max(1, Math.ceil(remaining / 60000));
    }
    await sleep(1000);
  }
  throw new Error("never reached a safe phase to advance into the resolve window");
}

const stamp = Date.now() % 100000;
const creatorEmail = `e2e.creator${stamp}@ntu.edu.sg`;
const traderEmail = `e2e.trader${stamp}@ntu.edu.sg`;
const titles = [`E2E signed rain market ${stamp}`, `E2E second signed market ${stamp}`];

async function register(cdp, name, email, password) {
  await evaluate(cdp, `__clickText('.signin-toggle button', 'Create account')`);
  await waitFor(cdp, "register form", `__has('#reg-email')`);
  await evaluate(cdp, `__set('#reg-name', ${JSON.stringify(name)})`);
  await evaluate(cdp, `__set('#reg-email', ${JSON.stringify(email)})`);
  await evaluate(cdp, `__set('#reg-password', ${JSON.stringify(password)})`);
  await evaluate(cdp, `__submit('form.stacked-form')`);
  await waitFor(cdp, "signed-in account chip", `__has('.account-chip')`);
}

async function signIn(cdp, email, password) {
  await evaluate(cdp, `__clickText('.signin-toggle button', 'Log in')`);
  await waitFor(cdp, "login form", `__has('#login-email')`);
  await evaluate(cdp, `__set('#login-email', ${JSON.stringify(email)})`);
  await evaluate(cdp, `__set('#login-password', ${JSON.stringify(password)})`);
  await evaluate(cdp, `__submit('#login-email')`);
  await waitFor(cdp, "signed-in account chip", `__has('.account-chip')`);
}

async function signOut(cdp) {
  await evaluate(cdp, `__clickText('button', 'Sign out')`);
  await waitFor(cdp, "signed out", `!__has('.account-chip')`);
}

async function publishSignedMarket(cdp, title, closeEpochMs, password) {
  await evaluate(cdp, `__clickText('nav button', 'Create market')`);
  await waitFor(cdp, "create form", `__has('#market-title')`);
  await evaluate(cdp, `__set('#market-title', ${JSON.stringify(title)})`);
  await evaluate(cdp, `__set('#market-criterion', ${JSON.stringify(`Resolves yes when the e2e station reports at least the threshold; e2e market ${title}`)})`);
  await evaluate(cdp, `__set('#market-station', 'e2e-station')`);
  await evaluate(cdp, `__set('#market-threshold', '100')`);
  await evaluate(cdp, `__set('#market-close', ${JSON.stringify(localInputValue(closeEpochMs))})`);
  // A 5-minute observation leaves a 60-second resolvable window between the
  // finalize point and the evidence deadline; the advance helper lands the
  // clock inside it using the instance's real timestamps.
  await evaluate(cdp, `__set('#market-observation', '5')`);
  await evaluate(cdp, `__set('#market-resolution', 'creator')`);
  if (password === undefined) {
    if (await evaluate(cdp, `__has('#market-key-password')`)) throw new Error("password field shown although the key was cached");
  } else {
    await waitFor(cdp, "password field at publish", `__has('#market-key-password')`, 5000);
    await evaluate(cdp, `__set('#market-key-password', ${JSON.stringify(password)})`);
  }
  await evaluate(cdp, `__submit('form.create-form')`);
  await waitFor(cdp, `market page for ${title}`, `__bodyHas(${JSON.stringify(title)}) && __has('.market-facts')`, 30000);
}

// Opening a series means opening one of its brackets: every bracket page
// carries the schedule, the day view, and the sibling bracket lists, so the
// browse grid's market card is the entry point. A closed market is collapsed
// behind the browse history tabs, so open the Closed tab when it is not live.
async function openSeries(cdp, title) {
  await evaluate(cdp, `__clickText('button.wordmark', 'Poly')`);
  await waitFor(cdp, "browse page loaded", `!__bodyHas('Loading markets…')`, 15000);
  const cardInGrid = `[...document.querySelectorAll('.market-card h3')].some((h) => h.textContent === ${JSON.stringify(title)})`;
  if (!(await evaluate(cdp, cardInGrid))) {
    await evaluate(cdp, `__clickText('.tab-bar button', 'Closed')`);
    await waitFor(cdp, `market card for ${title}`, cardInGrid, 15000);
  }
  await evaluate(cdp, `__clickText('.market-card h3', ${JSON.stringify(title)})`);
  await waitFor(cdp, `market page for ${title}`, `__bodyHas(${JSON.stringify(title)}) && __has('.market-facts')`, 30000);
}

async function resolveFromClosedTab(cdp) {
  await waitFor(cdp, "bracket history tabs", `__has('.tab-bar')`, 30000);
  await evaluate(cdp, `__clickText('.tab-bar button', 'Closed')`);
  await waitFor(cdp, "resolve control", `__has('.resolve-control select')`, 20000);
  await evaluate(cdp, `__clickText('.resolve-control button', 'Resolve')`);
  await waitFor(cdp, "resolved count in the tab", `(() => { const b = [...document.querySelectorAll('.tab-bar button')].find((x) => x.textContent.startsWith('Resolved')); return b && !b.textContent.includes('(0)'); })()`, 30000);
  await evaluate(cdp, `__clickText('.tab-bar button', 'Resolved')`);
  await waitFor(cdp, "settled result", `[...document.querySelectorAll('.bracket-list li')].some((li) => li.textContent.includes('Result:'))`);
}

// A resolver-authority bus series on the live instance whose operating window
// covers the current hour. The seeded lines only run 07:30 to 23:00 SGT, and
// the adapter accepts any bus market with a resolver authority, so this is how
// the live path is exercised at any hour. One-minute brackets keep the wait
// short: a bracket closes, its observation window ends, and the worker asks.
async function publishLiveBusSeries(token) {
  const now = (await liveApi.get("/config")).server_time_ms;
  const sgt = new Date(now + 8 * 3600_000);
  const startMinute = sgt.getUTCHours() * 60;
  const series = await liveApi.post("/series", {
    title: `E2E live bus ${stamp}`,
    resolution_criterion: "Yes if at least one matching bus arrives in the published window. No requires complete observation coverage with no matching arrival.",
    rule: { kind: "bus", route_id: "NTU-blue", direction: "clockwise", stop_id: "opp-spms" },
    source_id: "polyntu-creator",
    liquidity_units: 100,
    fee_charged: false,
    resolution: { kind: "resolver", endpoint: `${LIVE}/api/v2/resolvers/ntu-bus` },
    schedule: { kind: "recurring", interval_ms: 60000, active_start_minute: startMinute, active_end_minute: Math.min(1439, startMinute + 60), active_days: [1, 2, 3, 4, 5, 6, 7], max_concurrency: 2, end_ms: null },
  }, { token });
  for (let i = 0; i < 60; i++) {
    const detail = await liveApi.get(`/series/${series.id}`);
    const open = detail.instances.find((inst) => inst.state === "open");
    if (open) return { seriesId: series.id, instanceId: open.id };
    await sleep(500);
  }
  throw new Error("the published live bus series never spawned a bracket");
}

// Wait for one live bracket to reach a terminal state. A live bracket either
// records provider evidence or voids at its deadline; it must never record a
// fabricated outcome.
async function waitForLiveSettlement(instanceId) {
  const deadline = Date.now() + 5 * 60_000;
  while (Date.now() < deadline) {
    const instance = await liveApi.get(`/instances/${instanceId}`);
    if (instance.state === "resolved" || instance.state === "voided") return instance;
    await sleep(3000);
  }
  throw new Error("the live bracket never settled or voided within five minutes");
}

async function main() {
  await waitForServer(BASE);
  if (!SKIP_LIVE) await waitForServer(LIVE);
  const cdp = await connect();
  await evaluate(cdp, HELPERS);

  // ---- The simulated instance ------------------------------------------
  await navigate(cdp, BASE);
  // Start from a guest state regardless of what earlier runs left behind.
  await evaluate(cdp, `localStorage.clear()`);
  await navigate(cdp, BASE);

  // 1. Guest browse, and the seeded demo trading that makes the charts move.
  await waitFor(cdp, "market grid", `document.querySelectorAll('.market-card').length > 0`, 30000);
  const cards = await evaluate(cdp, `[...document.querySelectorAll('.market-card')].map((c) => c.textContent)`);
  log(1, `visitor sees ${cards.length} live market cards without signing in`);
  // The demo participants trade the seeded one-time markets, so the grid must
  // not read a flat 50/50 everywhere. The first fill lands on an early worker
  // tick; allow time for a few.
  let moved = [];
  for (let attempt = 0; attempt < 60 && moved.length < 3; attempt++) {
    const open = await api.get("/instances?state=open&limit=100");
    const oneTime = open.filter((i) => i.series_id === null);
    if (oneTime.length < 5) throw new Error(`expected 5 one-time demo markets, saw ${oneTime.length}`);
    moved = oneTime.filter((i) => i.outcomes.some((o) => Math.abs(o.probability - 0.5) > 0.005));
    if (moved.length < 3) await sleep(2000);
  }
  if (moved.length < 3) throw new Error("the seeded demo markets are still a flat 50/50 line: no demo trading happened");
  log(2, `${moved.length} of the seeded demo markets have moved off 50/50 from demo trading`);

  // 2. Register a member; the signing key is derived and cached at registration.
  await register(cdp, "E2E Creator", creatorEmail, PASSWORD);
  await waitFor(cdp, "member role in the chip", `__chipHas('member')`);
  await waitFor(cdp, "cached signing key", `localStorage.getItem('polyntu.v2.signing-key') !== null`, 15000);
  const cachedKey = JSON.parse(await evaluate(cdp, `localStorage.getItem('polyntu.v2.signing-key')`));
  if (cachedKey.email !== creatorEmail || !cachedKey.public_key) throw new Error("signing key cache is wrong");
  log(3, `registered ${creatorEmail} as a member; the signing key was derived from the password and cached`);

  // 3. Trade a seeded market: signed quote, confirm, receipt, and the chart.
  await evaluate(cdp, `__clickText('.market-card h3', 'Rainfall')`);
  await waitFor(cdp, "trade panel", `__has('#quantity')`, 20000);
  await evaluate(cdp, `__submit('.trade-panel form')`);
  await waitFor(cdp, "quote preview", `__has('.quote-preview')`);
  await evaluate(cdp, `__clickText('.quote-preview button', 'Confirm buy')`);
  await waitFor(cdp, "trade receipt", `__has('.receipt')`);
  await waitFor(cdp, "price history chart", `document.querySelectorAll('.chart-container canvas').length > 0`, 15000);
  log(4, "member previewed a signed quote, confirmed the buy, and the price/volume chart rendered");

  // 4. File a verification request.
  await evaluate(cdp, `__clickText('button.wordmark', 'Poly')`);
  await waitFor(cdp, "verification panel", `__bodyHas('Become a market creator')`, 15000);
  await evaluate(cdp, `__clickText('button', 'Request creator verification')`);
  await waitFor(cdp, "pending review", `__bodyHas('pending administrator review')`);
  log(5, "member filed a creator verification request");

  // 5. Approve it from the administrator console.
  const admin = await adminToken();
  await signOut(cdp);
  await signIn(cdp, "admin@ntu.edu.sg", "admin");
  await waitFor(cdp, "admin role in the chip", `__chipHas('admin')`);
  await waitFor(cdp, "admin footer button", `__footerHas('Admin')`);
  await evaluate(cdp, `__clickText('footer button', 'Admin')`);
  await waitFor(cdp, "administrator console", `__bodyHas('Administrator console')`);
  await evaluate(cdp, `__set('#console-token', ${JSON.stringify(admin)})`);
  await evaluate(cdp, `__clickText('button', 'Load pending requests')`);
  await waitFor(cdp, "pending request in the console", `document.querySelectorAll('.admin-list li').length > 0`);
  await evaluate(cdp, `[...document.querySelectorAll('.admin-list li')].find((li) => li.textContent.includes('E2E Creator'))?.querySelector('button').click()`);
  await waitFor(cdp, "approval confirmation", `__bodyHas('Approved')`, 15000);
  log(6, "administrator approved the request from the console UI");

  await signOut(cdp);
  await signIn(cdp, creatorEmail, PASSWORD);
  await waitFor(cdp, "creator role in the chip", `__chipHas('creator')`);
  await waitFor(cdp, "create market nav", `__bodyHas('Create market')`, 15000);
  log(7, "the member now holds the creator role and the Create market button appeared");

  // 6. Publish a creator-authority market; the cached key means no prompt.
  const config = await api.get("/config");
  await publishSignedMarket(cdp, titles[0], config.server_time_ms + 3 * 60000);
  log(8, `published "${titles[0]}" with the cached password-derived key (no prompt)`);
  const seriesList = await api.get("/series");
  const series1 = await api.get(`/series/${seriesList.find((s) => s.title === titles[0])?.id}`);
  if (!series1 || series1.resolution?.authority !== "creator") throw new Error("series not published with creator authority");
  if (series1.resolution.public_key !== cachedKey.public_key) throw new Error("published public key does not match the cached key");
  log(9, "the published series fixes creator authority and the key derived from the password");

  // 7. The creator cannot trade its own market.
  const creatorSession = await api.post("/auth/login", { email: creatorEmail, password: PASSWORD });
  const selfTrade = await api
    .post("/quotes", { instance_id: series1.instances[0].id, outcome_id: "yes", side: "buy", quantity_millis: 1000 }, { token: creatorSession.token })
    .then(() => "allowed")
    .catch((e) => (/own markets/.test(e.message) ? "blocked" : `unexpected: ${e.message}`));
  if (selfTrade !== "blocked") throw new Error(`self-trading was not blocked: ${selfTrade}`);
  log(10, "the creator's own trade is rejected: creators cannot trade in their own markets");

  // 8. A second member trades it, so it has a position and a price history.
  await signOut(cdp);
  await register(cdp, "E2E Trader", traderEmail, PASSWORD);
  await openSeries(cdp, titles[0]);
  await waitFor(cdp, "trade panel", `__has('#quantity')`);
  await evaluate(cdp, `__submit('.trade-panel form')`);
  await waitFor(cdp, "quote preview", `__has('.quote-preview')`);
  await evaluate(cdp, `__clickText('.quote-preview button', 'Confirm buy')`);
  await waitFor(cdp, "trade receipt", `__has('.receipt')`);
  log(11, "a second member traded the creator's market through the quote and confirm flow");

  // 9. Close the market and resolve it with the creator's cached key. Signing
  //    back in happens BEFORE advancing: the resolvable window is only sixty
  //    seconds of demo time, so no sign-in work may happen inside it.
  await signOut(cdp);
  await signIn(cdp, creatorEmail, PASSWORD);
  const advanced = await advanceIntoResolveWindow(series1.instances[0].id);
  await api.post("/admin/clock/advance", { minutes: advanced }, { admin });
  log(12, `demo clock advanced ${advanced} minutes into the resolvable window`);
  await openSeries(cdp, titles[0]);
  await resolveFromClosedTab(cdp);
  log(13, "creator resolved the bracket with the cached key; the result is in the Resolved tab");

  const series1After = await api.get(`/series/${series1.id}`);
  const resolved1 = series1After.instances.find((i) => i.state === "resolved");
  if (!resolved1) throw new Error("instance not resolved on the server");
  const evidence1 = (await api.get(`/instances/${resolved1.id}`)).evidence;
  // `source_id` names the evidence authority; `source` only carries which bus
  // path answered, so it is null for a creator signature.
  if (evidence1?.source_id !== "creator-signature") throw new Error(`unexpected evidence source: ${evidence1?.source_id}`);
  if (!series1After.day?.slots?.length) throw new Error("day view slots missing");
  await waitFor(cdp, "day probability bars", `document.querySelectorAll('.day-bar-row').length > 0`, 15000);
  log(14, `server recorded evidence with source_id "creator-signature"; the day view renders`);

  // 10. The trader's portfolio shows the settlement credit.
  await signOut(cdp);
  await signIn(cdp, traderEmail, PASSWORD);
  await evaluate(cdp, `__clickText('nav button', 'Portfolio')`);
  await waitFor(cdp, "portfolio", `__bodyHas('Outcome shares')`, 15000);
  await waitFor(cdp, "settlement credit", `__bodyHas('Settlement credits') && !__bodyHas('No settlement credits yet')`, 15000);
  log(15, "the trader's portfolio shows the settled position and its settlement credit");

  // 11. Reconciliation: balances, inventory, reserves, and net units agree.
  const reconcile = await api.get("/admin/reconcile", { admin });
  if (!reconcile.ok) throw new Error(`reconciliation found discrepancies: ${JSON.stringify(reconcile)}`);
  if (reconcile.net_units_micros !== "0") throw new Error(`ledger does not sum to zero: ${reconcile.net_units_micros}`);
  log(16, "reconciliation reports no discrepancies and a zero net ledger");

  // 12. The browse grid lists live markets only; settled ones sit behind the
  //     hidden-by-default history tabs.
  await evaluate(cdp, `__clickText('button.wordmark', 'Poly')`);
  await waitFor(cdp, "browse history tabs", `__has('.tab-bar')`, 15000);
  if (await evaluate(cdp, `[...document.querySelectorAll('.market-card h3')].some((h) => h.textContent === ${JSON.stringify(titles[0])})`)) throw new Error("resolved market still shown in the live grid");
  await evaluate(cdp, `__clickText('.tab-bar button', 'Resolved')`);
  await waitFor(cdp, "resolved card in the history tab", `[...document.querySelectorAll('.market-card h3')].some((h) => h.textContent === ${JSON.stringify(titles[0])})`);
  log(17, "the browse grid collapsed the settled market behind the Resolved tab");

  // 13. Second market: clear the cached key to exercise both password prompts.
  //     The portfolio step above left the browser signed in as the trader, so
  //     return to the creator first.
  await signOut(cdp);
  await signIn(cdp, creatorEmail, PASSWORD);
  await waitFor(cdp, "creator role in the chip", `__chipHas('creator')`);
  await evaluate(cdp, `localStorage.removeItem('polyntu.v2.signing-key')`);
  await navigate(cdp, BASE);
  const config2 = await api.get("/config");
  await waitFor(cdp, "creator nav button restored", `__bodyHas('Create market')`, 15000);
  await publishSignedMarket(cdp, titles[1], config2.server_time_ms + 3 * 60000, PASSWORD);
  log(18, "the publish form asked for the password once the cache was cleared, and published with the derived key");
  const series2 = await api.get(`/series/${(await api.get("/series")).find((s) => s.title === titles[1])?.id}`);
  if (!series2 || series2.resolution?.authority !== "creator") throw new Error("second series not published with creator authority");

  await evaluate(cdp, `localStorage.removeItem('polyntu.v2.signing-key')`);
  await navigate(cdp, BASE);
  const advanced2 = await advanceIntoResolveWindow(series2.instances[0].id);
  await api.post("/admin/clock/advance", { minutes: advanced2 }, { admin });
  await openSeries(cdp, titles[1]);
  await waitFor(cdp, "bracket history tabs", `__has('.tab-bar')`, 30000);
  await evaluate(cdp, `__clickText('.tab-bar button', 'Closed')`);
  if (!(await evaluate(cdp, `__has('#key-password')`))) throw new Error("password prompt missing on the bracket page");
  if (await evaluate(cdp, `__has('.resolve-control select')`)) throw new Error("resolve control shown without a matching key");
  await evaluate(cdp, `__set('#key-password', ${JSON.stringify(PASSWORD)})`);
  await evaluate(cdp, `__clickText('.resolve-control button', 'Derive signing key')`);
  await waitFor(cdp, "resolve control after derivation", `__has('.resolve-control select')`, 20000);
  await evaluate(cdp, `__clickText('.resolve-control button', 'Resolve')`);
  await waitFor(cdp, "resolved count in the tab", `(() => { const b = [...document.querySelectorAll('.tab-bar button')].find((x) => x.textContent.startsWith('Resolved')); return b && !b.textContent.includes('(0)'); })()`, 30000);
  await evaluate(cdp, `__clickText('.tab-bar button', 'Resolved')`);
  await waitFor(cdp, "settled result", `[...document.querySelectorAll('.bracket-list li')].some((li) => li.textContent.includes('Result:'))`);
  const series2After = await api.get(`/series/${series2.id}`);
  if (!series2After.instances.some((i) => i.state === "resolved")) throw new Error("second instance not resolved on the server");
  log(19, "resolved the second market through the password prompt after losing the cached key");

  // ---- The live instance ------------------------------------------------
  if (SKIP_LIVE) {
    log(20, "live instance checks skipped (E2E_SKIP_LIVE=1)");
  } else {
    await navigate(cdp, LIVE);
    const liveConfig = await liveApi.get("/config");
    if (!liveConfig.live_mode) throw new Error("the second instance is not in live mode");
    // The footer must not offer operations that cannot run in live mode.
    if (await evaluate(cdp, `__footerHas('Admin')`)) throw new Error("the Admin button is still shown in live mode");
    if (await evaluate(cdp, `__footerHas('Demo clock')`)) throw new Error("the Demo clock button is still shown in live mode");
    log(20, "the live instance is in live mode and hides both the Admin and Demo clock footer buttons");

    // Live mode seeds the bus series only: simulator-only one-time markets
    // would void there, because live mode never fabricates evidence.
    const liveOpen = await liveApi.get("/instances?state=open&limit=100");
    const liveOneTime = liveOpen.filter((i) => i.series_id === null);
    if (liveOneTime.length > 0) throw new Error(`live mode seeded ${liveOneTime.length} simulator-only one-time markets, which could only void`);
    log(21, "live mode seeds only the bus series: no simulator-only markets that could only void");

    // Publish a bus series covering the current hour, so the live path is
    // exercised regardless of the seeded lines' 07:30 to 23:00 service span.
    const liveCreatorEmail = `e2e.live${stamp}@ntu.edu.sg`;
    const liveSession = await liveApi.post("/auth/register", { display_name: "E2E Live Creator", email: liveCreatorEmail, password: PASSWORD });
    const verification = await liveApi.post("/verification-requests", {}, { token: liveSession.token });
    await liveApi.post(`/admin/verification-requests/${verification.id}/decision`, { approve: true }, { admin });
    const bus = await publishLiveBusSeries(liveSession.token);
    log(22, "published a resolver-authority bus series covering the current hour on the live instance");

    await navigate(cdp, LIVE);
    await waitFor(cdp, "live grid", `document.querySelectorAll('.market-card').length > 0`, 30000);
    await evaluate(cdp, `__clickText('.market-card h3', 'E2E live bus')`);
    await waitFor(cdp, "market page", `__has('.market-facts')`, 20000);
    await waitFor(cdp, "bus resolver shown", `__bodyHas('NTU Bus API')`, 15000);
    log(23, "the bus bracket page names the NTU Bus API adapter as its resolution authority");

    // The bracket closes, its observation window ends, and the worker asks the
    // adapter, which asks the provider.
    const settled = await waitForLiveSettlement(bus.instanceId);
    if (settled.state === "resolved" && settled.evidence?.source !== "ntubus-live") {
      throw new Error(`a live bracket resolved from "${settled.evidence?.source}", which is not the live feed`);
    }
    if (settled.state === "voided") {
      throw new Error("the live bracket voided: the provider did not answer, so the live path is not working");
    }
    log(24, `the live bracket settled from "${settled.evidence.source}": the provider answered, not the simulator`);
  }

  console.log("E2E PASSED");
}

// The CDP websocket keeps the Node event loop alive after a pass, so exit
// explicitly in both directions.
main().then(() => process.exit(0)).catch((e) => { console.error("E2E FAILED:", e.message); process.exit(1); });
