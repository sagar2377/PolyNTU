const LABELS = {
  delta: { label: "Delta", hint: "sensitivity to the current predicted arrival time moving" },
  gamma: { label: "Gamma", hint: "sensitivity of delta itself to the predicted arrival time" },
  vega: { label: "Vega", hint: "sensitivity to route volatility (punctuality uncertainty)" },
  theta: { label: "Theta", hint: "value decay as the scheduled time approaches" },
  rho: { label: "Rho", hint: "not meaningful here (rate is fixed at 0)" },
};

export default function Greeks({ greeks }) {
  return (
    <dl className="greeks-grid">
      {Object.entries(LABELS).map(([key, { label, hint }]) => (
        <div className="greek-cell" key={key} title={hint}>
          <dt>{label}</dt>
          <dd>{greeks[key].toFixed(4)}</dd>
        </div>
      ))}
    </dl>
  );
}
