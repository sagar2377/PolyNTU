import Greeks from "./Greeks";

function parseLabel(contractType) {
  const [type, threshold] = contractType.split("@");
  return { type, threshold: parseFloat(threshold) };
}

export default function QuotePanel({ quote, onCreateContract, creatingKey }) {
  if (!quote) {
    return <p className="muted">Select an upcoming trip to price its confidence contracts.</p>;
  }

  const byThreshold = {};
  for (const c of quote.contracts) {
    const { type, threshold } = parseLabel(c.contract_type);
    byThreshold[threshold] = byThreshold[threshold] || {};
    byThreshold[threshold][type] = c;
  }

  return (
    <div className="quote-panel">
      <div className="quote-summary">
        <div>
          <span className="muted">Live predicted delay</span>
          <strong>{quote.predicted_delay_minutes.toFixed(1)} min</strong>
        </div>
        <div>
          <span className="muted">Time remaining</span>
          <strong>{quote.minutes_remaining.toFixed(1)} min</strong>
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
        <span className="muted">(a gap here reflects delay's real-world skew, not a bug — see README)</span>
      </p>

      {Object.entries(byThreshold)
        .sort(([a], [b]) => Number(a) - Number(b))
        .map(([threshold, pair]) => (
          <div className="threshold-block" key={threshold}>
            <h4>Threshold: within {threshold} min</h4>
            <div className="contract-cards">
              {["call", "put"].map((type) => {
                const contract = pair[type];
                if (!contract) return null;
                const key = `${quote.scheduled_minute_of_day}-${threshold}-${type}`;
                return (
                  <div className="contract-card" key={type}>
                    <div className="contract-card-header">
                      <span className={`badge ${type}`}>{type === "call" ? "Call — arrives before" : "Put — arrives after"}</span>
                      <span className="price">{contract.price.toFixed(3)} <em>conf. units</em></span>
                    </div>
                    <Greeks greeks={contract.greeks} />
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
