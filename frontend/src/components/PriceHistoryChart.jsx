import { useEffect, useRef, useState } from "react";
import { createChart, HistogramSeries, LineSeries } from "lightweight-charts";
import { api, units } from "../api";

const LINE_COLORS = ["#245ce4", "#067554", "#78539b", "#91682d", "#992d34", "#286a96", "#539b78", "#8a5a2d"];

/// Live price and volume history for one instance (UC-19), drawn with
/// lightweight-charts like a stock chart. Reloads on every `reloadKey` bump,
/// which the market page raises from its SSE stream.
export default function PriceHistoryChart({ instance, reloadKey }) {
  const container = useRef(null);
  const chart = useRef(null);
  const series = useRef({ lines: [], volume: null });
  const [error, setError] = useState("");

  useEffect(() => {
    const created = createChart(container.current, {
      autoSize: true,
      layout: { attributionLogo: false },
      timeScale: { timeVisible: true, secondsVisible: false },
      grid: { vertLines: { color: "#edf0f5" }, horzLines: { color: "#edf0f5" } },
    });
    series.current.volume = created.addSeries(HistogramSeries, {
      priceFormat: { type: "custom", formatter: (value) => units(value) },
      priceScaleId: "volume",
    }, 1);
    chart.current = created;
    return () => { created.remove(); chart.current = null; series.current = { lines: [], volume: null }; };
  }, []);

  useEffect(() => {
    let cancelled = false;
    const load = async () => {
      try {
        const span = instance.bracket_start_ms
          ? instance.close_ms - instance.bracket_start_ms
          : 3600000;
        const bucketMs = Math.min(3600000, Math.max(1000, Math.ceil(span / 180)));
        const points = await api.history(instance.id, bucketMs);
        if (cancelled || !chart.current) return;
        const outcomeCount = instance.outcomes.length;
        while (series.current.lines.length < outcomeCount) {
          const index = series.current.lines.length;
          series.current.lines.push(chart.current.addSeries(LineSeries, {
            color: LINE_COLORS[index % LINE_COLORS.length],
            lineWidth: 2,
            priceFormat: { type: "percent", precision: 1, minMove: 0.001 },
          }));
        }
        series.current.lines.forEach((line, index) => {
          line.setData(points.map((point) => ({
            time: Math.round(point.start_ms / 1000),
            value: point.prices[index] ?? 0,
          })));
        });
        series.current.volume.setData(points.map((point) => ({
          time: Math.round(point.start_ms / 1000),
          value: point.volume_micros,
          color: "#c6d7fa",
        })));
        chart.current.timeScale().fitContent();
      } catch (e) { if (!cancelled) setError(e.message); }
    };
    load();
    return () => { cancelled = true; };
  }, [instance.id, instance.close_ms, instance.bracket_start_ms, instance.outcomes, reloadKey]);

  return <div className="chart-container-wrap">
    <div ref={container} className="chart-container" role="img" aria-label="Price and volume history chart" />
    {error && <p className="inline-error">{error}</p>}
  </div>;
}
