import { useCallback, useEffect, useState } from "react";
import { api } from "./api";
import MarketBrowse from "./pages/MarketBrowse";
import MarketPage from "./pages/MarketPage";
import Portfolio from "./pages/Portfolio";
import "./App.css";

export default function App() {
  const [markets, setMarkets] = useState([]);
  const [view, setView] = useState("browse"); // 'browse' | 'market' | 'portfolio'
  const [selectedMarketId, setSelectedMarketId] = useState(null);
  const [refreshTick, setRefreshTick] = useState(0);
  const [error, setError] = useState(null);

  const loadMarkets = useCallback(() => {
    api.listMarkets().then(setMarkets).catch((e) => setError(e.message));
  }, []);

  useEffect(() => {
    loadMarkets();
  }, [loadMarkets]);

  const handleAdvanceClock = async (minutes) => {
    try {
      await api.advanceClock(minutes);
      setRefreshTick((t) => t + 1);
    } catch (e) {
      setError(e.message);
    }
  };

  const selectedMarket = markets.find((m) => m.id === selectedMarketId);

  return (
    <div className="app">
      <header>
        <h1>PolyNTU</h1>
        <p className="tagline">
          An academic prediction-market platform for the NTU campus: confidence-priced outcomes
          across several campus market types, using derivatives-pricing theory (Black-Scholes,
          Greeks, Monte Carlo) to model and communicate uncertainty.
          <br />
          <strong>Educational project — all prices are simulated confidence units, never real currency.</strong>
        </p>
      </header>

      {error && <div className="error-banner" onClick={() => setError(null)}>{error} (click to dismiss)</div>}

      <div className="controls">
        <button onClick={() => setView("browse")}>Browse markets</button>
        <button onClick={() => setView("portfolio")}>Portfolio</button>
        <span className="spacer" />
        <button onClick={() => handleAdvanceClock(5)}>Advance clock +5 min</button>
        <button onClick={() => handleAdvanceClock(60)}>+1 hour</button>
        <button onClick={() => handleAdvanceClock(24 * 60)}>+1 day</button>
      </div>

      {view === "browse" && (
        <MarketBrowse markets={markets} onSelect={(id) => { setSelectedMarketId(id); setView("market"); }} />
      )}

      {view === "market" && selectedMarket && (
        <MarketPage market={selectedMarket} refreshTick={refreshTick}
                    onBack={() => setView("browse")} onError={setError} />
      )}

      {view === "portfolio" && (
        <Portfolio refreshTick={refreshTick} onError={setError} />
      )}
    </div>
  );
}
