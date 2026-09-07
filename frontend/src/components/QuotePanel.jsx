import Greeks from "./Greeks";

export default function QuotePanel({ quote, onCreateContract, creatingKey }) {
  if (!quote) {
    return <p className="muted">Select an upcoming resolution event to price its confidence contracts.</p>;
  }

  const byThreshold = {};
  for (const c of quote.contracts) {
    byThreshold[c.threshold] = byThreshold[c.threshold] || {};
    byThreshold[c.threshold][c.contract_type] = c;
  }

  return (
    <div className="quote-panel">
      <div className="quote-summary">
        <div>
          <span className="muted">Live estimate</span>
          <strong>{quote.spot.toFixed(1)} {quote.unit_label}</strong>
        </div>
        <div>
          <span className="muted">Time remaining</span>
          <strong>{quote.minutes_remaining.toFixed(0)} min</strong>
        </div>
        <div>
          <span className="muted">Calibrated volatility (&sigma;)</span>
          <strong>{(quote.sigma * 100).toFixed(1)}%</strong>
        </div>
      </div>

      <p className="cross-check">
        Cross-check on the shortest threshold — analytical vs. Monte Carlo:{" "}
        <strong>GBM {quote.monte_carlo_gbm_price.toFixed(3)}</strong> vs.{" "}
        <strong>bootstrap {quote.monte_carlo_bootstrap_price.toFixed(3)}</strong>{" "}
        <span className="muted">(a gap here reflects the domain's real-world skew, not a bug — see README)</span>
      </p>

      {Object.entries(byThreshold)
        .sort(([a], [b]) => Number(a) - Number(b))
        .map(([threshold, pair]) => (
          <div className="threshold-block" key={threshold}>
            <h4>Threshold: {threshold} {quote.unit_label}</h4>
            <div className="contract-cards">
              {["call", "put"].map((type) => {
                const contract = pair[type];
                if (!contract) return null;
                const key = `${quote.instance_id}-${threshold}-${type}`;
                return (
                  <div className="contract-card" key={type}>
                    <div className="contract-card-header">
                      <span className={`badge ${type}`}>
                        {type === "call" ? "Call" : "Put"} — {contract.label}
                      </span>
                      <span className="price">{contract.price.toFixed(3)} <em>conf. units</em></span>
                    </div>
                    <Greeks greeks={contract.greeks} hints={quote.greek_hints} />
                    <button
                      disabled={creatingKey === key}
                      onClick={() => onCreateContract(type, Number(threshold))}
                    >
                      {creatingKey === key ? "Creating…" : "Create position"}
                    </button>
                  </div>
                );
              })}
            </div>
          </div>
        ))}
    </div>
  );
}
