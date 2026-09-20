import { useEffect, useState } from "react";
import { api, categories, timestamp } from "../api";
export default function MarketBrowse({ refresh, onSelect, onError }) {
  const [instances, setInstances] = useState([]);
  const [category, setCategory] = useState("all");
  const [loading, setLoading] = useState(true);
  const [offset, setOffset] = useState(0);
  const [historyTab, setHistoryTab] = useState(null);
  useEffect(() => {
    let cancelled = false;
    const load = () => api.instances(offset).then((rows) => { if (!cancelled) { setInstances(rows); setLoading(false); } }).catch((e) => { if (!cancelled) { onError(e.message); setLoading(false); } });
    load(); const timer = setInterval(load, 10000);
    return () => { cancelled = true; clearInterval(timer); };
  }, [refresh, onError, offset]);
  const visible = instances.filter((i) => category === "all" || i.category === category);
  // Settled markets are collapsed into hidden-by-default tabs (UC-21): the
  // grid lists live markets only, and history opens on demand.
  const live = visible.filter((i) => i.state === "open");
  const closed = visible.filter((i) => i.state === "closed" || i.state === "resolving");
  const voided = visible.filter((i) => i.state === "voided");
  const resolved = visible.filter((i) => i.state === "resolved");
  const card = (instance) => <button className="market-card" key={instance.id} onClick={() => onSelect(instance.id)}>
    <div className="card-meta"><span className={`category category-${instance.category}`}>{categories[instance.category]}</span><span className={`status ${instance.tradable ? "open" : ""}`}>{instance.tradable ? "Open" : instance.state}</span></div>
    <h3>{instance.title}</h3><div className="outcome-preview">{instance.outcomes.slice(0, 3).map((outcome) => <div key={outcome.id}><span>{outcome.label}</span><strong>{(outcome.probability * 100).toFixed(1)}%</strong><span className="probability-track"><span style={{ width: `${outcome.probability * 100}%` }} /></span></div>)}</div>
    <div className="card-footer"><span>{instance.data_mode === "simulated" ? "Simulated observations" : "Published evidence source"}</span><span>Closes {timestamp(instance.close_ms)} SGT</span></div>
  </button>;
  const grid = (rows) => rows.length === 0 ? <div className="empty-state">No markets in this category on this page.</div> : <div className="market-grid">{rows.map(card)}</div>;
  return <>
    <section className="hero"><p className="eyebrow">THE CAMPUS FORECAST</p><h1>What happens next?</h1><p>Weather, bus arrivals, campus crowds and more.<br />Explore the outcomes. Share what you know.</p></section>
    <div className="filters" aria-label="Market categories">{[["all", "All markets"], ...Object.entries(categories)].map(([id, name]) => <button key={id} aria-pressed={category === id} className={category === id ? "selected" : ""} onClick={() => setCategory(id)}>{name}</button>)}</div>
    <div className="section-heading"><h2>{category === "all" ? "Campus markets" : categories[category]}</h2><span className="muted">{live.length} open on this page</span></div>
    {loading ? <p role="status">Loading markets…</p> : grid(live)}
    {(closed.length > 0 || voided.length > 0 || resolved.length > 0) && <section className="panel market-history" aria-label="Market history">
      <h2>Market history</h2>
      <p className="muted small">Settled markets are collapsed by default; open a tab to show it and click it again to hide it.</p>
      <div className="tab-bar" role="tablist" aria-label="Market history">
        <button role="tab" aria-selected={historyTab === "closed"} className={historyTab === "closed" ? "selected" : ""} onClick={() => setHistoryTab(historyTab === "closed" ? null : "closed")}>Closed ({closed.length})</button>
        <button role="tab" aria-selected={historyTab === "voided"} className={historyTab === "voided" ? "selected" : ""} onClick={() => setHistoryTab(historyTab === "voided" ? null : "voided")}>Voided ({voided.length})</button>
        <button role="tab" aria-selected={historyTab === "resolved"} className={historyTab === "resolved" ? "selected" : ""} onClick={() => setHistoryTab(historyTab === "resolved" ? null : "resolved")}>Resolved ({resolved.length})</button>
      </div>
      {historyTab === "closed" && grid(closed)}
      {historyTab === "voided" && grid(voided)}
      {historyTab === "resolved" && grid(resolved)}
    </section>}
    <div className="pagination"><button disabled={offset === 0} onClick={() => setOffset(Math.max(0, offset - 100))}>Previous</button><span>Page {offset / 100 + 1}</span><button disabled={instances.length < 100} onClick={() => setOffset(offset + 100)}>Next</button></div>
  </>;
}
