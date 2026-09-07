import { useCallback, useEffect, useState } from "react";
import { api } from "../api";
import InstanceList from "../components/InstanceList";
import QuotePanel from "../components/QuotePanel";
import ContractsBlotter from "../components/ContractsBlotter";

const THRESHOLD_PRESETS = {
  shuttle_arrival: [0, 5, 10],
  student_election: [40, 50, 60],
};

export default function MarketPage({ market, refreshTick, onBack, onError }) {
  const [instances, setInstances] = useState([]);
  const [selectedInstanceId, setSelectedInstanceId] = useState(null);
  const [quote, setQuote] = useState(null);
  const [contracts, setContracts] = useState([]);
  const [creatingKey, setCreatingKey] = useState(null);

  const thresholds = THRESHOLD_PRESETS[market.market_type] || [0, 5, 10];

  const refresh = useCallback(() => {
    api.getInstances(market.id).then(setInstances).catch((e) => onError(e.message));
    api.listContracts(market.id).then(setContracts).catch((e) => onError(e.message));
  }, [market.id, onError]);

  useEffect(() => {
    refresh();
    setSelectedInstanceId(null);
    setQuote(null);
  }, [market.id, refresh]);

  useEffect(() => {
    refresh();
  }, [refreshTick, refresh]);

  useEffect(() => {
    if (selectedInstanceId != null) {
      api.getQuote(market.id, selectedInstanceId, thresholds).then(setQuote).catch((e) => onError(e.message));
    }
  }, [market.id, selectedInstanceId, refreshTick]);

  const handleCreateContract = async (type, threshold) => {
    const key = `${selectedInstanceId}-${threshold}-${type}`;
    setCreatingKey(key);
    try {
      await api.createContract(market.id, selectedInstanceId, type, threshold);
      await api.listContracts(market.id).then(setContracts);
    } catch (e) {
      onError(e.message);
    } finally {
      setCreatingKey(null);
    }
  };

  return (
    <div>
      <button className="link-button" onClick={onBack}>&larr; Back to markets</button>
      <h2 className="market-page-title">{market.title}</h2>
      <p className="muted resolution-criterion">{market.resolution_criterion}</p>

      <main className="layout">
        <section className="panel">
          <h2>Upcoming resolution events</h2>
          <InstanceList instances={instances} unitLabel={market.unit_label}
                        selectedId={selectedInstanceId} onSelect={setSelectedInstanceId} />
        </section>

        <section className="panel">
          <h2>Priced confidence contracts</h2>
          <QuotePanel quote={quote} onCreateContract={handleCreateContract} creatingKey={creatingKey} />
        </section>
      </main>

      <section className="panel">
        <h2>Positions on this market</h2>
        <ContractsBlotter contracts={contracts} />
      </section>
    </div>
  );
}
