import { useEffect, useRef } from "react";
import { createChart, HistogramSeries, LineSeries } from "lightweight-charts";
import { units } from "../api";

/// The day-long probability view for a recurring series (UC-21): each slot's
/// first-outcome probability across the active period, with traded volume per
/// slot and settled slots pinned to their resolved value.
export default function DayProbabilityChart({ series, reloadKey }) {
  const container = useRef(null);
  const chart = useRef(null);
  const created = useRef({ line: null, volume: null });

  useEffect(() => {
    const instance = createChart(container.current, {
      autoSize: true,
      layout: { attributionLogo: false },
      timeScale: { timeVisible: true, secondsVisible: false },
      grid: { vertLines: { color: "#edf0f5" }, horzLines: { color: "#edf0f5" } },
    });
    // Hold refs to the created series; the chart object's series accessor is
    // not available in every lightweight-charts build.
    created.current.line = instance.addSeries(LineSeries, {
      color: "#245ce4",
      lineWidth: 2,
      priceFormat: { type: "percent", precision: 1, minMove: 0.001 },
    });
    created.current.volume = instance.addSeries(HistogramSeries, {
      priceFormat: { type: "custom", formatter: (value) => units(value) },
      priceScaleId: "volume",
    }, 1);
    chart.current = instance;
    return () => { instance.remove(); chart.current = null; created.current = { line: null, volume: null }; };
  }, []);

  useEffect(() => {
    if (!chart.current || !series?.day || !created.current.line) return;
    const slots = [...series.day.slots].sort((a, b) => a.close_ms - b.close_ms);
    const probabilityOf = (slot) => {
      if (slot.state === "resolved" && slot.result?.kind === "winner") {
        return slot.result.outcome === 0 ? 1 : 0;
      }
      return slot.probability;
    };
    created.current.line.setData(slots
      .filter((slot) => slot.state !== "voided")
      .map((slot) => ({ time: Math.round(slot.close_ms / 1000), value: probabilityOf(slot) })));
    created.current.volume.setData(slots.map((slot) => ({
      time: Math.round(slot.close_ms / 1000),
      value: slot.volume_micros,
      color: slot.state === "open" ? "#c6d7fa" : "#dce2e9",
    })));
    chart.current.timeScale().fitContent();
  }, [series, reloadKey]);

  return <div className="chart-container-wrap">
    <div ref={container} className="chart-container" role="img" aria-label="Day-long probability chart" />
  </div>;
}
