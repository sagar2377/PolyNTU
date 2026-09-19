const BASE = import.meta.env.VITE_API_URL || "";
export const TOKEN_KEY = "polyntu.v2.token";
export const PENDING_KEY = "polyntu.v2.pending-trade";
export const SERIES_KEYS_KEY = "polyntu.v2.series-keys";
export class ApiError extends Error {
  constructor(message, status = 0) { super(message); this.status = status; }
}
async function request(path, { body, token, adminToken, key, ...options } = {}) {
  const headers = { "Content-Type": "application/json" };
  const session = token ?? localStorage.getItem(TOKEN_KEY);
  if (session) headers.Authorization = `Bearer ${session}`;
  if (adminToken) headers["X-Admin-Token"] = adminToken;
  if (key) headers["Idempotency-Key"] = key;
  let response;
  try { response = await fetch(`${BASE}/api/v2${path}`, { ...options, headers, body: body === undefined ? undefined : JSON.stringify(body) }); }
  catch { throw new ApiError("Connection interrupted. Retry a submitted trade to retrieve its receipt."); }
  let text;
  try { text = await response.text(); }
  catch { throw new ApiError("Response interrupted. Retry to retrieve the original trade receipt."); }
  let result;
  try { result = JSON.parse(text); } catch {
    if (response.ok) throw new ApiError("The response could not be read. Retry to retrieve the original trade receipt.");
    result = {};
  }
  if (!response.ok) throw new ApiError(result.error?.message || text || `Request failed (${response.status})`, response.status);
  return result;
}
export const api = {
  config: () => request("/config"),
  me: (token) => request("/me", { token }),
  createAccount: (display_name) => request("/auth/demo", { method: "POST", body: { display_name } }),
  register: (body) => request("/auth/register", { method: "POST", body }),
  login: (body) => request("/auth/login", { method: "POST", body }),
  requestVerification: () => request("/verification-requests", { method: "POST" }),
  verificationRequest: () => request("/verification-requests"),
  instances: (offset = 0) => request(`/instances?limit=100&offset=${offset}`),
  instance: (id) => request(`/instances/${encodeURIComponent(id)}`),
  history: (id, bucketMs) => request(`/instances/${encodeURIComponent(id)}/history?bucket_ms=${bucketMs}`),
  seriesList: () => request("/series"),
  series: (id) => request(`/series/${encodeURIComponent(id)}`),
  createSeries: (body) => request("/series", { method: "POST", body }),
  resolveMarket: (instanceId, body) => request(`/instances/${encodeURIComponent(instanceId)}/resolution`, { method: "POST", body }),
  quote: (body) => request("/quotes", { method: "POST", body }),
  trade: (body, key) => request("/trades", { method: "POST", body, key }),
  portfolio: (offset = 0) => request(`/me/portfolio?limit=100&offset=${offset}`),
  trades: (offset = 0) => request(`/me/trades?limit=100&offset=${offset}`),
  advance: (minutes, adminToken) => request("/admin/clock/advance", { method: "POST", body: { minutes }, adminToken }),
  eventsUrl: (id) => `${BASE}/api/v2/instances/${encodeURIComponent(id)}/events`,
};
export const categories = { weather: "Weather", bus: "Bus timings", elections: "Elections", queue_crowd: "Queue & crowd", attendance: "Event & lecture attendance" };
export function units(micros) {
  const amount = BigInt(micros ?? 0);
  const sign = amount < 0n ? "-" : "";
  const magnitude = amount < 0n ? -amount : amount;
  const whole = (magnitude / 1000000n).toLocaleString("en-SG");
  const fraction = (magnitude % 1000000n).toString().padStart(6, "0").replace(/0+$/, "");
  return `${sign}${whole}${fraction ? `.${fraction}` : ""}`;
}
export function quantityMillis(value) {
  if (!/^\d+(\.\d{1,3})?$/.test(value)) return null;
  const [whole, fraction = ""] = value.split(".");
  const quantity = Number(whole) * 1000 + Number(fraction.padEnd(3, "0"));
  return Number.isSafeInteger(quantity) && quantity >= 1 && quantity <= 100000 ? quantity : null;
}
export function timestamp(ms) {
  return new Intl.DateTimeFormat("en-SG", { timeZone: "Asia/Singapore", year: "numeric", day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" }).format(new Date(ms));
}
export function pendingTrade() {
  try { return JSON.parse(localStorage.getItem(PENDING_KEY) || "null"); } catch { return null; }
}

function toBase64(buffer) {
  let binary = "";
  for (const byte of new Uint8Array(buffer)) binary += String.fromCharCode(byte);
  return btoa(binary);
}

/// Generate an ed25519 keypair for creator resolution (ADR 0007). The
/// private key never leaves the browser; only the public key is published.
export async function generateResolutionKeyPair() {
  const pair = await crypto.subtle.generateKey({ name: "Ed25519" }, true, ["sign", "verify"]);
  return {
    public_key: toBase64(await crypto.subtle.exportKey("raw", pair.publicKey)),
    private_key: toBase64(await crypto.subtle.exportKey("pkcs8", pair.privateKey)),
  };
}

export async function signResolution(privateKeyB64, instanceId, outcomeId, nonce) {
  const bytes = Uint8Array.from(atob(privateKeyB64), (char) => char.charCodeAt(0));
  const key = await crypto.subtle.importKey("pkcs8", bytes, { name: "Ed25519" }, false, ["sign"]);
  const message = new TextEncoder().encode(`polyntu.resolution.v1:${instanceId}:${outcomeId}:${nonce}`);
  return toBase64(await crypto.subtle.sign({ name: "Ed25519" }, key, message));
}

export function loadSeriesKeys() {
  try { return JSON.parse(localStorage.getItem(SERIES_KEYS_KEY) || "{}"); } catch { return {}; }
}
export function storeSeriesKey(seriesId, keys) {
  const all = loadSeriesKeys();
  all[seriesId] = keys;
  localStorage.setItem(SERIES_KEYS_KEY, JSON.stringify(all));
}
export function seriesKey(seriesId) {
  return loadSeriesKeys()[seriesId] || null;
}
