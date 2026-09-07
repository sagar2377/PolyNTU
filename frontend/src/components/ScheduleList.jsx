import { formatMinuteOfDay } from "../api";

export default function ScheduleList({ trips, selectedMinute, onSelect }) {
  if (!trips.length) {
    return <p className="muted">No upcoming trips on this route right now.</p>;
  }
  return (
    <ul className="trip-list">
      {trips.map((trip) => (
        <li
          key={trip.scheduled_minute_of_day}
          className={trip.scheduled_minute_of_day === selectedMinute ? "trip selected" : "trip"}
          onClick={() => onSelect(trip.scheduled_minute_of_day)}
        >
          <span className="trip-time">{formatMinuteOfDay(trip.scheduled_minute_of_day)}</span>
          <span className="trip-remaining">
            {trip.minutes_remaining <= 0 ? "due now" : `${Math.round(trip.minutes_remaining)} min out`}
          </span>
          <span className={trip.predicted_delay_minutes > 0 ? "trip-delay late" : "trip-delay early"}>
            predicted {trip.predicted_delay_minutes >= 0 ? "+" : ""}
            {trip.predicted_delay_minutes.toFixed(1)} min
          </span>
        </li>
      ))}
    </ul>
  );
}
