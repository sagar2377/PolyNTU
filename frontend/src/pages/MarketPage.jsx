import { useEffect, useState } from "react";
import { api, categories, deriveResolutionKeyPair, signResolution, signingKey as accountSigningKey, storeSigningKey, timestamp } from "../api";
import TradePanel from "../components/TradePanel";
import PriceHistoryChart from "../components/PriceHistoryChart";
import DayProbabilityBars from "../components/DayProbabilityBars";
export default function MarketPage({ id, account, refresh, onTrade, onError, onBack, onSelect }) {
  const [instance, setInstance] = useState(null);
  const [seriesData, setSeriesData] = useState(null);
  const [reloads, setReloads] = useState(0);
  const [busy, setBusy] = useState(false);
  const [choices, setChoices] = useState({});
  const [keyPassword, setKeyPassword] = useState("");
  const [deriving, setDeriving] = useState(false);
  const [localKey, setLocalKey] = useState(null);
  const [historyTab, setHistoryTab] = useState(null);
  useEffect(() => {
    let cancelled = false; let debounce;
    const load = () => api.instance(id).then((value) => { if (!cancelled) { setInstance(value); setReloads((n) => n + 1); } }).catch((e) => { if (!cancelled) onError(e.message); });
    load(); const stream = new EventSource(api.eventsUrl(id));
    stream.addEventListener("market", () => { clearTimeout(debounce); debounce = setTimeout(load, 100); });
    const timer = setInterval(load, 5000);
    return () => { cancelled = true; clearTimeout(debounce); clearInterval(timer); stream.close(); };
  }, [id, refresh, onError]);
  // Every bracket page carries its whole series: the schedule facts, the day
  // view, and the sibling brackets, so no separate series page is needed.
  const seriesId = instance?.series_id;
  useEffect(() => {
    if (!seriesId) return;
    let cancelled = false;
    const load = () => api.series(seriesId).then((value) => { if (!cancelled) setSeriesData(value); }).catch(() => { if (!cancelled) setSeriesData(null); });
    load(); const timer = setInterval(load, 5000);
    return () => { cancelled = true; clearInterval(timer); };
  }, [seriesId, refresh]);
  if (!instance) return <p role="status">Loading market…</p>;
  const series = seriesId ? seriesData : null;
  const resultLabel = instance.result?.kind === "winner" ? instance.outcomes[instance.result.outcome]?.label : "Voided";
  const recurring = series?.schedule?.kind === "recurring";
  const intervalMinutes = Math.round((series?.schedule?.interval_ms || 0) / 60000);
  const windowLabel = (minutes) => `${String(Math.floor(minutes / 60)).padStart(2, "0")}:${String(minutes % 60).padStart(2, "0")}`;
  const dayNames = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];
  const daysLabel = (days) => {
    const set = days ?? [];
    if (set.length === 7) return "Every day";
    if (set.join(",") === "1,2,3,4,5") return "Monday to Friday";
    if (set.join(",") === "6,7") return "Saturday and Sunday";
    return set.map((d) => dayNames[d - 1]).join(", ");
  };
  const live = series ? series.instances.filter((i) => i.state === "open") : [];
  const awaiting = series ? series.instances.filter((i) => i.state === "closed" || i.state === "resolving") : [];
  const voided = series ? series.instances.filter((i) => i.state === "voided") : [];
  const resolved = series ? series.instances.filter((i) => i.state === "resolved") : [];
  const isCreator = account && series && series.creator_account_id === account.id;
  const canSign = isCreator && series.resolution?.authority === "creator";
  const key = localKey || (canSign ? accountSigningKey(account) : null);
  const keyMatches = key?.public_key === series?.resolution?.public_key;
  const resolve = async (bracket, outcomeId, pair) => {
    setBusy(true); onError("");
    try {
      const nonce = crypto.randomUUID();
      const signature = await signResolution(pair.private_key, bracket.id, outcomeId, nonce);
      await api.resolveMarket(bracket.id, { outcome_id: outcomeId, nonce, signature });
      setSeriesData(await api.series(seriesId));
      setInstance(await api.instance(id));
    } catch (e) { onError(e.message); } finally { setBusy(false); }
  };
  // The signing key is derived from the account password (ADR 0007
  // amendment); this prompt covers a browser that signed in through a saved
  // token or logged in before the key was cached.
  const deriveKey = async () => {
    setDeriving(true); onError("");
    try {
      if (!account?.email) throw new Error("Sign in with your email account to resolve");
      const pair = await deriveResolutionKeyPair(account.email, keyPassword);
      if (pair.public_key !== series.resolution?.public_key) {
        throw new Error("That password does not match the key published for this series");
      }
      storeSigningKey(account.email, pair);
      setLocalKey(pair);
      setKeyPassword("");
    } catch (e) { onError(e.message); } finally { setDeriving(false); }
  };
  return <>
    <button className="link-button" onClick={onBack}>← All markets</button>
    <div className="market-heading"><span className={`category category-${instance.category}`}>{categories[instance.category]}</span><h1>{instance.title}</h1><p className="muted">{instance.data_mode === "simulated" ? "Simulated observations" : "Evidence from the published source"} · Times in Singapore (SGT)</p></div>
    {instance.result && <div className="result-banner"><strong>{instance.state === "resolving" ? "Settlement in progress" : "Final result"}: {resultLabel}</strong><span>{instance.result.reason}</span></div>}
    <div className="layout"><section className="panel"><h2>Market probabilities</h2>
      {instance.outcomes.map((outcome) => <div className="outcome-row" key={outcome.id}><span>{outcome.label}</span><strong>{(outcome.probability * 100).toFixed(2)}%</strong><div className="probability-track"><span style={{ width: `${outcome.probability * 100}%` }} /></div></div>)}
      <p className="muted small">Prices reflect trading activity. Opening prices are uniform and are not a forecast from a data provider.</p>
      <dl className="market-facts"><div><dt>Trading closes</dt><dd>{timestamp(instance.close_ms)}</dd></div><div><dt>Observation window</dt><dd>{timestamp(instance.observation_start_ms)} – {timestamp(instance.observation_end_ms)}</dd></div><div><dt>Evidence deadline</dt><dd>{timestamp(instance.evidence_deadline_ms)}</dd></div><div><dt>Liquidity parameter</dt><dd>{instance.liquidity_units} units</dd></div><div><dt>Trading fee</dt><dd>{instance.fee_charged ? "25 bps, half funds the creator" : "None, a welfare market"}</dd></div></dl>
    </section><TradePanel instance={instance} account={account} onTrade={onTrade} /></div>
    <section className="panel" aria-label="Price and volume history"><h2>Price and volume history</h2>
      <p className="muted small">Each line is one outcome's price; bars are traded volume per interval. The chart refreshes with every trade.</p>
      <PriceHistoryChart instance={instance} reloadKey={reloads} />
    </section>
    {series && instance.outcomes.length === 2 && <section className="panel" aria-label="Day-long probability view"><h2>Implied probability across the day</h2>
      {series.day?.weighted_probability !== null && series.day?.weighted_probability !== undefined
        ? <p className="muted">Volume-weighted {series.day.slots[0]?.outcome_label} probability across live brackets: <strong>{(series.day.weighted_probability * 100).toFixed(1)}%</strong>, each bracket weighted by units bet.</p>
        : <p className="muted">No live brackets have traded volume yet; the weighted probability appears with the first trade.</p>}
      <DayProbabilityBars series={series} />
    </section>}
    {series && <section className="panel"><h2>How this series works</h2>
      <dl className="market-facts">
        {recurring && <div><dt>Bracket interval</dt><dd>Every {intervalMinutes} minute{intervalMinutes === 1 ? "" : "s"}</dd></div>}
        {recurring && <div><dt>Operating window</dt><dd>{windowLabel(series.schedule.active_start_minute)} to {windowLabel(series.schedule.active_end_minute)} SGT</dd></div>}
        {recurring && <div><dt>Operating days</dt><dd>{daysLabel(series.schedule.active_days)}</dd></div>}
        {recurring && <div><dt>Live horizon</dt><dd>{series.schedule.max_concurrency} brackets, about {series.schedule.max_concurrency * intervalMinutes} minutes ahead</dd></div>}
        {recurring && <div><dt>Runs until</dt><dd>{series.schedule.end_ms ? timestamp(series.schedule.end_ms) : "Perpetual"}</dd></div>}
        <div><dt>Trading fee</dt><dd>{series.fee_charged ? "25 bps, half funds the creator" : "None, a welfare market"}</dd></div>
        <div><dt>Resolved by</dt><dd>{series.resolution?.authority === "creator" ? "The creator's signed statement" : series.resolution?.authority === "resolver" ? (series.resolution.endpoint?.includes("/resolvers/ntu-bus") ? "NTU Bus API" : `An automatic resolver (${series.resolution.endpoint})`) : "Platform administrator evidence"}</dd></div>
        <div><dt>Liquidity parameter</dt><dd>{series.liquidity_units} units per bracket</dd></div>
        <div><dt>Series state</dt><dd>{series.state === "active" ? "Active" : "Ended"}</dd></div>
      </dl>
      <p>{series.resolution_criterion}</p>
    </section>}
    {recurring && live.length > 0 && <section className="panel"><h2>Live brackets</h2>
      <ul className="bracket-list">{live.map((bracket) => <li key={bracket.id}><span>Closes {timestamp(bracket.close_ms)} SGT</span>
        <span>{bracket.outcomes.map((o) => `${o.label} ${(o.probability * 100).toFixed(1)}%`).join(" · ")}</span>
        {bracket.id === id ? <span className="muted small">This bracket</span> : <button onClick={() => onSelect(bracket.id)}>Trade</button>}</li>)}
      </ul>
    </section>}
    {series && (awaiting.length > 0 || voided.length > 0 || resolved.length > 0) && <section className="panel">
      <h2>Bracket history</h2>
      <p className="muted small">History is collapsed by default; open a tab to show it and click it again to hide it.</p>
      <div className="tab-bar" role="tablist" aria-label="Bracket history">
        <button role="tab" aria-selected={historyTab === "closed"} className={historyTab === "closed" ? "selected" : ""} onClick={() => setHistoryTab(historyTab === "closed" ? null : "closed")}>Closed ({awaiting.length})</button>
        <button role="tab" aria-selected={historyTab === "voided"} className={historyTab === "voided" ? "selected" : ""} onClick={() => setHistoryTab(historyTab === "voided" ? null : "voided")}>Voided ({voided.length})</button>
        <button role="tab" aria-selected={historyTab === "resolved"} className={historyTab === "resolved" ? "selected" : ""} onClick={() => setHistoryTab(historyTab === "resolved" ? null : "resolved")}>Resolved ({resolved.length})</button>
      </div>
      {historyTab === "closed" && <>
        {canSign && (keyMatches
          ? <p className="muted small">Pick the outcome for each closed bracket; this browser signs with the key derived from your password.</p>
          : <div className="resolve-control"><label className="sr-only" htmlFor="key-password">Password</label>
              <input id="key-password" type="password" placeholder="Account password" value={keyPassword} autoComplete="current-password" onChange={(e) => setKeyPassword(e.target.value)} />
              <button disabled={deriving || !keyPassword} onClick={deriveKey}>Derive signing key</button></div>)}
        <ul className="bracket-list">{awaiting.map((bracket) => <li key={bracket.id}>
          <span>Closed {timestamp(bracket.close_ms)} SGT</span>
          <span>{bracket.outcomes.map((o) => `${o.label} ${(o.probability * 100).toFixed(1)}%`).join(" · ")}</span>
          {canSign && keyMatches
            ? <span className="resolve-control"><label className="sr-only" htmlFor={`resolve-${bracket.id}`}>Outcome</label>
                <select id={`resolve-${bracket.id}`} value={choices[bracket.id] ?? bracket.outcomes[0].id} onChange={(e) => setChoices({ ...choices, [bracket.id]: e.target.value })}>
                  {bracket.outcomes.map((o) => <option key={o.id} value={o.id}>{o.label}</option>)}
                </select>
                <button disabled={busy} onClick={() => resolve(bracket, choices[bracket.id] ?? bracket.outcomes[0].id, key)}>Resolve</button></span>
            : canSign
              ? <span className="muted small">Derive the signing key above to resolve these brackets.</span>
              : <span className="muted small">{series.resolution?.authority === "resolver" ? "The automatic resolver is answering" : "Awaiting the resolution authority"}</span>}
        </li>)}
        </ul>
      </>}
      {historyTab === "voided" && <ul className="bracket-list">{voided.map((bracket) => <li key={bracket.id}>
        <span>Closed {timestamp(bracket.close_ms)} SGT</span>
        <span>Voided</span>
        {bracket.id === id ? <span className="muted small">This bracket</span> : <button onClick={() => onSelect(bracket.id)}>View</button>}
      </li>)}
      </ul>}
      {historyTab === "resolved" && <ul className="bracket-list">{resolved.map((bracket) => <li key={bracket.id}>
        <span>Closed {timestamp(bracket.close_ms)} SGT</span>
        <span>Result: {bracket.outcomes[bracket.result?.outcome]?.label}</span>
        {bracket.id === id ? <span className="muted small">This bracket</span> : <button onClick={() => onSelect(bracket.id)}>View</button>}
      </li>)}
      </ul>}
    </section>}
    <section className="panel"><h2>How this market resolves</h2><p>{instance.resolution_criterion}</p><p className="muted">{instance.void_policy}</p><dl className="market-facts"><div><dt>Source</dt><dd>{instance.source_id}</dd></div><div><dt>Evidence</dt><dd>{instance.evidence ? `Received ${timestamp(instance.evidence.received_ms)}` : "Awaiting the observation window and final evidence"}</dd></div></dl>{instance.evidence && <details><summary>View resolution evidence</summary><pre>{JSON.stringify(instance.evidence.payload, null, 2)}</pre></details>}</section>
  </>;
}
