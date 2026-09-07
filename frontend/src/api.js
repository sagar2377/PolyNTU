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
  listRoutes: () => request("/routes"),
  getSchedule: (routeId) => request(`/routes/${routeId}/schedule`),
  getQuote: (routeId, scheduledMinuteOfDay, thresholds) =>
    request(
      `/routes/${routeId}/quote?scheduled_minute_of_day=${scheduledMinuteOfDay}&thresholds=${thresholds.join(",")}`
    ),
  createContract: (routeId, contractType, thresholdMinutes, scheduledMinuteOfDay) =>
    request("/contracts", {
      method: "POST",
      body: JSON.stringify({
        route_id: routeId,
        contract_type: contractType,
        threshold_minutes: thresholdMinutes,
        scheduled_minute_of_day: scheduledMinuteOfDay,
      }),
    }),
  listContracts: (routeId) => request(`/routes/${routeId}/contracts`),
  advanceClock: (minutes) =>
    request("/clock/advance", { method: "POST", body: JSON.stringify({ minutes }) }),
};

export function formatMinuteOfDay(minute) {
  const h = Math.floor(minute / 60) % 24;
  const m = minute % 60;
  const ampm = h < 12 ? "AM" : "PM";
  const h12 = h % 12 === 0 ? 12 : h % 12;
  return `${h12}:${String(m).padStart(2, "0")} ${ampm}`;
}
