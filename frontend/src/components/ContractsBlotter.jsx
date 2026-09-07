const STATE_LABEL = {
  open: "Open",
  resolving: "Resolving",
  settled: "Settled",
  expired: "Expired",
};

export default function ContractsBlotter({ contracts, showMarket = false }) {
  if (!contracts.length) {
    return <p className="muted">No positions yet — create a confidence contract above.</p>;
  }
  return (
    <table className="contracts-table">
      <thead>
        <tr>
          {showMarket && <th>Market</th>}
          <th>Type</th>
          <th>Threshold</th>
          <th>At creation</th>
          <th>State</th>
          <th>Settlement</th>
        </tr>
      </thead>
      <tbody>
        {contracts.map((c) => (
          <tr key={c.id}>
            {showMarket && <td>{c.market_title || c.market_id}</td>}
            <td className={c.contract_type}>{c.contract_type}</td>
            <td>{c.threshold}</td>
            <td>{c.spot_at_creation.toFixed(1)}</td>
            <td>{STATE_LABEL[c.state] || c.state}</td>
            <td>
              {c.state === "settled" ? (
                <span className={c.in_the_money ? "itm" : "otm"}>
                  {c.in_the_money ? "in-the-money" : "out-of-the-money"} ({c.settlement_price.toFixed(2)})
                </span>
              ) : (
                <span className="muted">pending</span>
              )}
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}
