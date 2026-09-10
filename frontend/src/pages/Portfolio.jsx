import { useEffect, useState } from "react";
import { api, units, timestamp } from "../api";
export default function Portfolio({ account, refresh, onError, onSelect }) {
  const [data, setData] = useState(null);
  const [trades, setTrades] = useState([]);
  const [offset, setOffset] = useState(0);
  const accountId = account?.id;
  useEffect(() => {
    if (!accountId) return;
    let cancelled = false;
    const load = async () => {
      try { const [portfolio, history] = await Promise.all([api.portfolio(offset), api.trades(offset)]); if (!cancelled) { setData(portfolio); setTrades(history); } }
      catch (e) { if (!cancelled) onError(e.message); }
    };
    load(); const timer = setInterval(load, 5000); return () => { cancelled = true; clearInterval(timer); };
  }, [accountId, refresh, offset, onError]);
  if (!account) return <div className="empty-state">Sign in to view your positions and trade history.</div>;
  if (!data || data.account.id !== account.id) return <p role="status">Loading portfolio…</p>;
  return <>
    <section className="hero compact"><p className="eyebrow">YOUR PORTFOLIO</p><h1>{units(data.account.balance_micros)} <span className="muted">units</span></h1><p>Available simulated balance · {data.account.display_name}</p></section>
    <section className="panel"><h2>Outcome shares</h2>{!data.positions.length ? <p className="muted">No positions on this page.</p> : <div className="table-wrap"><table><thead><tr><th>Market</th><th>Outcome</th><th>Shares</th><th>Status</th></tr></thead><tbody>{data.positions.map((p) => <tr key={`${p.instance_id}:${p.outcome_id}`}><td><button className="link-button" onClick={() => onSelect(p.instance_id)}>{p.title}</button></td><td>{p.outcome_label}</td><td>{p.quantity_millis / 1000}</td><td>{p.settled ? "Redeemed" : p.state}</td></tr>)}</tbody></table></div>}</section>
    <section className="panel"><h2>Settlement credits</h2>{!data.settlements.length ? <p className="muted">Final credits appear after resolution.</p> : <div className="table-wrap"><table><thead><tr><th>Market</th><th>Credit</th><th>Time (SGT)</th></tr></thead><tbody>{data.settlements.map((s) => <tr key={s.instance_id}><td>{s.title}</td><td>{units(s.credit_micros)} units</td><td>{timestamp(s.created_ms)}</td></tr>)}</tbody></table></div>}</section>
    <section className="panel"><h2>Trade history</h2>{!trades.length ? <p className="muted">No trades on this page.</p> : <div className="table-wrap"><table><thead><tr><th>Market / outcome</th><th>Action</th><th>Shares</th><th>Units</th><th>Time (SGT)</th></tr></thead><tbody>{trades.map((t) => <tr key={t.id}><td>{t.title}<span className="muted table-detail">{t.outcome_label}</span></td><td>{t.side}</td><td>{t.quantity_millis / 1000}</td><td>{units(t.amount_micros)}</td><td>{timestamp(t.created_ms)}</td></tr>)}</tbody></table></div>}</section>
    <div className="pagination"><button disabled={offset === 0} onClick={() => setOffset(Math.max(0, offset - 100))}>Previous</button><span>Page {offset / 100 + 1}</span><button disabled={trades.length < 100 && data.positions.length < 100 && data.settlements.length < 100} onClick={() => setOffset(offset + 100)}>Next</button></div>
  </>;
}
