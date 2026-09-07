export default function MarketBrowse({ markets, onSelect }) {
  if (!markets.length) {
    return <p className="muted">No markets available.</p>;
  }
  return (
    <div className="market-grid">
      {markets.map((m) => (
        <div className="market-card" key={m.id} onClick={() => onSelect(m.id)}>
          <span className={`badge market-type-${m.market_type}`}>{m.market_type.replace("_", " ")}</span>
          <h3>{m.title}</h3>
          <p className="muted resolution-criterion">{m.resolution_criterion}</p>
          <p className="unit-label">Priced in: {m.unit_label}</p>
        </div>
      ))}
    </div>
  );
}
