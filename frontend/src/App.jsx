import { useEffect, useState } from "react";
import { api, TOKEN_KEY, deriveResolutionKeyPair, pendingTrade, storeSigningKey, units } from "./api";
import MarketBrowse from "./pages/MarketBrowse";
import MarketPage from "./pages/MarketPage";
import Portfolio from "./pages/Portfolio";
import CreateMarket from "./pages/CreateMarket";
import "./App.css";

export default function App() {
  const [config, setConfig] = useState(null);
  const [account, setAccount] = useState(null);
  const [view, setView] = useState("browse");
  const [selected, setSelected] = useState(null);
  const [marketFrom, setMarketFrom] = useState("browse");
  const [refresh, setRefresh] = useState(0);
  const [error, setError] = useState("");
  const [name, setName] = useState("");
  const [entryView, setEntryView] = useState(null);
  const [admin, setAdmin] = useState("");
  const [busy, setBusy] = useState(false);
  const [regName, setRegName] = useState("");
  const [regEmail, setRegEmail] = useState("");
  const [regPassword, setRegPassword] = useState("");
  const [loginEmail, setLoginEmail] = useState("");
  const [loginPassword, setLoginPassword] = useState("");
  const [verification, setVerification] = useState(null);
  useEffect(() => {
    let cancelled = false;
    const load = () => {
      api.config().then((cfg) => { if (!cancelled) setConfig(cfg); }).catch((e) => { if (!cancelled) setError(e.message); });
      const session = localStorage.getItem(TOKEN_KEY);
      if (session) api.me(session).then((owner) => { if (!cancelled && localStorage.getItem(TOKEN_KEY) === session) setAccount(owner); }).catch((e) => {
        if (cancelled || localStorage.getItem(TOKEN_KEY) !== session) return;
        // A rotated-out session token (another login) signs this browser out
        // instead of erroring on every poll.
        if (e.status === 401) { localStorage.removeItem(TOKEN_KEY); setAccount(null); } else setError(e.message);
      });
    };
    load(); const timer = setInterval(load, 5000);
    return () => { cancelled = true; clearInterval(timer); };
  }, [refresh]);
  useEffect(() => {
    if (!account?.id) return;
    api.verificationRequest().then((request) => { setVerification(request && request.id ? request : null); }).catch(() => setVerification(null));
  }, [account?.id, account?.role, refresh]);
  const openSession = (session, password) => {
    localStorage.setItem(TOKEN_KEY, session.token);
    setAccount(session.account);
    setVerification(null);
    setEntryView(null);
    // Cache the resolution signing key derived from the password (ADR 0007
    // amendment) so publishing and resolving never need to re-prompt while
    // this browser stays signed in. Best effort: a failure just means the
    // password is asked for where the key is needed.
    if (session.account?.email && password) {
      deriveResolutionKeyPair(session.account.email, password)
        .then((keys) => storeSigningKey(session.account.email, keys))
        .catch(() => {});
    }
  };
  const signIn = async (event, mode) => {
    event.preventDefault(); setBusy(true); setError("");
    try {
      if (mode === "register") openSession(await api.register({ display_name: regName, email: regEmail, password: regPassword }), regPassword);
      else if (mode === "login") openSession(await api.login({ email: loginEmail, password: loginPassword }), loginPassword);
      else openSession(await api.createAccount(name));
    } catch (e) { setError(e.message); } finally { setBusy(false); }
  };
  const signInDemoAdmin = async () => {
    setBusy(true); setError("");
    try { openSession(await api.login({ email: "admin@ntu.edu.sg", password: "admin" })); }
    catch (e) { setError(e.message); } finally { setBusy(false); }
  };
  const requestVerification = async () => {
    setBusy(true); setError("");
    try { setVerification(await api.requestVerification()); setRefresh((n) => n + 1); }
    catch (e) { setError(e.message); } finally { setBusy(false); }
  };
  const advance = async (minutes) => {
    setBusy(true); setError("");
    try { await api.advance(minutes, admin); setRefresh((n) => n + 1); }
    catch (e) { setError(e.message); } finally { setBusy(false); }
  };
  const signOut = () => {
    localStorage.removeItem(TOKEN_KEY);
    setAccount(null);
    setVerification(null);
  };
  // A market page's back button returns to the list it was opened from, so
  // bracket-to-bracket navigation never stacks layers of back.
  const openMarket = (instanceId, from = "browse") => {
    setSelected(instanceId); setMarketFrom(from); setView("market");
  };
  // Every bracket page shows its whole series, so opening a series means
  // opening one of its brackets; a just-published recurring series may need a
  // moment before its first bracket spawns.
  const openSeries = async (seriesId) => {
    try {
      let target;
      for (let attempt = 0; attempt < 5 && !target; attempt++) {
        if (attempt) await new Promise((resolve) => setTimeout(resolve, 800));
        const detail = await api.series(seriesId);
        target = detail.instances.find((i) => i.state === "open") ?? detail.instances[0];
      }
      if (target) openMarket(target.id, "browse"); else setView("browse");
    } catch (e) { setError(e.message); }
  };
  const pending = pendingTrade();
  return <div className="app">
    <header className="topbar"><button className="wordmark" onClick={() => setView("browse")}>Poly<span>NTU</span></button>
      <nav aria-label="Main navigation"><button className={view !== "portfolio" && view !== "create" ? "active" : ""} onClick={() => setView("browse")}>Markets</button><button className={view === "portfolio" ? "active" : ""} onClick={() => setView("portfolio")}>Portfolio</button>{account?.role === "creator" && <button className={view === "create" ? "active" : ""} onClick={() => setView("create")}>Create market</button>}</nav>
      {account
        ? <div className="account-chip"><span>{account.display_name}{account.role ? ` · ${account.role}` : ""}</span><strong>{units(account.balance_micros)} units</strong></div>
        : <div className="signin-toggle" aria-label="Account access">
            <button className={entryView === "register" ? "active" : ""} onClick={() => setEntryView(entryView === "register" ? null : "register")}>Create account</button>
            <button className={entryView === "login" ? "active" : ""} onClick={() => setEntryView(entryView === "login" ? null : "login")}>Log in</button>
          </div>}
    </header>
    <div className="notice">Academic campus markets · simulated units only{config?.demo_mode ? " · Demo observations" : ""}</div>
    {error && <div className="error-banner" role="alert"><span>{error}</span><button aria-label="Dismiss error" onClick={() => setError("")}>×</button></div>}
    {pending && pending.account_id === account?.id && <div className="notice pending">A trade is awaiting a receipt. <button onClick={() => openMarket(pending.instance_id, "browse")}>Resume trade</button></div>}
    {pending && pending.account_id !== account?.id && <div className="notice pending">A saved trade belongs to another account. Sign in with that account to retrieve its receipt.</div>}
    {!account && entryView && <section className="panel signin-panel" aria-label="Account access">
      {entryView === "register" ? <>
        <h2>Create your account</h2>
        <p className="muted small">An NTU email is required. Each winning share resolves to one unit; new accounts start with 10,000 units.</p>
        <form onSubmit={(e) => signIn(e, "register")} className="stacked-form">
          <label htmlFor="reg-name">Display name</label>
          <input id="reg-name" value={regName} minLength={2} maxLength={60} required onChange={(e) => setRegName(e.target.value)} placeholder="Your name" />
          <label htmlFor="reg-email">NTU email</label>
          <input id="reg-email" type="email" value={regEmail} required onChange={(e) => setRegEmail(e.target.value)} placeholder="name@ntu.edu.sg" />
          <label htmlFor="reg-password">Password</label>
          <input id="reg-password" type="password" value={regPassword} minLength={12} required autoComplete="new-password" onChange={(e) => setRegPassword(e.target.value)} placeholder="At least 12 characters" />
          <button className="primary" disabled={busy}>Create account</button>
        </form>
      </> : <>
        <h2>Log in</h2>
        <form onSubmit={(e) => signIn(e, "login")} className="stacked-form">
          <label htmlFor="login-email">NTU email</label>
          <input id="login-email" type="email" value={loginEmail} required autoComplete="username" onChange={(e) => setLoginEmail(e.target.value)} />
          <label htmlFor="login-password">Password</label>
          <input id="login-password" type="password" value={loginPassword} required autoComplete="current-password" onChange={(e) => setLoginPassword(e.target.value)} />
          <button disabled={busy}>Log in</button>
        </form>
      </>}
      {config?.demo_mode && <details><summary>Demo accounts</summary>
        <p className="muted small">A one-click participant with 1,000 units, or the seeded administrator.</p>
        <form onSubmit={(e) => signIn(e, "demo")}><label htmlFor="display-name">Demo display name</label><div className="inline-form"><input id="display-name" value={name} minLength={2} maxLength={60} required onChange={(e) => setName(e.target.value)} placeholder="Your name" /><button disabled={busy}>Start with 1,000 units</button></div></form>
        <div className="button-row"><button disabled={busy} onClick={signInDemoAdmin}>Sign in as admin@ntu.edu.sg</button></div>
      </details>}
    </section>}
    {account?.role === "member" && <section className="panel" aria-label="Creator verification">
      <h3>Become a market creator</h3>
      {verification?.status === "pending" ? <p className="muted">Your creator verification is pending administrator review.</p>
        : verification?.status === "rejected" ? <div><p className="muted">Your last request was rejected: {verification.reason}</p><button disabled={busy} onClick={requestVerification}>Apply again</button></div>
        : <div><p className="muted">Verified creators publish their own markets and earn half of every trading fee. Request verification to get started.</p><button className="primary" disabled={busy} onClick={requestVerification}>Request creator verification</button></div>}
    </section>}
    {view === "browse" && <MarketBrowse refresh={refresh} onError={setError} onSelect={(id) => openMarket(id, "browse")} onOpenSeries={openSeries} />}
    {view === "market" && selected && <MarketPage key={`${selected}:${account?.id || "guest"}`} id={selected} account={account} refresh={refresh} onTrade={() => setRefresh((n) => n + 1)} onError={setError} onBack={() => setView(marketFrom)} onSelect={(id) => openMarket(id, marketFrom)} />}
    {view === "create" && account?.role === "creator" && <CreateMarket account={account} onCreated={(id) => { setRefresh((n) => n + 1); openSeries(id); }} onError={setError} onBack={() => setView("browse")} />}
    {view === "portfolio" && <Portfolio account={account} refresh={refresh} onError={setError} onSelect={(id) => openMarket(id, "portfolio")} />}
    <footer><span>PolyNTU · Outcome markets</span>{account && <button className="link-button" onClick={signOut}>Sign out</button>}</footer>
    {account && <details className="account-settings"><summary>Account access</summary><p>Logging in again invalidates every other session. Registered accounts simply log in again; a demo account cannot sign back in after signing out, so create a new one instead.</p></details>}
    {config?.demo_mode && <details className="demo-controls"><summary>Demo clock controls</summary><p>Advance simulated time to observe closing and settlement. Administrator access is required.</p><label htmlFor="admin-token">Administrator token</label><input id="admin-token" type="password" value={admin} autoComplete="off" onChange={(e) => setAdmin(e.target.value)} /><div className="button-row"><button disabled={busy || !admin} onClick={() => advance(60)}>Advance 1 hour</button><button disabled={busy || !admin} onClick={() => advance(1440)}>Advance 1 day</button></div></details>}
  </div>;
}
