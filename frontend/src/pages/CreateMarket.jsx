import { useState } from "react";
import { api, categories, generateResolutionKeyPair, storeSeriesKey } from "../api";

const localToMs = (value) => value ? new Date(value).getTime() : null;
const minutesOfDay = (value) => {
  const [hours, minutes] = value.split(":").map(Number);
  return hours * 60 + minutes;
};

export default function CreateMarket({ onCreated, onError, onBack }) {
  const [title, setTitle] = useState("");
  const [criterion, setCriterion] = useState("");
  const [category, setCategory] = useState("weather");
  const [stationId, setStationId] = useState("");
  const [threshold, setThreshold] = useState("");
  const [routeId, setRouteId] = useState("");
  const [direction, setDirection] = useState("");
  const [stopId, setStopId] = useState("");
  const [candidates, setCandidates] = useState("");
  const [metric, setMetric] = useState("queue_length");
  const [locationId, setLocationId] = useState("");
  const [sourceId, setSourceId] = useState("");
  const [liquidity, setLiquidity] = useState("100");
  const [feeCharged, setFeeCharged] = useState(true);
  const [scheduleKind, setScheduleKind] = useState("once");
  const [resolutionKind, setResolutionKind] = useState("admin");
  const [resolverEndpoint, setResolverEndpoint] = useState("");
  const [keyPair, setKeyPair] = useState(null);
  const [closeAt, setCloseAt] = useState("");
  const [observationMinutes, setObservationMinutes] = useState("10");
  const [intervalMinutes, setIntervalMinutes] = useState("2");
  const [windowStart, setWindowStart] = useState("06:00");
  const [windowEnd, setWindowEnd] = useState("23:59");
  const [concurrency, setConcurrency] = useState("5");
  const [endAt, setEndAt] = useState("");
  const [busy, setBusy] = useState(false);
  const chooseResolution = async (kind) => {
    setResolutionKind(kind);
    if (kind === "creator" && !keyPair) {
      try { setKeyPair(await generateResolutionKeyPair()); }
      catch { onError("This browser cannot generate ed25519 keys"); setResolutionKind("admin"); }
    }
  };
  const submit = async (event) => {
    event.preventDefault(); setBusy(true); onError("");
    try {
      const rule = category === "weather" ? { kind: "weather", station_id: stationId.trim(), threshold_milli_mm: Number(threshold) }
        : category === "bus" ? { kind: "bus", route_id: routeId.trim(), direction: direction.trim(), stop_id: stopId.trim() }
        : category === "elections" ? { kind: "election", candidates: candidates.split(",").map((c) => c.trim()).filter(Boolean), is_fictional: true }
        : { kind: "count", metric, location_id: locationId.trim(), threshold: Number(threshold) };
      let schedule;
      if (scheduleKind === "once") {
        const closeMs = localToMs(closeAt);
        const observationMs = closeMs + Number(observationMinutes) * 60000;
        schedule = { kind: "once", close_ms: closeMs, observation_start_ms: closeMs, observation_end_ms: observationMs, finalize_after_ms: observationMs + 1000, evidence_deadline_ms: observationMs + 60000 };
      } else {
        schedule = { kind: "recurring", interval_ms: Number(intervalMinutes) * 60000, active_start_minute: minutesOfDay(windowStart), active_end_minute: minutesOfDay(windowEnd), max_concurrency: Number(concurrency), end_ms: localToMs(endAt) };
      }
      const resolution = resolutionKind === "creator"
        ? { kind: "creator", public_key: keyPair?.public_key }
        : resolutionKind === "resolver"
          ? { kind: "resolver", endpoint: resolverEndpoint.trim() }
          : undefined;
      const created = await api.createSeries({ title: title.trim(), resolution_criterion: criterion.trim(), rule, source_id: sourceId.trim(), liquidity_units: Number(liquidity), fee_charged: feeCharged, resolution, schedule });
      if (resolutionKind === "creator" && keyPair) storeSeriesKey(created.id, keyPair);
      onCreated(created.id);
    } catch (e) { onError(e.message); } finally { setBusy(false); }
  };
  return <>
    <button className="link-button" onClick={onBack}>← All markets</button>
    <div className="market-heading"><h1>Publish a market</h1><p className="muted">Definitions are immutable once published. Trading fees split 50/50 with you; leave the fee off for welfare markets.</p></div>
    <form onSubmit={submit} className="create-form panel">
      <label htmlFor="market-title">Title</label>
      <input id="market-title" value={title} minLength={5} maxLength={240} required onChange={(e) => setTitle(e.target.value)} placeholder="Short, specific, time-bound" />
      <label htmlFor="market-criterion">Resolution criterion</label>
      <textarea id="market-criterion" value={criterion} minLength={30} maxLength={4000} required rows={4} onChange={(e) => setCriterion(e.target.value)} placeholder="Exactly what evidence resolves each outcome, in at least 30 characters" />
      <label htmlFor="market-category">Category</label>
      <select id="market-category" value={category} onChange={(e) => setCategory(e.target.value)}>
        {Object.entries(categories).map(([id, name]) => <option key={id} value={id}>{name}</option>)}
      </select>
      {category === "weather" && <>
        <label htmlFor="market-station">Weather station identifier</label>
        <input id="market-station" value={stationId} maxLength={120} required onChange={(e) => setStationId(e.target.value)} />
        <label htmlFor="market-threshold">Rainfall threshold (milli-mm)</label>
        <input id="market-threshold" type="number" min={1} max={1000000} value={threshold} required onChange={(e) => setThreshold(e.target.value)} />
      </>}
      {category === "bus" && <>
        <label htmlFor="market-route">Bus route</label>
        <input id="market-route" value={routeId} maxLength={120} required onChange={(e) => setRouteId(e.target.value)} />
        <label htmlFor="market-direction">Direction</label>
        <input id="market-direction" value={direction} maxLength={120} required onChange={(e) => setDirection(e.target.value)} />
        <label htmlFor="market-stop">Stop</label>
        <input id="market-stop" value={stopId} maxLength={120} required onChange={(e) => setStopId(e.target.value)} />
      </>}
      {category === "elections" && <>
        <label htmlFor="market-candidates">Candidates (comma separated, 2 to 8, fictional only)</label>
        <input id="market-candidates" value={candidates} required onChange={(e) => setCandidates(e.target.value)} placeholder="Candidate A, Candidate B" />
      </>}
      {(category === "queue_crowd" || category === "attendance") && <>
        <label htmlFor="market-metric">Metric</label>
        <select id="market-metric" value={metric} onChange={(e) => setMetric(e.target.value)}>
          <option value="queue_length">Queue length</option>
          <option value="crowd_occupancy">Crowd occupancy</option>
          <option value="unique_attendance">Unique attendance</option>
        </select>
        <label htmlFor="market-location">Location identifier</label>
        <input id="market-location" value={locationId} maxLength={120} required onChange={(e) => setLocationId(e.target.value)} />
        <label htmlFor="market-count-threshold">Count threshold</label>
        <input id="market-count-threshold" type="number" min={1} max={1000000} value={threshold} required onChange={(e) => setThreshold(e.target.value)} />
      </>}
      <label htmlFor="market-source">Evidence source identifier</label>
      <input id="market-source" value={sourceId} maxLength={120} required onChange={(e) => setSourceId(e.target.value)} placeholder="The source your criterion names" />
      <div className="form-row">
        <div><label htmlFor="market-liquidity">Liquidity (units, 10 to 100000)</label>
          <input id="market-liquidity" type="number" min={10} max={100000} value={liquidity} required onChange={(e) => setLiquidity(e.target.value)} /></div>
        <div><label htmlFor="market-fee">Charge the 25 bps trading fee?</label>
          <select id="market-fee" value={feeCharged ? "yes" : "no"} onChange={(e) => setFeeCharged(e.target.value === "yes")}>
            <option value="yes">Yes, half funds me</option>
            <option value="no">No, a welfare market</option>
          </select></div>
      </div>
      <label htmlFor="market-resolution">Resolution authority</label>
      <select id="market-resolution" value={resolutionKind} onChange={(e) => chooseResolution(e.target.value)}>
        <option value="admin">Platform administrator records evidence</option>
        <option value="creator">I sign each resolution with a key from this browser</option>
        <option value="resolver">An external API answers automatically</option>
      </select>
      {resolutionKind === "creator" && <p className="muted small">{keyPair ? "An ed25519 keypair was generated in this browser. The private key never leaves it and is saved locally for this series; losing it voids the market at its deadline." : "Generating the browser keypair…"}</p>}
      {resolutionKind === "resolver" && <>
        <label htmlFor="market-resolver-endpoint">Resolver endpoint (https URL)</label>
        <input id="market-resolver-endpoint" value={resolverEndpoint} maxLength={500} required onChange={(e) => setResolverEndpoint(e.target.value)} placeholder="https://example.com/polyntu-resolver" />
        <p className="muted small">PolyNTU posts the instance identifier, bracket window, and rule; your endpoint must answer with one published outcome identifier or pending.</p>
      </>}
      <label htmlFor="market-schedule">Schedule</label>
      <select id="market-schedule" value={scheduleKind} onChange={(e) => setScheduleKind(e.target.value)}>
        <option value="once">One-time market</option>
        <option value="recurring">Recurring or perpetual series</option>
      </select>
      {scheduleKind === "once" && <div className="form-row">
        <div><label htmlFor="market-close">Trading closes at</label>
          <input id="market-close" type="datetime-local" value={closeAt} required onChange={(e) => setCloseAt(e.target.value)} /></div>
        <div><label htmlFor="market-observation">Observation window (minutes)</label>
          <input id="market-observation" type="number" min={1} max={1440} value={observationMinutes} required onChange={(e) => setObservationMinutes(e.target.value)} /></div>
      </div>}
      {scheduleKind === "recurring" && <>
        <div className="form-row">
          <div><label htmlFor="market-interval">Bracket interval (minutes)</label>
            <input id="market-interval" type="number" min={1} max={1440} value={intervalMinutes} required onChange={(e) => setIntervalMinutes(e.target.value)} /></div>
          <div><label htmlFor="market-concurrency">Live brackets (1 to 50)</label>
            <input id="market-concurrency" type="number" min={1} max={50} value={concurrency} required onChange={(e) => setConcurrency(e.target.value)} /></div>
        </div>
        <div className="form-row">
          <div><label htmlFor="market-window-start">Operating window start</label>
            <input id="market-window-start" type="time" value={windowStart} required onChange={(e) => setWindowStart(e.target.value)} /></div>
          <div><label htmlFor="market-window-end">Operating window end</label>
            <input id="market-window-end" type="time" value={windowEnd} required onChange={(e) => setWindowEnd(e.target.value)} /></div>
          <div><label htmlFor="market-end">End date (optional)</label>
            <input id="market-end" type="datetime-local" value={endAt} onChange={(e) => setEndAt(e.target.value)} /></div>
        </div>
        <p className="muted small">Brackets spawn on the interval grid, only inside the operating window, and never after the end date. Leave the end date empty for a perpetual series.</p>
      </>}
      <button className="primary" disabled={busy}>{scheduleKind === "once" ? "Publish market" : "Publish series"}</button>
    </form>
  </>;
}
