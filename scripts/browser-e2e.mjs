// End-to-end browser test over CDP (Chrome DevTools Protocol) against the
// real app. Drives the actual React UI in headless Chrome: register, verify,
// publish a password-keyed signed market, trade, close, resolve with the
// cached key, then publish and resolve a second market through the password
// prompt after the cached key is cleared.
//
// Prerequisites: the app serving the built frontend (demo mode, so the seeded
// administrator and clock advance exist) at E2E_BASE_URL, and a Chrome with
// --remote-debugging-port on a dedicated --user-data-dir (Chrome 136+
// ignores the debug port on the default profile) reachable at E2E_CDP_URL.
// Locally: scripts/run-prod.ps1 plus
//   chrome --headless=new --remote-debugging-port=9223 \
//     --user-data-dir=.local/chrome-profile about:blank
const BASE = process.env.E2E_BASE_URL ?? "http://127.0.0.1:8000";
const API = `${BASE}/api/v2`;
const CDP = process.env.E2E_CDP_URL ?? "http://127.0.0.1:9223";
const CREATOR_PASSWORD = "correct e2e password";
const log = (step, detail) => console.log(`[${String(step).padStart(2, "0")}] ${detail}`);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function waitForServer() {
  for (let i = 0; i < 120; i++) {
    try { await fetch(`${BASE}/health`); return; } catch { await sleep(1500); }
  }
  throw new Error("app server never became ready");
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
window.__bodyHas = (text) => document.body.textContent.includes(text);`;

async function waitFor(cdp, description, expression, timeout = 20000) {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    if (await evaluate(cdp, expression)) return;
    await sleep(300);
  }
  throw new Error(`timeout waiting for ${description}`);
}

async function apiGet(path, token) {
  const response = await fetch(API + path, { headers: token ? { authorization: `Bearer ${token}` } : {} });
  const data = await response.json();
  if (!response.ok) throw new Error(`GET ${path} -> ${response.status}`);
  return data;
}
async function apiPost(path, body, token) {
  const response = await fetch(API + path, {
    method: "POST",
    headers: { "content-type": "application/json", ...(token ? { authorization: `Bearer ${token}` } : {}) },
    body: JSON.stringify(body),
  });
  const data = await response.json().catch(() => null);
  if (!response.ok) throw new Error(`POST ${path} -> ${response.status} ${JSON.stringify(data)}`);
  return data;
}

const pad = (n) => String(n).padStart(2, "0");
const localInputValue = (epochMs) => {
  const d = new Date(epochMs);
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
};

// Advance the demo clock to just past a market's observation end while
// staying inside its evidence deadline (observation end + 60s). Whole-minute
// granularity, computed from a fresh server time so elapsed wall clock does
// not push the landing point past the deadline.
async function advanceIntoResolveWindow(admin, closeEpochMs) {
  const now = (await apiGet("/config")).server_time_ms;
  const observationEnd = closeEpochMs + 5 * 60000;
  const minutes = Math.max(1, Math.ceil((observationEnd - now) / 60000));
  await apiPost("/admin/clock/advance", { minutes }, admin.token);
  return minutes;
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
  await evaluate(cdp, `__set('#market-source', 'e2e-source')`);
  await evaluate(cdp, `__set('#market-close', ${JSON.stringify(localInputValue(closeEpochMs))})`);
  // A 5-minute observation leaves a resolvable window between the finalize
  // point (close + 5min + 1s) and the evidence deadline (close + 6min); a
  // 9-minute advance from publication lands inside it.
  await evaluate(cdp, `__set('#market-observation', '5')`);
  await evaluate(cdp, `__set('#market-resolution', 'creator')`);
  if (password === undefined) {
    if (await evaluate(cdp, `__has('#market-key-password')`)) throw new Error("password field shown although the key was cached");
  } else {
    await waitFor(cdp, "password field at publish", `__has('#market-key-password')`, 5000);
    await evaluate(cdp, `__set('#market-key-password', ${JSON.stringify(password)})`);
  }
  await evaluate(cdp, `__submit('form.create-form')`);
  await waitFor(cdp, `series page for ${title}`, `__bodyHas(${JSON.stringify(title)}) && __has('.market-facts')`, 30000);
}

async function openSeries(cdp, title) {
  await evaluate(cdp, `__clickText('button.wordmark', 'Poly')`);
  await waitFor(cdp, `series chip for ${title}`, `[...document.querySelectorAll('.series-chip strong')].some((s) => s.textContent === ${JSON.stringify(title)})`);
  await evaluate(cdp, `__clickText('.series-chip', ${JSON.stringify(title)})`);
  await waitFor(cdp, `series facts for ${title}`, `__has('.market-facts')`);
}

async function main() {
  await waitForServer();
  const cdp = await connect();
  await evaluate(cdp, HELPERS);

  // Creator registers; the signing key is derived and cached at registration.
  await navigate(cdp, BASE);
  // Start from a guest state regardless of what earlier runs left behind.
  await evaluate(cdp, `localStorage.clear()`);
  await navigate(cdp, BASE);
  await register(cdp, "E2E Creator", creatorEmail, CREATOR_PASSWORD);
  log(1, `registered ${creatorEmail}`);
  await waitFor(cdp, "cached signing key", `localStorage.getItem('polyntu.v2.signing-key') !== null`, 15000);
  const cachedKey = JSON.parse(await evaluate(cdp, `localStorage.getItem('polyntu.v2.signing-key')`));
  if (cachedKey.email !== creatorEmail || !cachedKey.public_key) throw new Error("signing key cache is wrong");
  log(2, "signing key derived from the password and cached in this browser");

  // Request creator verification; the seeded administrator approves it.
  await waitFor(cdp, "verification panel", `__bodyHas('Become a market creator')`);
  await evaluate(cdp, `__clickText('button', 'Request creator verification')`);
  await waitFor(cdp, "pending review", `__bodyHas('pending administrator review')`);
  log(3, "creator verification requested");
  const admin = await apiPost("/auth/login", { email: "admin@ntu.edu.sg", password: "admin" });
  const requests = await apiGet("/admin/verification-requests?status=pending", admin.token);
  const mine = requests.find((r) => r.email === creatorEmail);
  if (!mine) throw new Error("verification request not visible to the administrator");
  await apiPost(`/admin/verification-requests/${mine.id}/decision`, { approve: true }, admin.token);
  log(4, "administrator approved the request");

  // Publish the first signed market; the cached key means no password prompt.
  const config = await apiGet("/config");
  const closeAt = config.server_time_ms + 3 * 60000;
  await waitFor(cdp, "creator nav button", `__bodyHas('Create market')`, 15000);
  await publishSignedMarket(cdp, titles[0], closeAt);
  log(5, `published "${titles[0]}" with the cached password-derived key (no prompt)`);
  const seriesList = await apiGet("/series");
  const series1 = await apiGet(`/series/${seriesList.find((s) => s.title === titles[0])?.id}`);
  if (!series1 || series1.resolution?.authority !== "creator") throw new Error("series not published with creator authority");
  if (series1.resolution.public_key !== cachedKey.public_key) throw new Error("published public key does not match the cached key");
  log(6, "published public key equals the key derived from the password");

  // A second account trades, exercising the trade panel and the price chart.
  await signOut(cdp);
  await register(cdp, "E2E Trader", traderEmail, "correct e2e password");
  log(7, `registered ${traderEmail}`);
  await openSeries(cdp, titles[0]);
  await waitFor(cdp, "live bracket trade button", `[...document.querySelectorAll('.bracket-list button')].some((b) => b.textContent === 'Trade')`);
  await evaluate(cdp, `__clickText('.bracket-list button', 'Trade')`);
  await waitFor(cdp, "trade panel", `__has('#quantity')`);
  await evaluate(cdp, `__submit('.trade-panel form')`);
  await waitFor(cdp, "quote preview", `__has('.quote-preview')`);
  await evaluate(cdp, `__clickText('.quote-preview button', 'Confirm buy')`);
  await waitFor(cdp, "trade receipt", `__has('.receipt')`);
  log(8, "trader bought shares through the quote and confirm flow");
  await waitFor(cdp, "price history chart", `document.querySelectorAll('.chart-container canvas').length > 0`, 15000);
  log(9, "price and volume history chart rendered");

  // Close the market by advancing the demo clock, then resolve as the creator.
  const advanced = await advanceIntoResolveWindow(admin, closeAt);
  log(10, `demo clock advanced ${advanced} minutes into the resolvable window (past the observation end, before the deadline)`);
  await signOut(cdp);
  await signIn(cdp, creatorEmail, CREATOR_PASSWORD);
  await openSeries(cdp, titles[0]);
  await waitFor(cdp, "awaiting resolution section", `__bodyHas('Awaiting resolution')`, 30000);
  if (!(await evaluate(cdp, `__has('.resolve-control select')`))) throw new Error("resolve control missing with a cached key");
  await evaluate(cdp, `__clickText('.resolve-control button', 'Resolve')`);
  await waitFor(cdp, "settled result", `[...document.querySelectorAll('.bracket-list li')].some((li) => li.textContent.includes('Result:'))`, 30000);
  log(11, "creator resolved the bracket with the cached key; result recorded");
  const series1After = await apiGet(`/series/${series1.id}`);
  const settled = series1After.instances.find((i) => i.state === "resolved");
  if (!settled) throw new Error("instance not resolved on the server");
  if (!series1After.day?.slots?.length) throw new Error("day view slots missing");
  await waitFor(cdp, "day probability chart", `document.querySelectorAll('.chart-container canvas').length > 0`, 15000);
  log(12, `settled on the server (${settled.result?.kind}); day view and chart render`);

  // Second market: clear the cached key to exercise both password prompts.
  await evaluate(cdp, `localStorage.removeItem('polyntu.v2.signing-key')`);
  await navigate(cdp, BASE);
  const config2 = await apiGet("/config");
  await waitFor(cdp, "creator nav button restored", `__bodyHas('Create market')`, 15000);
  await publishSignedMarket(cdp, titles[1], config2.server_time_ms + 3 * 60000, CREATOR_PASSWORD);
  log(13, "publish form asked for the password once the cache was cleared, and published with the derived key");
  const series2 = await apiGet(`/series/${(await apiGet("/series")).find((s) => s.title === titles[1])?.id}`);
  if (!series2 || series2.resolution?.authority !== "creator") throw new Error("second series not published with creator authority");

  // Clear the cache again: resolving must now go through the password prompt.
  await evaluate(cdp, `localStorage.removeItem('polyntu.v2.signing-key')`);
  await navigate(cdp, BASE);
  await advanceIntoResolveWindow(admin, config2.server_time_ms + 3 * 60000);
  log(14, "clock advanced again; cached key cleared");
  await openSeries(cdp, titles[1]);
  await waitFor(cdp, "awaiting resolution section", `__bodyHas('Awaiting resolution')`, 30000);
  if (!(await evaluate(cdp, `__has('#series-key-password')`))) throw new Error("password prompt missing on the series page");
  if (await evaluate(cdp, `__has('.resolve-control select')`)) throw new Error("resolve control shown without a matching key");
  await evaluate(cdp, `__set('#series-key-password', ${JSON.stringify(CREATOR_PASSWORD)})`);
  await evaluate(cdp, `__clickText('.resolve-control button', 'Derive signing key')`);
  await waitFor(cdp, "resolve control after derivation", `__has('.resolve-control select')`, 20000);
  await evaluate(cdp, `__clickText('.resolve-control button', 'Resolve')`);
  await waitFor(cdp, "settled result", `[...document.querySelectorAll('.bracket-list li')].some((li) => li.textContent.includes('Result:'))`, 30000);
  const series2After = await apiGet(`/series/${series2.id}`);
  if (!series2After.instances.some((i) => i.state === "resolved")) throw new Error("second instance not resolved on the server");
  log(15, "resolved the second market through the password prompt after losing the cached key");
  console.log("E2E PASSED");
}

// The CDP websocket keeps the Node event loop alive after a pass, so exit
// explicitly in both directions.
main().then(() => process.exit(0)).catch((e) => { console.error("E2E FAILED:", e.message); process.exit(1); });
