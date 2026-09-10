import { useEffect, useState } from "react";
import { api, categories, timestamp } from "../api";
export default function MarketBrowse({ refresh, onSelect, onError }) {
  const [instances, setInstances] = useState([]);
  const [category, setCategory] = useState("all");
  const [loading, setLoading] = useState(true);
  const [offset, setOffset] = useState(0);
  useEffect(() => {
    let cancelled = false;
    const load = () => api.instances(offset).then((rows) => { if (!cancelled) { setInstances(rows); setLoading(false); } }).catch((e) => { if (!cancelled) { onError(e.message); setLoading(false); } });
    load(); const timer = setInterval(load, 10000);
    return () => { cancelled = true; clearInterval(timer); };
  }, [refresh, onError, offset]);
  const visible = instances.filter((i) => category === "all" || i.category === category);
  return <>
    <section className="hero"><p className="eyebrow">THE CAMPUS FORECAST</p><h1>What happens next?</h1><p>Weather, bus arrivals, campus crowds and more.<br />Explore the outcomes. Share what you know.</p></section>
    <div className="filters" aria-label="Market categories">{[["all", "All markets"], ...Object.entries(categories)].map(([id, name]) => <button key={id} aria-pressed={category === id} className={category === id ? "selected" : ""} onClick={() => setCategory(id)}>{name}</button>)}</div>
    <div className="section-heading"><h2>{category === "all" ? "Campus markets" : categories[category]}</h2><span className="muted">{visible.filter((i) => i.tradable).length} open on this page</span></div>
    {loading ? <p role="status">Loading markets…</p> : visible.length === 0 ? <div className="empty-state">No markets in this category on this page.</div> : <div className="market-grid">
      {visible.map((instance) => <button className="market-card" key={instance.id} onClick={() => onSelect(instance.id)}>
        <div className="card-meta"><span className={`category category-${instance.category}`}>{categories[instance.category]}</span><span className={`status ${instance.tradable ? "open" : ""}`}>{instance.tradable ? "Open" : instance.state}</span></div>
        <h3>{instance.title}</h3><div className="outcome-preview">{instance.outcomes.slice(0, 3).map((outcome) => <div key={outcome.id}><span>{outcome.label}</span><strong>{(outcome.probability * 100).toFixed(1)}%</strong><span className="probability-track"><span style={{ width: `${outcome.probability * 100}%` }} /></span></div>)}</div>
        <div className="card-footer"><span>{instance.data_mode === "simulated" ? "Simulated observations" : "Published evidence source"}</span><span>Closes {timestamp(instance.close_ms)} SGT</span></div>
      </button>)}
    </div>}
    <div className="pagination"><button disabled={offset === 0} onClick={() => setOffset(Math.max(0, offset - 100))}>Previous</button><span>Page {offset / 100 + 1}</span><button disabled={instances.length < 100} onClick={() => setOffset(offset + 100)}>Next</button></div>
  </>;
}
