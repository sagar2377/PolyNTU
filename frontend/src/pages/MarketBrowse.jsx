import { useEffect, useState } from "react";
import { api, categories, timestamp } from "../api";

const HISTORY_CLASSES = ["closed", "voided", "resolved"];
const emptyHistory = { closed: [], voided: [], resolved: [] };

export default function MarketBrowse({ refresh, onSelect, onError }) {
  const [live, setLive] = useState([]);
  const [liveOffset, setLiveOffset] = useState(0);
  const [history, setHistory] = useState(emptyHistory);
  const [offsets, setOffsets] = useState({ closed: 0, voided: 0, resolved: 0 });
  const [category, setCategory] = useState("all");
  const [loading, setLoading] = useState(true);
  const [historyTab, setHistoryTab] = useState(null);
  useEffect(() => {
    let cancelled = false;
    const load = () => api.instances(liveOffset, "open")
      .then((rows) => { if (!cancelled) { setLive(rows); setLoading(false); } })
      .catch((e) => { if (!cancelled) { onError(e.message); setLoading(false); } });
    load(); const timer = setInterval(load, 10000);
    return () => { cancelled = true; clearInterval(timer); };
  }, [refresh, onError, liveOffset]);
  // Each history class pages on its own, so all three lists load regardless
  // of which tab is open; the counts on the tab buttons come from them.
  useEffect(() => {
    let cancelled = false;
    const load = () => Promise.all(HISTORY_CLASSES.map((cls) => api.instances(offsets[cls], cls)))
      .then(([closed, voided, resolved]) => { if (!cancelled) setHistory({ closed, voided, resolved }); })
      .catch((e) => { if (!cancelled) onError(e.message); });
    load(); const timer = setInterval(load, 10000);
    return () => { cancelled = true; clearInterval(timer); };
  }, [refresh, onError, offsets]);
  const inCategory = (rows) => rows.filter((i) => category === "all" || i.category === category);
  const liveRows = inCategory(live);
  const card = (instance) => <button className="market-card" key={instance.id} onClick={() => onSelect(instance.id)}>
    <div className="card-meta"><span className={`category category-${instance.category}`}>{categories[instance.category]}</span><span className={`status ${instance.tradable ? "open" : ""}`}>{instance.tradable ? "Open" : instance.state}</span></div>
    <h3>{instance.title}</h3><div className="outcome-preview">{instance.outcomes.slice(0, 3).map((outcome) => <div key={outcome.id}><span>{outcome.label}</span><strong>{(outcome.probability * 100).toFixed(1)}%</strong><span className="probability-track"><span style={{ width: `${outcome.probability * 100}%` }} /></span></div>)}</div>
    <div className="card-footer"><span>{instance.data_mode === "simulated" ? "Simulated observations" : "Published evidence source"}</span><span>Closes {timestamp(instance.close_ms)} SGT</span></div>
  </button>;
  const grid = (rows) => rows.length === 0 ? <div className="empty-state">No markets in this category on this page.</div> : <div className="market-grid">{rows.map(card)}</div>;
  const count = (rows) => rows.length >= 100 ? "100+" : String(rows.length);
  const pager = (offset, rows, turn) => <div className="pagination">
    <button disabled={offset === 0} onClick={() => turn(Math.max(0, offset - 100))}>Previous</button>
    <span>Page {offset / 100 + 1}</span>
    <button disabled={rows.length < 100} onClick={() => turn(offset + 100)}>Next</button>
  </div>;
  return <>
    <section className="hero"><p className="eyebrow">THE CAMPUS FORECAST</p><h1>What happens next?</h1><p>Weather, bus arrivals, campus crowds and more.<br />Explore the outcomes. Share what you know.</p></section>
    <div className="filters" aria-label="Market categories">{[["all", "All markets"], ...Object.entries(categories)].map(([id, name]) => <button key={id} aria-pressed={category === id} className={category === id ? "selected" : ""} onClick={() => setCategory(id)}>{name}</button>)}</div>
    <div className="section-heading"><h2>{category === "all" ? "Campus markets" : categories[category]}</h2><span className="muted">{liveRows.length} open on this page</span></div>
    {loading ? <p role="status">Loading markets…</p> : grid(liveRows)}
    {/* The live grid pages only in the unlikely case it fills a page; history
        pages per class, and only while its tab is open. */}
    {(liveOffset > 0 || live.length >= 100) && pager(liveOffset, live, setLiveOffset)}
    {(history.closed.length > 0 || history.voided.length > 0 || history.resolved.length > 0) && <section className="panel market-history" aria-label="Market history">
      <h2>Market history</h2>
      <p className="muted small">Settled markets are collapsed by default; open a tab to show it and click it again to hide it.</p>
      <div className="tab-bar" role="tablist" aria-label="Market history">
        {HISTORY_CLASSES.map((cls) => <button key={cls} role="tab" aria-selected={historyTab === cls} className={historyTab === cls ? "selected" : ""} onClick={() => setHistoryTab(historyTab === cls ? null : cls)}>
          {cls === "closed" ? "Closed" : cls === "voided" ? "Voided" : "Resolved"} ({count(history[cls])})
        </button>)}
      </div>
      {HISTORY_CLASSES.map((cls) => historyTab === cls && <div key={cls}>
        {grid(inCategory(history[cls]))}
        {pager(offsets[cls], history[cls], (offset) => setOffsets({ ...offsets, [cls]: offset }))}
      </div>)}
    </section>}
  </>;
}
