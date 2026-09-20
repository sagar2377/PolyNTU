import { units } from "../api";

const sgtHm = (ms) => {
  const minute = Math.floor(ms / 60000 + 480) % 1440;
  return `${String(Math.floor(minute / 60)).padStart(2, "0")}:${String(minute % 60).padStart(2, "0")}`;
};

/// The day-long probability view for a recurring series (UC-21): one
/// horizontal bar per bracket, longer meaning a higher first-outcome
/// probability, with the traded volume beside it. Slots show their actual
/// implied probability, so an untraded series reads as a flat line of
/// opening probabilities.
export default function DayProbabilityBars({ series }) {
  const slots = [...(series?.day?.slots ?? [])].sort((a, b) => a.close_ms - b.close_ms);
  if (slots.length === 0) return null;
  return <div className="day-bars">
    {slots.map((slot) => <div key={`${slot.bracket_start_ms ?? slot.close_ms}`} className="day-bar-row">
      <span className="day-bar-time">{slot.bracket_start_ms
        ? `${sgtHm(slot.bracket_start_ms)} to ${sgtHm(slot.close_ms)}`
        : sgtHm(slot.close_ms)}</span>
      <span className="probability-track"><span style={{ width: `${slot.probability * 100}%` }} /></span>
      <span className="day-bar-value">{(slot.probability * 100).toFixed(1)}%</span>
      <span className="day-bar-volume">{slot.volume_micros > 0 ? units(slot.volume_micros) : ""}</span>
    </div>)}
  </div>;
}
