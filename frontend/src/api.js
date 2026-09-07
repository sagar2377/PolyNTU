const BASE_URL = import.meta.env.VITE_API_URL || "http://localhost:8000";

async function request(path, options) {
  const res = await fetch(`${BASE_URL}${path}`, {
    headers: { "Content-Type": "application/json" },
    ...options,
  });
  if (!res.ok) {
    const body = await res.text();
    throw new Error(`${res.status} ${res.statusText}: ${body}`);
  }
  return res.json();
}

export const api = {
  listMarkets: () => request("/markets"),
  getInstances: (marketId) => request(`/markets/${marketId}/instances`),
  getQuote: (marketId, instanceId, thresholds) =>
    request(
      `/markets/${marketId}/quote?instance_id=${encodeURIComponent(instanceId)}&thresholds=${thresholds.join(",")}`
    ),
  createContract: (marketId, instanceId, contractType, threshold) =>
    request("/contracts", {
      method: "POST",
      body: JSON.stringify({
        market_id: marketId,
        instance_id: instanceId,
        contract_type: contractType,
        threshold: threshold,
      }),
    }),
  listContracts: (marketId) => request(`/markets/${marketId}/contracts`),
  getPortfolio: () => request("/portfolio"),
  advanceClock: (minutes) =>
    request("/clock/advance", { method: "POST", body: JSON.stringify({ minutes }) }),
  createMarket: (payload) =>
    request("/admin/markets", { method: "POST", body: JSON.stringify(payload) }),
};

export function formatMinutes(totalMinutes) {
  const days = Math.floor(totalMinutes / (24 * 60));
  const hours = Math.floor((totalMinutes % (24 * 60)) / 60);
  const mins = Math.round(totalMinutes % 60);
  if (days > 0) return `${days}d ${hours}h out`;
  if (hours > 0) return `${hours}h ${mins}m out`;
  return `${mins} min out`;
}

export function formatMinuteOfDay(minute) {
  const h = Math.floor(minute / 60) % 24;
  const m = Math.round(minute % 60);
  const ampm = h < 12 ? "AM" : "PM";
  const h12 = h % 12 === 0 ? 12 : h % 12;
  return `${h12}:${String(m).padStart(2, "0")} ${ampm}`;
}
