const BASE = import.meta.env.VITE_API_URL || "";
export const TOKEN_KEY = "polyntu.v2.token";
export const PENDING_KEY = "polyntu.v2.pending-trade";
export const SIGNING_KEY_KEY = "polyntu.v2.signing-key";
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

/// PBKDF2 iterations for the resolution key derivation; must match the
/// backend constant and the published contract (ADR 0007 amendment).
export const RESOLUTION_KEY_ITERATIONS = 600000;
// The fixed RFC 5958 PKCS8 prefix of an ed25519 seed, identical to what
// exportKey("pkcs8") produces for a generated key.
const ED25519_PKCS8_PREFIX = [0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04, 0x20];

/// Derive the creator's resolution keypair from the account password (ADR
/// 0007 amendment): PBKDF2-HMAC-SHA256 over the password with the
/// email-bound salt yields the ed25519 seed, so holding the password is
/// holding the key and any browser where the creator signs in can resolve.
/// The password and seed never leave the browser; only the public key is
/// published.
export async function deriveResolutionKeyPair(email, password) {
  const enc = new TextEncoder();
  const material = await crypto.subtle.importKey("raw", enc.encode(password), "PBKDF2", false, ["deriveBits"]);
  const seed = await crypto.subtle.deriveBits(
    { name: "PBKDF2", salt: enc.encode(`polyntu.resolution.v1:${email}`), iterations: RESOLUTION_KEY_ITERATIONS, hash: "SHA-256" },
    material,
    256,
  );
  const pkcs8 = new Uint8Array([...ED25519_PKCS8_PREFIX, ...new Uint8Array(seed)]);
  const pair = await crypto.subtle.importKey("pkcs8", pkcs8, { name: "Ed25519" }, true, ["sign"]);
  const jwk = await crypto.subtle.exportKey("jwk", pair);
  let publicKey = jwk.x.replace(/-/g, "+").replace(/_/g, "/");
  while (publicKey.length % 4) publicKey += "=";
  return { public_key: publicKey, private_key: toBase64(pkcs8) };
}

export async function signResolution(privateKeyB64, instanceId, outcomeId, nonce) {
  const bytes = Uint8Array.from(atob(privateKeyB64), (char) => char.charCodeAt(0));
  const key = await crypto.subtle.importKey("pkcs8", bytes, { name: "Ed25519" }, false, ["sign"]);
  const message = new TextEncoder().encode(`polyntu.resolution.v1:${instanceId}:${outcomeId}:${nonce}`);
  return toBase64(await crypto.subtle.sign({ name: "Ed25519" }, key, message));
}

export function loadSigningKey() {
  try { return JSON.parse(localStorage.getItem(SIGNING_KEY_KEY) || "null"); } catch { return null; }
}
export function storeSigningKey(email, keys) {
  localStorage.setItem(SIGNING_KEY_KEY, JSON.stringify({ email, ...keys }));
}
export function signingKey(account) {
  const entry = loadSigningKey();
  return account?.email && entry?.email === account.email ? entry : null;
}
