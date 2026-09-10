import { useEffect, useState } from "react";
import { api, TOKEN_KEY, units, pendingTrade } from "./api";
import MarketBrowse from "./pages/MarketBrowse";
import MarketPage from "./pages/MarketPage";
import Portfolio from "./pages/Portfolio";
import "./App.css";

export default function App() {
  const [config, setConfig] = useState(null);
  const [account, setAccount] = useState(null);
  const [view, setView] = useState("browse");
  const [selected, setSelected] = useState(null);
  const [refresh, setRefresh] = useState(0);
  const [error, setError] = useState("");
  const [name, setName] = useState("");
  const [token, setToken] = useState("");
  const [admin, setAdmin] = useState("");
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    let cancelled = false;
    const load = () => {
      api.config().then((cfg) => { if (!cancelled) setConfig(cfg); }).catch((e) => { if (!cancelled) setError(e.message); });
      const session = localStorage.getItem(TOKEN_KEY);
      if (session) api.me(session).then((owner) => { if (!cancelled && localStorage.getItem(TOKEN_KEY) === session) setAccount(owner); }).catch((e) => { if (!cancelled && localStorage.getItem(TOKEN_KEY) === session) setError(e.message); });
    };
    load(); const timer = setInterval(load, 5000);
    return () => { cancelled = true; clearInterval(timer); };
  }, [refresh]);
  const signIn = async (event, create) => {
    event.preventDefault(); setBusy(true); setError("");
    try {
      if (create) {
        const session = await api.createAccount(name);
        localStorage.setItem(TOKEN_KEY, session.token); setAccount(session.account);
      } else {
        const owner = await api.me(token.trim());
        localStorage.setItem(TOKEN_KEY, token.trim()); setAccount(owner); setToken("");
      }
    } catch (e) { setError(e.message); } finally { setBusy(false); }
  };
  const advance = async (minutes) => {
    setBusy(true); setError("");
    try { await api.advance(minutes, admin); setRefresh((n) => n + 1); }
    catch (e) { setError(e.message); } finally { setBusy(false); }
  };
  const pending = pendingTrade();
  return <div className="app">
    <header className="topbar"><button className="wordmark" onClick={() => setView("browse")}>Poly<span>NTU</span></button>
      <nav aria-label="Main navigation"><button className={view !== "portfolio" ? "active" : ""} onClick={() => setView("browse")}>Markets</button><button className={view === "portfolio" ? "active" : ""} onClick={() => setView("portfolio")}>Portfolio</button></nav>
      {account && <div className="account-chip"><span>{account.display_name}</span><strong>{units(account.balance_micros)} units</strong></div>}
    </header>
    <div className="notice">Academic campus markets · simulated units only{config?.demo_mode ? " · Demo observations" : ""}</div>
    {error && <div className="error-banner" role="alert"><span>{error}</span><button aria-label="Dismiss error" onClick={() => setError("")}>×</button></div>}
    {pending && pending.account_id === account?.id && <div className="notice pending">A trade is awaiting a receipt. <button onClick={() => { setSelected(pending.instance_id); setView("market"); }}>Resume trade</button></div>}
    {pending && pending.account_id !== account?.id && <div className="notice pending">A saved trade belongs to another account. Sign in with that account’s access token to retrieve its receipt.</div>}
    {!account && <section className="panel account-entry" aria-label="Account access">
      <div><h2>Make your forecast count</h2><p className="muted">Sign in to buy and sell outcome shares. Each winning share resolves to one unit.</p></div>
      {config?.demo_mode && <form onSubmit={(e) => signIn(e, true)}><label htmlFor="display-name">Demo display name</label><div className="inline-form"><input id="display-name" value={name} minLength={2} maxLength={60} required onChange={(e) => setName(e.target.value)} placeholder="Your name" /><button className="primary" disabled={busy}>Start with 1,000 units</button></div></form>}
      <details><summary>Use an existing access token</summary><form onSubmit={(e) => signIn(e, false)}><label htmlFor="access-token">Account token</label><div className="inline-form"><input id="access-token" type="password" autoComplete="off" value={token} required onChange={(e) => setToken(e.target.value)} /><button disabled={busy}>Sign in</button></div></form></details>
    </section>}
    {view === "browse" && <MarketBrowse refresh={refresh} onError={setError} onSelect={(id) => { setSelected(id); setView("market"); }} />}
    {view === "market" && selected && <MarketPage key={`${selected}:${account?.id || "guest"}`} id={selected} account={account} refresh={refresh} onTrade={() => setRefresh((n) => n + 1)} onError={setError} onBack={() => setView("browse")} />}
    {view === "portfolio" && <Portfolio account={account} refresh={refresh} onError={setError} onSelect={(id) => { setSelected(id); setView("market"); }} />}
    <footer><span>PolyNTU · Outcome markets</span>{account && <button className="link-button" onClick={() => { localStorage.removeItem(TOKEN_KEY); setAccount(null); }}>Sign out</button>}</footer>
    {account && <details className="account-settings"><summary>Account access</summary><p>Your access token restores this demo account. Keep it private.</p><button onClick={async () => {
      try { await navigator.clipboard.writeText(localStorage.getItem(TOKEN_KEY)); } catch { setError("Clipboard unavailable. Your token remains stored in this browser."); }
    }}>Copy account token</button></details>}
    {config?.demo_mode && <details className="demo-controls"><summary>Demo clock controls</summary><p>Advance simulated time to observe closing and settlement. Administrator access is required.</p><label htmlFor="admin-token">Administrator token</label><input id="admin-token" type="password" value={admin} autoComplete="off" onChange={(e) => setAdmin(e.target.value)} /><div className="button-row"><button disabled={busy || !admin} onClick={() => advance(60)}>Advance 1 hour</button><button disabled={busy || !admin} onClick={() => advance(1440)}>Advance 1 day</button></div></details>}
  </div>;
}
