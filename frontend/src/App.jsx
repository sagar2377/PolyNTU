import { useEffect, useState, useCallback } from "react";
import { api } from "./api";
import ScheduleList from "./components/ScheduleList";
import QuotePanel from "./components/QuotePanel";
import ContractsBlotter from "./components/ContractsBlotter";
import "./App.css";

const THRESHOLDS = [0, 5, 10];

export default function App() {
  const [routes, setRoutes] = useState([]);
  const [routeId, setRouteId] = useState(null);
  const [trips, setTrips] = useState([]);
  const [selectedMinute, setSelectedMinute] = useState(null);
  const [quote, setQuote] = useState(null);
  const [contracts, setContracts] = useState([]);
  const [creatingKey, setCreatingKey] = useState(null);
  const [error, setError] = useState(null);

  useEffect(() => {
    api.listRoutes().then((r) => {
      setRoutes(r);
      setRouteId(r[0]);
    }).catch((e) => setError(e.message));
  }, []);

  const refresh = useCallback(() => {
    if (!routeId) return;
    api.getSchedule(routeId).then(setTrips).catch((e) => setError(e.message));
    api.listContracts(routeId).then(setContracts).catch((e) => setError(e.message));
  }, [routeId]);

  useEffect(() => {
    refresh();
    setSelectedMinute(null);
    setQuote(null);
  }, [routeId, refresh]);

  useEffect(() => {
    if (routeId && selectedMinute != null) {
      api.getQuote(routeId, selectedMinute, THRESHOLDS).then(setQuote).catch((e) => setError(e.message));
    }
  }, [routeId, selectedMinute]);

  const handleCreateContract = async (type, threshold) => {
    const key = `${selectedMinute}-${threshold}-${type}`;
    setCreatingKey(key);
    try {
      await api.createContract(routeId, type, threshold, selectedMinute);
      await api.listContracts(routeId).then(setContracts);
    } catch (e) {
      setError(e.message);
    } finally {
      setCreatingKey(null);
    }
  };

  const handleAdvanceClock = async (minutes) => {
    try {
      await api.advanceClock(minutes);
      refresh();
      if (selectedMinute != null) {
        api.getQuote(routeId, selectedMinute, THRESHOLDS).then(setQuote).catch(() => {});
      }
    } catch (e) {
      setError(e.message);
    }
  };

  return (
    <div className="app">
      <header>
        <h1>ShuttlePredict</h1>
        <p className="tagline">
          Confidence-priced NTU shuttle arrival predictions, using derivatives-pricing theory
          (Black-Scholes, Greeks, Monte Carlo) to model and communicate arrival uncertainty.
          <br />
          <strong>Educational project — all prices are simulated confidence units, never real currency.</strong>
        </p>
      </header>

      {error && <div className="error-banner" onClick={() => setError(null)}>{error} (click to dismiss)</div>}

      <div className="controls">
        <label>
          Route:{" "}
          <select value={routeId || ""} onChange={(e) => setRouteId(e.target.value)}>
            {routes.map((r) => (
              <option key={r} value={r}>{r}</option>
            ))}
          </select>
        </label>
        <button onClick={() => handleAdvanceClock(5)}>Advance simulated clock +5 min</button>
        <button onClick={refresh}>Refresh</button>
      </div>

      <main className="layout">
        <section className="panel">
          <h2>Upcoming trips</h2>
          <ScheduleList trips={trips} selectedMinute={selectedMinute} onSelect={setSelectedMinute} />
        </section>

        <section className="panel">
          <h2>Priced confidence contracts</h2>
          <QuotePanel quote={quote} onCreateContract={handleCreateContract} creatingKey={creatingKey} />
        </section>
      </main>

      <section className="panel">
        <h2>Positions</h2>
        <ContractsBlotter contracts={contracts} />
      </section>
    </div>
  );
}
