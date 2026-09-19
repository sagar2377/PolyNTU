import { useEffect, useState } from "react";
import { api, categories, timestamp } from "../api";
import TradePanel from "../components/TradePanel";
export default function MarketPage({ id, account, refresh, onTrade, onError, onBack, onOpenSeries }) {
  const [instance, setInstance] = useState(null);
  useEffect(() => {
    let cancelled = false; let debounce;
    const load = () => api.instance(id).then((value) => { if (!cancelled) setInstance(value); }).catch((e) => { if (!cancelled) onError(e.message); });
    load(); const stream = new EventSource(api.eventsUrl(id));
    stream.addEventListener("market", () => { clearTimeout(debounce); debounce = setTimeout(load, 100); });
    const timer = setInterval(load, 5000);
    return () => { cancelled = true; clearTimeout(debounce); clearInterval(timer); stream.close(); };
  }, [id, refresh, onError]);
  if (!instance) return <p role="status">Loading market…</p>;
  const resultLabel = instance.result?.kind === "winner" ? instance.outcomes[instance.result.outcome]?.label : "Voided";
  return <>
    <button className="link-button" onClick={onBack}>← All markets</button>{instance.series_id && <button className="link-button" onClick={() => onOpenSeries(instance.series_id)}>View the series and its brackets</button>}<div className="market-heading"><span className={`category category-${instance.category}`}>{categories[instance.category]}</span><h1>{instance.title}</h1><p className="muted">{instance.data_mode === "simulated" ? "Simulated observations" : "Evidence from the published source"} · Times in Singapore (SGT)</p></div>
    {instance.result && <div className="result-banner"><strong>{instance.state === "resolving" ? "Settlement in progress" : "Final result"}: {resultLabel}</strong><span>{instance.result.reason}</span></div>}
    <div className="layout"><section className="panel"><h2>Market probabilities</h2>
      {instance.outcomes.map((outcome) => <div className="outcome-row" key={outcome.id}><span>{outcome.label}</span><strong>{(outcome.probability * 100).toFixed(2)}%</strong><div className="probability-track"><span style={{ width: `${outcome.probability * 100}%` }} /></div></div>)}
      <p className="muted small">Prices reflect trading activity. Opening prices are uniform and are not a forecast from a data provider.</p>
      <dl className="market-facts"><div><dt>Trading closes</dt><dd>{timestamp(instance.close_ms)}</dd></div><div><dt>Observation window</dt><dd>{timestamp(instance.observation_start_ms)} – {timestamp(instance.observation_end_ms)}</dd></div><div><dt>Evidence deadline</dt><dd>{timestamp(instance.evidence_deadline_ms)}</dd></div><div><dt>Liquidity parameter</dt><dd>{instance.liquidity_units} units</dd></div><div><dt>Trading fee</dt><dd>{instance.fee_charged ? "25 bps, half funds the creator" : "None, a welfare market"}</dd></div></dl>
    </section><TradePanel instance={instance} account={account} onTrade={onTrade} /></div>
    <section className="panel"><h2>How this market resolves</h2><p>{instance.resolution_criterion}</p><p className="muted">{instance.void_policy}</p><dl className="market-facts"><div><dt>Source</dt><dd>{instance.source_id}</dd></div><div><dt>Evidence</dt><dd>{instance.evidence ? `Received ${timestamp(instance.evidence.received_ms)}` : "Awaiting the observation window and final evidence"}</dd></div></dl>{instance.evidence && <details><summary>View resolution evidence</summary><pre>{JSON.stringify(instance.evidence.payload, null, 2)}</pre></details>}</section>
  </>;
}
