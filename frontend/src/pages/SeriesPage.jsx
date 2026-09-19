import { useEffect, useState } from "react";
import { api, categories, timestamp } from "../api";

const categoryOf = (rule) =>
  rule.kind === "election"
    ? "elections"
    : rule.kind === "count"
      ? rule.metric === "unique_attendance"
        ? "attendance"
        : "queue_crowd"
      : rule.kind;

export default function SeriesPage({ id, refresh, onError, onBack, onSelect }) {
  const [series, setSeries] = useState(null);
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
  const settled = series.instances.filter((i) => i.state !== "open");
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
    {settled.length > 0 && <section className="panel"><h2>Settled brackets</h2>
      <ul className="bracket-list">{settled.map((bracket) => <li key={bracket.id}>
        <span>Closed {timestamp(bracket.close_ms)} SGT</span>
        <span>{bracket.result?.kind === "winner" ? `Result: ${bracket.outcomes[bracket.result.outcome]?.label}` : "Voided"}</span>
        <button onClick={() => onSelect(bracket.id)}>View</button></li>)}
      </ul>
    </section>}
  </>;
}
