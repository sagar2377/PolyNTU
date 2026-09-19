import { useEffect, useRef, useState } from "react";
import { api, units, quantityMillis, pendingTrade, PENDING_KEY } from "../api";
export default function TradePanel({ instance, account, onTrade }) {
  const [outcome, setOutcome] = useState(instance.outcomes[0].id);
  const [side, setSide] = useState("buy");
  const [quantity, setQuantity] = useState("10");
  const [quoteData, setQuote] = useState(null);
  const requestKey = `${outcome}:${side}:${quantity}:${instance.version}`;
  const quote = quoteData?.requestKey === requestKey ? quoteData : null;
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [receipt, setReceipt] = useState(null);
  const [pending, setPending] = useState(() => { const saved = pendingTrade(); return saved?.account_id === account?.id && saved?.instance_id === instance.id ? saved : null; });
  const [remaining, setRemaining] = useState(0);
  const sequence = useRef(0);
  const executing = useRef(false);
  useEffect(() => {
    if (!quote) return;
    const update = () => setRemaining(Math.max(0, Math.ceil((quote.local_deadline - Date.now()) / 1000)));
    update(); const timer = setInterval(update, 200); return () => clearInterval(timer);
  }, [quote]);
  const preview = async (event) => {
    event.preventDefault(); setBusy(true); setError(""); setReceipt(null);
    const version = ++sequence.current;
    try {
      const result = await api.quote({ instance_id: instance.id, outcome_id: outcome, side, quantity_millis: quantityMillis(quantity) });
      if (version === sequence.current) setQuote({ ...result, requestKey, local_deadline: Date.now() + result.expires_ms - result.server_time_ms });
    } catch (e) { setError(e.message); } finally { setBusy(false); }
  };
  const execute = async () => {
    if (busy || executing.current || (!pending && !quote)) return;
    executing.current = true;
    const attempt = pending || { account_id: account.id, instance_id: instance.id, body: { quote_token: quote.quote_token, limit_micros: quote.amount_micros }, key: crypto.randomUUID() };
    setBusy(true); setError("");
    try {
      localStorage.setItem(PENDING_KEY, JSON.stringify(attempt)); setPending(attempt);
      const result = await api.trade(attempt.body, attempt.key);
      localStorage.removeItem(PENDING_KEY); setPending(null); setQuote(null); setReceipt(result); onTrade();
    } catch (e) {
      if ([400, 404, 409, 422].includes(e.status)) { localStorage.removeItem(PENDING_KEY); setPending(null); setQuote(null); }
      setError(e.message);
    } finally { executing.current = false; setBusy(false); }
  };
  const otherPending = pendingTrade();
  const blocked = otherPending && !pending;
  return <section className="panel trade-panel"><h2>{pending ? "Retrieve your trade receipt" : "Trade outcome shares"}</h2>
    {!account ? <p className="muted">Sign in above to preview and place a trade.</p> : <>
      <p className="muted small">Available: <strong>{units(account.balance_micros)} units</strong></p>
      {error && <p className="inline-error" role="alert">{error}</p>}
      {blocked && <p className="inline-error">Resolve the pending trade before starting another one. Use “Resume trade” above.</p>}
      {pending ? <><p>The original request is saved. Retrying uses the same key, so a completed trade returns its existing receipt.</p><button className="primary wide" disabled={busy} onClick={execute}>{busy ? "Retrieving…" : "Retry / retrieve receipt"}</button></> : <>
        <form onSubmit={preview}><fieldset disabled={busy || !instance.tradable || Boolean(blocked)}><legend className="sr-only">Trade details</legend>
          <div className="segmented"><button type="button" aria-pressed={side === "buy"} className={side === "buy" ? "selected" : ""} onClick={() => setSide("buy")}>Buy</button><button type="button" aria-pressed={side === "sell"} className={side === "sell" ? "selected" : ""} onClick={() => setSide("sell")}>Sell owned shares</button></div>
          <label htmlFor="outcome">Outcome</label><select id="outcome" value={outcome} onChange={(e) => setOutcome(e.target.value)}>{instance.outcomes.map((o) => <option key={o.id} value={o.id}>{o.label}</option>)}</select>
          <label htmlFor="quantity">Shares <span className="muted">(0.001–100)</span></label><input id="quantity" inputMode="decimal" value={quantity} onChange={(e) => setQuantity(e.target.value)} required />
          <button className="wide" disabled={quantityMillis(quantity) === null}>{busy ? "Calculating…" : "Preview trade"}</button>
        </fieldset></form>
        {!instance.tradable && <p className="muted">Trading is {instance.suspended ? "suspended" : "closed"}.</p>}
        {quote && <div className="quote-preview"><div className="quote-total"><span>{side === "buy" ? "Total cost" : "Total proceeds"}</span><strong>{units(quote.amount_micros)} units</strong></div><dl><div><dt>Average price per share</dt><dd>{quote.average_price.toFixed(6)}</dd></div><div><dt>Trading fee (0.25%)</dt><dd>{units(quote.fee_micros)} units</dd></div><div><dt>Probability after trade</dt><dd>{(quote.price_after * 100).toFixed(2)}%</dd></div><div><dt>Quote expires</dt><dd>{remaining > 0 ? `in ${remaining}s` : "Expired — preview again"}</dd></div></dl><button className="primary wide" disabled={busy || remaining === 0 || !instance.tradable} onClick={execute}>Confirm {side}</button><p className="small muted">A changed price requires a new preview. Winning shares resolve to one unit each.</p></div>}
      </>}
      {receipt && <div className="receipt" role="status"><strong>Trade confirmed</strong><p>{receipt.side === "buy" ? "Bought" : "Sold"} {receipt.quantity_millis / 1000} shares for {units(receipt.amount_micros)} units.</p><p>Balance: {units(receipt.balance_micros)} units</p><span className="small">Receipt {receipt.trade_id}</span></div>}
    </>}
  </section>;
}
