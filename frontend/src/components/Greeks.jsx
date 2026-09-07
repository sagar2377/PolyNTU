const ORDER = ["delta", "gamma", "vega", "theta", "rho"];
const LABELS = { delta: "Delta", gamma: "Gamma", vega: "Vega", theta: "Theta", rho: "Rho" };

export default function Greeks({ greeks, hints }) {
  return (
    <dl className="greeks-grid">
      {ORDER.map((key) => (
        <div className="greek-cell" key={key} title={hints?.[key] || ""}>
          <dt>{LABELS[key]}</dt>
          <dd>{greeks[key].toFixed(4)}</dd>
        </div>
      ))}
    </dl>
  );
}
