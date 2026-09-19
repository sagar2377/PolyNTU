import { useEffect, useState } from "react";
import { api, categories, seriesKey, signResolution, timestamp } from "../api";

const categoryOf = (rule) =>
  rule.kind === "election"
    ? "elections"
    : rule.kind === "count"
      ? rule.metric === "unique_attendance"
        ? "attendance"
        : "queue_crowd"
      : rule.kind;

export default function SeriesPage({ id, account, refresh, onError, onBack, onSelect }) {
  const [series, setSeries] = useState(null);
  const [busy, setBusy] = useState(false);
  const [choices, setChoices] = useState({});
  useEffect(() => {
    let cancelled = false;
    const load = () => api.series(id).then((value) => { if (!cancelled) setSeries(value); }).catch((e) => { if (!cancelled) onError(e.message); });
    load(); const timer = setInterval(load, 5000);
    return () => { cancelled = true; clearInterval(timer); };
  }, [id, refresh, onError]);
  if (!series) return <p role="status">Loading series…</p>;
  const recurring = series.schedule.kind === "recurring";
  const intervalMinutes = Math.round((series.schedule.interval_ms || 0) / 60000);
  const windowLabel = (minutes) => `${String(Math.floor(minutes / 60)).padStart(2, "0")}:${String(minutes % 60).padStart(2, "0")}`;
  const live = series.instances.filter((i) => i.state === "open");
  const awaiting = series.instances.filter((i) => i.state === "closed" || i.state === "resolving");
  const settled = series.instances.filter((i) => i.state === "resolved" || i.state === "voided");
  const isCreator = account && series.creator_account_id === account.id;
  const canSign = isCreator && series.resolution?.authority === "creator";
  const signingKey = canSign ? seriesKey(series.id) : null;
  const resolve = async (bracket, outcomeId) => {
    setBusy(true); onError("");
    try {
      const nonce = crypto.randomUUID();
      const signature = await signResolution(signingKey.private_key, bracket.id, outcomeId, nonce);
      await api.resolveMarket(bracket.id, { outcome_id: outcomeId, nonce, signature });
      const fresh = await api.series(id);
      setSeries(fresh);
    } catch (e) { onError(e.message); } finally { setBusy(false); }
  };
  return <>
    <button className="link-button" onClick={onBack}>← All markets</button>
    <div className="market-heading"><span className={`category category-${categoryOf(series.rule)}`}>{categories[categoryOf(series.rule)]}</span><h1>{series.title}</h1>
      <p className="muted">{recurring ? `Rolling series · a new bracket every ${intervalMinutes} minute${intervalMinutes === 1 ? "" : "s"} · ${series.schedule.max_concurrency} live at once` : "One-time market"} · Times in Singapore (SGT)</p></div>
    <section className="panel"><h2>How this series works</h2>
      <dl className="market-facts">
        {recurring && <div><dt>Bracket interval</dt><dd>Every {intervalMinutes} minute{intervalMinutes === 1 ? "" : "s"}</dd></div>}
        {recurring && <div><dt>Operating window</dt><dd>{windowLabel(series.schedule.active_start_minute)} to {windowLabel(series.schedule.active_end_minute)} daily</dd></div>}
        {recurring && <div><dt>Live horizon</dt><dd>{series.schedule.max_concurrency} brackets, about {series.schedule.max_concurrency * intervalMinutes} minutes ahead</dd></div>}
        {recurring && <div><dt>Runs until</dt><dd>{series.schedule.end_ms ? timestamp(series.schedule.end_ms) : "Perpetual"}</dd></div>}
        <div><dt>Trading fee</dt><dd>{series.fee_charged ? "25 bps, half funds the creator" : "None, a welfare market"}</dd></div>
        <div><dt>Resolved by</dt><dd>{series.resolution?.authority === "creator" ? "The creator's signed statement" : series.resolution?.authority === "resolver" ? `An external resolver (${series.resolution.endpoint})` : "Platform administrator evidence"}</dd></div>
        <div><dt>Liquidity parameter</dt><dd>{series.liquidity_units} units per bracket</dd></div>
        <div><dt>State</dt><dd>{series.state === "active" ? "Active" : "Ended"}</dd></div>
      </dl>
      <p>{series.resolution_criterion}</p>
    </section>
    <section className="panel"><h2>Live brackets</h2>
      {live.length === 0 ? <p className="muted">No live brackets right now. {recurring ? "The next one appears at the start of the next operating window slot." : ""}</p> : <ul className="bracket-list">
        {live.map((bracket) => <li key={bracket.id}><span>Closes {timestamp(bracket.close_ms)} SGT</span>
          <span>{bracket.outcomes.map((o) => `${o.label} ${(o.probability * 100).toFixed(1)}%`).join(" · ")}</span>
          <button onClick={() => onSelect(bracket.id)}>Trade</button></li>)}
      </ul>}
    </section>
    {awaiting.length > 0 && <section className="panel"><h2>Awaiting resolution</h2>
      {canSign && <p className="muted small">{signingKey ? "Pick the outcome for each closed bracket; this browser signs the statement with the series key." : "This browser does not hold the signing key for this series."}</p>}
      <ul className="bracket-list">{awaiting.map((bracket) => <li key={bracket.id}>
        <span>Closed {timestamp(bracket.close_ms)} SGT</span>
        <span>{bracket.outcomes.map((o) => `${o.label} ${(o.probability * 100).toFixed(1)}%`).join(" · ")}</span>
        {canSign && signingKey
          ? <span className="resolve-control"><label className="sr-only" htmlFor={`resolve-${bracket.id}`}>Outcome</label>
              <select id={`resolve-${bracket.id}`} value={choices[bracket.id] ?? bracket.outcomes[0].id} onChange={(e) => setChoices({ ...choices, [bracket.id]: e.target.value })}>
                {bracket.outcomes.map((o) => <option key={o.id} value={o.id}>{o.label}</option>)}
              </select>
              <button disabled={busy} onClick={() => resolve(bracket, choices[bracket.id] ?? bracket.outcomes[0].id)}>Resolve</button></span>
          : <span className="muted small">{series.resolution?.authority === "resolver" ? "Asking the external resolver" : "Awaiting the resolution authority"}</span>}
      </li>)}
      </ul>
    </section>}
    {settled.length > 0 && <section className="panel"><h2>Settled brackets</h2>
      <ul className="bracket-list">{settled.map((bracket) => <li key={bracket.id}>
        <span>Closed {timestamp(bracket.close_ms)} SGT</span>
        <span>{bracket.result?.kind === "winner" ? `Result: ${bracket.outcomes[bracket.result.outcome]?.label}` : "Voided"}</span>
        <button onClick={() => onSelect(bracket.id)}>View</button></li>)}
      </ul>
    </section>}
  </>;
}
