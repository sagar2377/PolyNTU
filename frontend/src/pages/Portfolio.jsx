import { useEffect, useState } from "react";
import { api } from "../api";
import ContractsBlotter from "../components/ContractsBlotter";

export default function Portfolio({ refreshTick, onError }) {
  const [contracts, setContracts] = useState([]);

  useEffect(() => {
    api.getPortfolio().then(setContracts).catch((e) => onError(e.message));
  }, [refreshTick, onError]);

  return (
    <div>
      <h2 className="market-page-title">Portfolio</h2>
      <p className="muted">All positions across every market — simulated confidence units, not real currency.</p>
      <section className="panel">
        <ContractsBlotter contracts={contracts} showMarket />
      </section>
    </div>
  );
}
