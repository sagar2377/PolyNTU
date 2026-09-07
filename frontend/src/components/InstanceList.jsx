import { formatMinutes } from "../api";

export default function InstanceList({ instances, unitLabel, selectedId, onSelect }) {
  if (!instances.length) {
    return <p className="muted">No upcoming resolution events on this market right now.</p>;
  }
  return (
    <ul className="trip-list">
      {instances.map((inst) => (
        <li
          key={inst.instance_id}
          className={inst.instance_id === selectedId ? "trip selected" : "trip"}
          onClick={() => onSelect(inst.instance_id)}
        >
          <span className="trip-time">{formatMinutes(inst.minutes_remaining)}</span>
          <span className={inst.spot > 0 ? "trip-delay late" : "trip-delay early"}>
            live estimate {inst.spot >= 0 ? "+" : ""}
            {inst.spot.toFixed(1)} {unitLabel}
          </span>
        </li>
      ))}
    </ul>
  );
}
