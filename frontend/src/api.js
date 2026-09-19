const BASE = import.meta.env.VITE_API_URL || "";
export const TOKEN_KEY = "polyntu.v2.token";
export const PENDING_KEY = "polyntu.v2.pending-trade";
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
