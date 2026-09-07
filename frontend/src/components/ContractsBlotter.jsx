import { formatMinuteOfDay } from "../api";

const STATE_LABEL = {
  created: "Open",
  arrived: "Arrived",
  settled: "Settled",
  expired: "Expired",
};

export default function ContractsBlotter({ contracts }) {
  if (!contracts.length) {
    return <p className="muted">No positions yet — create a confidence contract above.</p>;
  }
  return (
    <table className="contracts-table">
      <thead>
        <tr>
          <th>Trip</th>
          <th>Type</th>
          <th>Threshold</th>
          <th>Predicted @ creation</th>
          <th>State</th>
          <th>Settlement</th>
        </tr>
      </thead>
      <tbody>
        {contracts.map((c) => (
          <tr key={c.id}>
            <td>{formatMinuteOfDay(c.scheduled_minute_of_day)}</td>
            <td className={c.contract_type}>{c.contract_type}</td>
            <td>{c.threshold_minutes} min</td>
            <td>{c.predicted_delay_at_creation.toFixed(1)} min</td>
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
