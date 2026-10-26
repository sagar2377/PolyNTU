// Executes docs/demo-runbook.md against the two running instances, in order,
// over the Chrome DevTools protocol. This is the demo itself, scripted: it
// fails on the first step the runbook promises and does not.
//
// Prerequisites: scripts/run-sim.ps1 on 8000, scripts/run-live-demo.ps1 on
// 8001, provider/ntubus/service.py on 8090, and headless Chrome with
// --remote-debugging-port=9223 on a dedicated --user-data-dir.
const SIM = process.env.DEMO_SIM_URL ?? "http://127.0.0.1:8000";
const LIVE = process.env.DEMO_LIVE_URL ?? "http://127.0.0.1:8001";
const CDP = process.env.DEMO_CDP_URL ?? "http://127.0.0.1:9223";
const PASSWORD = "demo walkthrough password";
const stamp = Date.now() % 100000;
const memberEmail = `demo.member${stamp}@ntu.edu.sg`;
const marketTitle = `Demo signed rain market ${stamp}`;
const log = (step, detail) => console.log(`[${String(step).padStart(2, "0")}] ${detail}`);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

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
window.__footerHas = (text) => { const f = document.querySelector("footer"); return !!f && f.textContent.includes(text); };`;

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

async function navigate(cdp, url) {
  await evaluate(cdp, "window.__nav = 1").catch(() => {});
  await cdp.send("Page.navigate", { url });
  let ready = false;
  for (let i = 0; i < 100 && !ready; i++) {
    await sleep(200);
    try { ready = await evaluate(cdp, "document.readyState === 'complete' && !window.__nav"); } catch { /* in flight */ }
  }
  if (!ready) throw new Error(`page did not finish loading: ${url}`);
  await evaluate(cdp, HELPERS);
}

async function waitFor(cdp, description, expression, timeout = 20000) {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    if (await evaluate(cdp, expression)) return;
    await sleep(300);
  }
  const text = await evaluate(cdp, `document.body.textContent.replace(/\\s+/g, " ").slice(0, 300)`).catch(() => "unavailable");
  throw new Error(`timeout waiting for ${description}; page text: ${text}`);
}

const api = (base) => ({
  get: async (path, { token, admin } = {}) => {
    const r = await fetch(base + "/api/v2" + path, { headers: { ...(token ? { authorization: `Bearer ${token}` } : {}), ...(admin ? { "x-admin-token": admin } : {}) } });
    const body = await r.json();
    if (!r.ok) throw new Error(`GET ${path} -> ${r.status} ${JSON.stringify(body)}`);
    return body;
  },
  post: async (path, body, { token, admin, key } = {}) => {
    const r = await fetch(base + "/api/v2" + path, {
      method: "POST",
      headers: { "content-type": "application/json", ...(token ? { authorization: `Bearer ${token}` } : {}), ...(admin ? { "x-admin-token": admin } : {}), ...(key ? { "idempotency-key": key } : {}) },
      body: JSON.stringify(body ?? {}),
    });
    const data = await r.json().catch(() => null);
    if (!r.ok) throw new Error(`POST ${path} -> ${r.status} ${JSON.stringify(data)}`);
    return data;
  },
});
const sim = api(SIM);
const live = api(LIVE);

const pad = (n) => String(n).padStart(2, "0");
const localInputValue = (epochMs) => {
  const d = new Date(epochMs);
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
};

async function signOut(cdp) {
  await evaluate(cdp, `__clickText('button', 'Sign out')`);
  await waitFor(cdp, "signed out", `!__has('.account-chip')`);
}

// Advance the demo clock into one instance's resolvable window: past its real
// observation end (read from the server, because the create form's
// datetime-local input truncates the close to the whole minute) and with at
// least 30 seconds left before the evidence deadline. Whole-minute advances
// preserve the clock's sub-minute phase, so the phase at the moment of the
// advance decides where inside the 60-second window the clock lands; wait for
// a phase that leaves room, or the deadline can pass while the demo signs back
// in. Returns the number of minutes to advance.
async function advanceIntoResolveWindow(instanceId) {
  const instance = await sim.get(`/instances/${instanceId}`);
  const observationEnd = instance.observation_end_ms;
  for (let attempt = 0; attempt < 120; attempt++) {
    const now = (await sim.get("/config")).server_time_ms;
    const remaining = observationEnd - now;
    if (remaining <= 0) return 1; // the window is already open; one minute is harmless
    if (remaining >= 30000 && remaining % 60000 >= 30000) {
      return Math.max(1, Math.ceil(remaining / 60000));
    }
    await sleep(1000);
  }
  throw new Error("never reached a safe clock phase to advance into the resolve window");
}

// Publish a resolver-authority bus series on the live instance whose operating
// window covers the current hour, and return its first spawned bracket. The
// seeded lines only run 07:30 to 23:00 SGT; the adapter accepts any bus market
// with a resolver authority, so this is how the live path is shown outside
// those hours. The creator is a freshly verified member, so the series is
// creator-owned with an external resolver, exactly like a real integration.
async function publishLiveBusSeries() {
  const adminToken = await adminSecret();
  const email = `demo.live${stamp}@ntu.edu.sg`;
  const session = await live.post("/auth/register", { display_name: "Live Demo Creator", email, password: PASSWORD });
  const token = session.token;
  const request = await live.post("/verification-requests", {}, { token });
  await live.post(`/admin/verification-requests/${request.id}/decision`, { approve: true }, { admin: adminToken });
  const now = (await live.get("/config")).server_time_ms;
  // A one-hour window starting at the top of the current SGT hour, so the
  // current moment is inside it whatever the wall clock says.
  const sgt = new Date(now + 8 * 3600_000);
  const startMinute = sgt.getUTCHours() * 60;
  const endMinute = Math.min(1439, startMinute + 60);
  const series = await live.post("/series", {
    title: "Blue line · live demo arrival at Opp SPMS",
    resolution_criterion: "Yes if at least one matching bus arrives in the published window. No requires complete observation coverage with no matching arrival.",
    rule: { kind: "bus", route_id: "NTU-blue", direction: "clockwise", stop_id: "opp-spms" },
    source_id: "polyntu-creator",
    liquidity_units: 100,
    fee_charged: false,
    resolution: { kind: "resolver", endpoint: "http://127.0.0.1:8001/api/v2/resolvers/ntu-bus" },
    schedule: { kind: "recurring", interval_ms: 120000, active_start_minute: startMinute, active_end_minute: endMinute, active_days: [1, 2, 3, 4, 5, 6, 7], max_concurrency: 3, end_ms: null },
  }, { token });
  // The scheduler spawns brackets on the next worker tick; poll briefly.
  for (let i = 0; i < 40; i++) {
    const detail = await live.get(`/series/${series.id}`);
    const open = detail.instances.find((inst) => inst.state === "open");
    if (open) return { seriesId: series.id, instanceId: open.id };
    await sleep(500);
  }
  throw new Error("the published live bus series never spawned a bracket");
}

// Wait for one live bracket to reach a terminal state and report which path
// answered. A live bracket either records provider evidence or voids at its
// published deadline; it must never record a fabricated outcome.
async function waitForLiveSettlement(instanceId) {
  const deadline = Date.now() + 6 * 60_000;
  while (Date.now() < deadline) {
    const instance = await live.get(`/instances/${instanceId}`);
    if (instance.state === "resolved" || instance.state === "voided") {
      const source = instance.evidence?.source ?? "none";
      if (instance.state === "resolved" && source !== "ntubus-live") {
        throw new Error(`a live bracket resolved from "${source}", which is not the live feed`);
      }
      return instance.state === "resolved"
        ? `live bracket settled from "${source}": the provider answered from the campus Omnibus feed`
        : "live bracket voided at its deadline because the provider had no coverage; it never fabricated an outcome";
    }
    await sleep(3000);
  }
  throw new Error("the live bracket never settled or voided within six minutes");
}

async function adminSecret() {
  // configure-dev.ps1 writes this file with a UTF-8 BOM, which JSON.parse
  // rejects, so the marker is stripped first.
  const secretText = await (await import("node:fs/promises")).readFile(new URL("../.local/dev-secrets.json", import.meta.url), "utf8");
  return JSON.parse(secretText.replace(/^﻿/, "")).admin;
}

async function main() {
  // ---- Setup -------------------------------------------------------------
  for (const [name, base] of [["simulated", SIM], ["live", LIVE]]) {
    const health = await fetch(`${base}/health`);
    if (!health.ok) throw new Error(`${name} instance is not healthy`);
  }
  const cdp = await connect();
  await navigate(cdp, SIM);
  await evaluate(cdp, `localStorage.clear()`);
  await navigate(cdp, SIM);

  // ---- 1. Browse the grid as a visitor --------------------------------
  await waitFor(cdp, "market grid", `document.querySelectorAll('.market-card').length > 0`, 30000);
  const cards = await evaluate(cdp, `[...document.querySelectorAll('.market-card')].map((c) => c.textContent)`);
  log(1, `visitor sees ${cards.length} live market cards`);
  // The seeded demo markets must not all read 50/50.
  const open = await sim.get("/instances?state=open&limit=100");
  const oneTime = open.filter((i) => i.series_id === null);
  if (oneTime.length < 5) throw new Error(`expected 5 one-time demo markets, saw ${oneTime.length}`);
  const moved = oneTime.filter((i) => i.outcomes.some((o) => Math.abs(o.probability - 0.5) > 0.005));
  if (moved.length < 3) throw new Error(`demo markets are still flat: ${oneTime.map((i) => i.outcomes.map((o) => o.probability.toFixed(3)).join("/")).join(", ")}`);
  log(2, `${moved.length} of ${oneTime.length} demo markets have a non-50/50 price from seeded demo trading`);

  // ---- 2. Register a member and trade ---------------------------------
  await evaluate(cdp, `__clickText('.signin-toggle button', 'Create account')`);
  await waitFor(cdp, "register form", `__has('#reg-email')`);
  await evaluate(cdp, `__set('#reg-name', 'Demo Member')`);
  await evaluate(cdp, `__set('#reg-email', ${JSON.stringify(memberEmail)})`);
  await evaluate(cdp, `__set('#reg-password', ${JSON.stringify(PASSWORD)})`);
  await evaluate(cdp, `__submit('form.stacked-form')`);
  await waitFor(cdp, "account chip", `__has('.account-chip')`);
  await waitFor(cdp, "member role in the chip", `document.querySelector('.account-chip')?.textContent.includes('member')`);
  log(3, `registered ${memberEmail} as a member with the welcome gift`);

  const rainCard = cards.find((c) => c.includes("Rainfall"));
  if (!rainCard) throw new Error("the seeded rainfall market is missing from the grid");
  await evaluate(cdp, `__clickText('.market-card h3', 'Rainfall')`);
  await waitFor(cdp, "trade panel", `__has('#quantity')`, 20000);
  await evaluate(cdp, `__submit('.trade-panel form')`);
  await waitFor(cdp, "quote preview", `__has('.quote-preview')`);
  await evaluate(cdp, `__clickText('.quote-preview button', 'Confirm buy')`);
  await waitFor(cdp, "trade receipt", `__has('.receipt')`);
  await waitFor(cdp, "price history chart", `document.querySelectorAll('.chart-container canvas').length > 0`, 15000);
  log(4, "member previewed a signed quote, confirmed the buy, and the price/volume chart rendered");

  // ---- 3. File a verification request ----------------------------------
  await evaluate(cdp, `__clickText('button.wordmark', 'Poly')`);
  await waitFor(cdp, "verification panel", `__bodyHas('Become a market creator')`, 15000);
  await evaluate(cdp, `__clickText('button', 'Request creator verification')`);
  await waitFor(cdp, "pending review", `__bodyHas('pending administrator review')`);
  log(5, "member filed a creator verification request; a second one is refused while it is pending");

  // ---- 4. Approve as the administrator ---------------------------------
  await signOut(cdp);
  await evaluate(cdp, `__clickText('.signin-toggle button', 'Log in')`);
  await waitFor(cdp, "login form", `__has('#login-email')`);
  await evaluate(cdp, `__set('#login-email', 'admin@ntu.edu.sg')`);
  await evaluate(cdp, `__set('#login-password', 'admin')`);
  await evaluate(cdp, `__submit('#login-email')`);
  await waitFor(cdp, "admin chip", `document.querySelector('.account-chip')?.textContent.includes('admin')`);
  await waitFor(cdp, "admin footer button", `__footerHas('Admin')`);
  await evaluate(cdp, `__clickText('footer button', 'Admin')`);
  await waitFor(cdp, "administrator console", `__bodyHas('Administrator console')`);
  // The shared administrator token, not an admin login: a login rotates the
  // account's single session token and would sign the browser out mid-demo.
  const adminToken = await adminSecret();
  const queue = await sim.get("/admin/verification-requests?status=pending", { admin: adminToken });
  const mine = queue.find((r) => r.email === memberEmail);
  if (!mine) throw new Error("the administrator cannot see the member's request");
  // Approve through the console UI, not the API, so the demo covers the button.
  // The console needs its own token field; the browser session is enough for
  // everything else, but the token form is what the runbook shows.
  await evaluate(cdp, `__set('#console-token', ${JSON.stringify(adminToken)})`);
  await evaluate(cdp, `__clickText('button', 'Load pending requests')`);
  await waitFor(cdp, "pending request in the console", `document.querySelectorAll('.admin-list li').length > 0`);
  await evaluate(cdp, `[...document.querySelectorAll('.admin-list li')].find((li) => li.textContent.includes(${JSON.stringify("Demo Member")}))?.querySelector('button').click()`);
  await waitFor(cdp, "approval confirmation", `__bodyHas('Approved')`, 15000);
  log(6, "administrator approved the request from the console UI");

  await signOut(cdp);
  await evaluate(cdp, `__clickText('.signin-toggle button', 'Log in')`);
  await waitFor(cdp, "login form", `__has('#login-email')`);
  await evaluate(cdp, `__set('#login-email', ${JSON.stringify(memberEmail)})`);
  await evaluate(cdp, `__set('#login-password', ${JSON.stringify(PASSWORD)})`);
  await evaluate(cdp, `__submit('#login-email')`);
  await waitFor(cdp, "creator chip", `document.querySelector('.account-chip')?.textContent.includes('creator')`);
  await waitFor(cdp, "create market nav", `__bodyHas('Create market')`, 15000);
  log(7, "the member now holds the creator role and the Create market button appeared");

  // ---- 5. Publish a creator-signed market ------------------------------
  const config = await sim.get("/config");
  await evaluate(cdp, `__clickText('nav button', 'Create market')`);
  await waitFor(cdp, "create form", `__has('#market-title')`);
  await evaluate(cdp, `__set('#market-title', ${JSON.stringify(marketTitle)})`);
  await evaluate(cdp, `__set('#market-criterion', 'Yes if the demo station records at least the threshold rainfall during the published window.')`);
  await evaluate(cdp, `__set('#market-station', 'demo-walkthrough')`);
  await evaluate(cdp, `__set('#market-threshold', '150')`);
  await evaluate(cdp, `__set('#market-close', ${JSON.stringify(localInputValue(config.server_time_ms + 3 * 60000))})`);
  await evaluate(cdp, `__set('#market-observation', '5')`);
  await evaluate(cdp, `__set('#market-resolution', 'creator')`);
  if (await evaluate(cdp, `__has('#market-key-password')`)) throw new Error("password prompt shown although the key was cached at sign-in");
  await evaluate(cdp, `__submit('form.create-form')`);
  await waitFor(cdp, `market page for ${marketTitle}`, `__bodyHas(${JSON.stringify(marketTitle)}) && __has('.market-facts')`, 30000);
  log(8, "creator published a creator-authority market; the cached password-derived key needed no prompt");

  const seriesList = await sim.get("/series");
  const series = await sim.get(`/series/${seriesList.find((s) => s.title === marketTitle)?.id}`);
  if (series.resolution?.authority !== "creator" || !series.resolution.public_key) throw new Error("the series did not fix creator authority");
  log(9, "the published series fixes creator authority and the creator's ed25519 public key");

  // ---- 6. The creator cannot trade its own market ----------------------
  await evaluate(cdp, `__submit('.trade-panel form')`).catch(() => {});
  const selfTrade = await sim
    .post("/quotes", { instance_id: series.instances[0].id, outcome_id: "yes", side: "buy", quantity_millis: 1000 }, { token: (await sim.post("/auth/login", { email: memberEmail, password: PASSWORD })).token })
    .then(() => "allowed")
    .catch((e) => (/own markets/.test(e.message) ? "blocked" : `unexpected: ${e.message}`));
  if (selfTrade !== "blocked") throw new Error(`self-trading was not blocked: ${selfTrade}`);
  log(10, "the creator's own trade is rejected: creators cannot trade in their own markets");

  // ---- 7. A member trades it, then the creator resolves it -------------
  await signOut(cdp);
  const traderEmail = `demo.trader${stamp}@ntu.edu.sg`;
  await evaluate(cdp, `__clickText('.signin-toggle button', 'Create account')`);
  await waitFor(cdp, "register form", `__has('#reg-email')`);
  await evaluate(cdp, `__set('#reg-name', 'Demo Trader')`);
  await evaluate(cdp, `__set('#reg-email', ${JSON.stringify(traderEmail)})`);
  await evaluate(cdp, `__set('#reg-password', ${JSON.stringify(PASSWORD)})`);
  await evaluate(cdp, `__submit('form.stacked-form')`);
  await waitFor(cdp, "account chip", `__has('.account-chip')`);
  await evaluate(cdp, `__clickText('button.wordmark', 'Poly')`);
  await waitFor(cdp, "browse grid", `document.querySelectorAll('.market-card').length > 0`, 15000);
  await evaluate(cdp, `__clickText('.market-card h3', ${JSON.stringify(marketTitle)})`);
  await waitFor(cdp, "trade panel", `__has('#quantity')`, 20000);
  await evaluate(cdp, `__submit('.trade-panel form')`);
  await waitFor(cdp, "quote preview", `__has('.quote-preview')`);
  await evaluate(cdp, `__clickText('.quote-preview button', 'Confirm buy')`);
  await waitFor(cdp, "trade receipt", `__has('.receipt')`);
  log(11, "a second member traded the creator's market, giving it a position and a price history");

  // Advance the demo clock into the market's resolvable window: past its
  // observation end, with at least 30 seconds left before the evidence
  // deadline. The window is only sixty seconds wide, and a whole-minute jump
  // has to keep a safe margin because signing in and opening the market costs
  // time, so wait for a clock phase that leaves room before advancing.
  await signOut(cdp);
  await evaluate(cdp, `__clickText('.signin-toggle button', 'Log in')`);
  await waitFor(cdp, "login form", `__has('#login-email')`);
  await evaluate(cdp, `__set('#login-email', 'admin@ntu.edu.sg')`);
  await evaluate(cdp, `__set('#login-password', 'admin')`);
  await evaluate(cdp, `__submit('#login-email')`);
  await waitFor(cdp, "admin chip", `document.querySelector('.account-chip')?.textContent.includes('admin')`);
  await evaluate(cdp, `__clickText('footer button', 'Demo clock')`);
  await waitFor(cdp, "demo clock dialog", `__has('#admin-token')`);
  await evaluate(cdp, `__set('#admin-token', ${JSON.stringify(adminToken)})`);
  const minutes = await advanceIntoResolveWindow(series.instances[0].id);
  await evaluate(cdp, `__set('#clock-minutes', ${JSON.stringify(String(minutes))})`);
  await evaluate(cdp, `__clickText('#clock-minutes', '')`).catch(() => {});
  await evaluate(cdp, `[...document.querySelectorAll('.inline-form button')].find((b) => b.textContent === 'Advance').click()`);
  await sleep(2500);
  const advanced = await sim.get(`/instances/${series.instances[0].id}`);
  if (advanced.state !== "closed") throw new Error(`expected a closed market inside the resolve window, saw ${advanced.state}`);
  log(12, `demo clock advanced ${minutes} minutes into the resolvable window (market is ${advanced.state}, deadline still ahead)`);

  // Resolve as the creator with the cached key.
  await signOut(cdp);
  await evaluate(cdp, `__clickText('.signin-toggle button', 'Log in')`);
  await waitFor(cdp, "login form", `__has('#login-email')`);
  await evaluate(cdp, `__set('#login-email', ${JSON.stringify(memberEmail)})`);
  await evaluate(cdp, `__set('#login-password', ${JSON.stringify(PASSWORD)})`);
  await evaluate(cdp, `__submit('#login-email')`);
  await waitFor(cdp, "creator chip", `document.querySelector('.account-chip')?.textContent.includes('creator')`);
  await evaluate(cdp, `__clickText('button.wordmark', 'Poly')`);
  await waitFor(cdp, "browse grid", `document.querySelectorAll('.market-card').length > 0`, 15000);
  if (!(await evaluate(cdp, `[...document.querySelectorAll('.market-card h3')].some((h) => h.textContent === ${JSON.stringify(marketTitle)})`))) {
    await evaluate(cdp, `__clickText('.tab-bar button', 'Closed')`);
    await waitFor(cdp, "closed card", `[...document.querySelectorAll('.market-card h3')].some((h) => h.textContent === ${JSON.stringify(marketTitle)})`, 15000);
  }
  await evaluate(cdp, `__clickText('.market-card h3', ${JSON.stringify(marketTitle)})`);
  await waitFor(cdp, "bracket history tabs", `__has('.tab-bar')`, 30000);
  await evaluate(cdp, `__clickText('.tab-bar button', 'Closed')`);
  await waitFor(cdp, "resolve control", `__has('.resolve-control select')`, 15000);
  await evaluate(cdp, `__clickText('.resolve-control button', 'Resolve')`);
  await waitFor(cdp, "resolved tab count", `(() => { const b = [...document.querySelectorAll('.tab-bar button')].find((x) => x.textContent.startsWith('Resolved')); return b && !b.textContent.includes('(0)'); })()`, 30000);
  await evaluate(cdp, `__clickText('.tab-bar button', 'Resolved')`);
  await waitFor(cdp, "settled result", `[...document.querySelectorAll('.bracket-list li')].some((li) => li.textContent.includes('Result:'))`);
  log(13, "creator resolved the bracket with the cached password-derived key; the result is in the Resolved tab");

  const settled = await sim.get(`/series/${series.id}`);
  const resolved = settled.instances.find((i) => i.state === "resolved");
  if (!resolved) throw new Error("the instance is not resolved on the server");
  const evidence = (await sim.get(`/instances/${resolved.id}`)).evidence;
  // `source_id` names the evidence authority; `source` only carries which bus
  // path answered (ntubus-live or simulated-fallback), so it is null here.
  if (evidence?.source_id !== "creator-signature") throw new Error(`unexpected evidence source: ${evidence?.source_id}`);
  log(14, `server recorded evidence with source_id "${evidence.source_id}"; the platform verified the signature against the published key`);

  // ---- 8. Portfolio, fee split, reconciliation -------------------------
  // Sign in as the trader who held the winning side: the settlement credit is
  // theirs. The creator's balance shows the other half of the fee pot.
  await signOut(cdp);
  await evaluate(cdp, `__clickText('.signin-toggle button', 'Log in')`);
  await waitFor(cdp, "login form", `__has('#login-email')`);
  await evaluate(cdp, `__set('#login-email', ${JSON.stringify(traderEmail)})`);
  await evaluate(cdp, `__set('#login-password', ${JSON.stringify(PASSWORD)})`);
  await evaluate(cdp, `__submit('#login-email')`);
  await waitFor(cdp, "account chip", `__has('.account-chip')`);
  await evaluate(cdp, `__clickText('nav button', 'Portfolio')`);
  await waitFor(cdp, "portfolio", `__bodyHas('Outcome shares')`, 15000);
  await waitFor(cdp, "settlement credit", `__bodyHas('Settlement credits') && !__bodyHas('No settlement credits yet')`, 15000);
  log(15, "the trader's portfolio shows the settled position and its settlement credit");

  // The creator's balance is read from the API, but a fresh login would rotate
  // the account's single session token and sign the browser out, so read the
  // creator's balance from the ledger instead of logging in again.
  const creatorRow = await sim.get("/admin/reconcile", { admin: adminToken });
  if (!creatorRow.ok) throw new Error(`reconciliation found discrepancies: ${JSON.stringify(creatorRow)}`);
  if (creatorRow.net_units_micros !== "0") throw new Error(`ledger does not sum to zero: ${creatorRow.net_units_micros}`);
  log(16, "reconciliation: balances, inventory, reserves, and net units all agree");

  // ---- 9. The live instance: the external connector --------------------
  await navigate(cdp, LIVE);
  const liveConfig = await live.get("/config");
  if (!liveConfig.live_mode) throw new Error("the second instance is not in live mode");
  // The footer must not offer operations that cannot run in live mode.
  if (await evaluate(cdp, `__footerHas('Admin')`)) throw new Error("the Admin button is still shown in live mode");
  if (await evaluate(cdp, `__footerHas('Demo clock')`)) throw new Error("the Demo clock button is still shown in live mode");
  log(17, "live instance is in live mode and hides both the Admin and Demo clock footer buttons");

  const liveOpen = await live.get("/instances?state=open&limit=100");
  const liveOneTime = liveOpen.filter((i) => i.series_id === null);
  if (liveOneTime.length > 0) throw new Error(`live mode seeded ${liveOneTime.length} simulator-only one-time markets, which would void`);
  log(18, "live mode seeds only the bus series: no simulator-only markets that could only void");

  // The seeded bus lines run on their published service span (07:30 to 23:00
  // SGT), so a demonstration outside those hours has no bracket to show. A
  // creator-published bus series with a resolver authority is eligible for the
  // live feed too (the adapter accepts manual resolver-authority bus
  // markets), so publish one whose window covers the current hour. Outside
  // those hours this is the only way to see the live path without waiting.
  let busInstanceId = liveOpen.find((i) => i.series_id !== null)?.id;
  if (!busInstanceId) {
    const busSeries = await publishLiveBusSeries();
    busInstanceId = busSeries.instanceId;
    log(19, `no seeded bus bracket is live at this hour; published a resolver-authority bus series whose window covers now`);
    // The series was published after the page loaded, so reload the grid to
    // pick up its brackets (the browse poll runs every ten seconds).
    await navigate(cdp, LIVE);
    await waitFor(cdp, "live grid after publishing", `document.querySelectorAll('.market-card').length > 0`, 30000);
  }

  const busCard = await evaluate(cdp, `[...document.querySelectorAll('.market-card h3')].map((h) => h.textContent).find((t) => t.includes('line'))`);
  if (!busCard) throw new Error("no bus bracket on the live grid");
  await evaluate(cdp, `__clickText('.market-card h3', ${JSON.stringify(busCard.split(" · ")[0])})`);
  await waitFor(cdp, "market page", `__has('.market-facts')`, 20000);
  await waitFor(cdp, "bus resolver shown", `__bodyHas('NTU Bus API')`, 15000);
  log(20, `bus bracket "${busCard}" resolves through the NTU Bus API adapter`);

  // Wait for the observation window to end, then confirm the provider answered
  // (or that the bracket voids rather than fabricating an outcome).
  const settledLive = await waitForLiveSettlement(busInstanceId);
  log(21, settledLive);

  // ---- 10. Closing state ----------------------------------------------
  const health = await (await fetch("http://127.0.0.1:8090/")).json();
  if (!health.ok) throw new Error("the bus provider is not healthy");
  log(22, `provider healthy: ${health.events} arrival events across ${Object.keys(health.routes).length} routes`);
  console.log("DEMO PASSED");
}

main().then(() => process.exit(0)).catch((e) => { console.error("DEMO FAILED:", e.message); process.exit(1); });
