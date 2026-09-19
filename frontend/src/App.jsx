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
  const openSession = (session) => {
    localStorage.setItem(TOKEN_KEY, session.token);
    setAccount(session.account);
    setVerification(null);
  };
  const signIn = async (event, mode) => {
    event.preventDefault(); setBusy(true); setError("");
    try {
      if (mode === "register") openSession(await api.register({ display_name: regName, email: regEmail, password: regPassword }));
      else if (mode === "login") openSession(await api.login({ email: loginEmail, password: loginPassword }));
      else if (mode === "demo") openSession(await api.createAccount(name));
      else { const owner = await api.me(token.trim()); localStorage.setItem(TOKEN_KEY, token.trim()); setAccount(owner); setVerification(null); }
      setToken("");
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
  const copyToken = async () => {
    try { await navigator.clipboard.writeText(localStorage.getItem(TOKEN_KEY)); } catch { setError("Clipboard unavailable. Your token remains stored in this browser."); }
  };
  const pending = pendingTrade();
  return <div className="app">
    <header className="topbar"><button className="wordmark" onClick={() => setView("browse")}>Poly<span>NTU</span></button>
      <nav aria-label="Main navigation"><button className={view !== "portfolio" ? "active" : ""} onClick={() => setView("browse")}>Markets</button><button className={view === "portfolio" ? "active" : ""} onClick={() => setView("portfolio")}>Portfolio</button></nav>
      {account && <div className="account-chip"><span>{account.display_name}{account.role ? ` · ${account.role}` : ""}</span><strong>{units(account.balance_micros)} units</strong></div>}
    </header>
    <div className="notice">Academic campus markets · simulated units only{config?.demo_mode ? " · Demo observations" : ""}</div>
    {error && <div className="error-banner" role="alert"><span>{error}</span><button aria-label="Dismiss error" onClick={() => setError("")}>×</button></div>}
    {pending && pending.account_id === account?.id && <div className="notice pending">A trade is awaiting a receipt. <button onClick={() => { setSelected(pending.instance_id); setView("market"); }}>Resume trade</button></div>}
    {pending && pending.account_id !== account?.id && <div className="notice pending">A saved trade belongs to another account. Sign in with that account to retrieve its receipt.</div>}
    {!account && <section className="panel account-entry" aria-label="Account access">
      <div><h2>Make your forecast count</h2><p className="muted">Sign in to buy and sell outcome shares. Each winning share resolves to one unit. New NTU accounts start with 10,000 units.</p></div>
      <form onSubmit={(e) => signIn(e, "register")} className="stacked-form">
        <label htmlFor="reg-name">Display name</label>
        <input id="reg-name" value={regName} minLength={2} maxLength={60} required onChange={(e) => setRegName(e.target.value)} placeholder="Your name" />
        <label htmlFor="reg-email">NTU email</label>
        <input id="reg-email" type="email" value={regEmail} required onChange={(e) => setRegEmail(e.target.value)} placeholder="name@ntu.edu.sg" />
        <label htmlFor="reg-password">Password</label>
        <input id="reg-password" type="password" value={regPassword} minLength={12} required autoComplete="new-password" onChange={(e) => setRegPassword(e.target.value)} placeholder="At least 12 characters" />
        <button className="primary" disabled={busy}>Create account</button>
      </form>
      <details><summary>Log in with email and password</summary><form onSubmit={(e) => signIn(e, "login")} className="stacked-form">
        <label htmlFor="login-email">NTU email</label>
        <input id="login-email" type="email" value={loginEmail} required autoComplete="username" onChange={(e) => setLoginEmail(e.target.value)} />
        <label htmlFor="login-password">Password</label>
        <input id="login-password" type="password" value={loginPassword} required autoComplete="current-password" onChange={(e) => setLoginPassword(e.target.value)} />
        <button disabled={busy}>Log in</button>
      </form></details>
      {config?.demo_mode && <details><summary>Demo: one-click participant</summary><form onSubmit={(e) => signIn(e, "demo")}><label htmlFor="display-name">Demo display name</label><div className="inline-form"><input id="display-name" value={name} minLength={2} maxLength={60} required onChange={(e) => setName(e.target.value)} placeholder="Your name" /><button disabled={busy}>Start with 1,000 units</button></div></form></details>}
      {config?.demo_mode && <details><summary>Demo: administrator</summary><p>Sign in as the seeded administrator account.</p><button disabled={busy} onClick={signInDemoAdmin}>Sign in as admin@ntu.edu.sg</button></details>}
      <details><summary>Use an existing access token</summary><form onSubmit={(e) => signIn(e, "token")}><label htmlFor="access-token">Account token</label><div className="inline-form"><input id="access-token" type="password" autoComplete="off" value={token} required onChange={(e) => setToken(e.target.value)} /><button disabled={busy}>Sign in</button></div></form></details>
    </section>}
    {account?.role === "member" && <section className="panel" aria-label="Creator verification">
      <h3>Become a market creator</h3>
      {verification?.status === "pending" ? <p className="muted">Your creator verification is pending administrator review.</p>
        : verification?.status === "rejected" ? <div><p className="muted">Your last request was rejected: {verification.reason}</p><button disabled={busy} onClick={requestVerification}>Apply again</button></div>
        : <div><p className="muted">Verified creators publish their own markets and earn half of every trading fee. Request verification to get started.</p><button className="primary" disabled={busy} onClick={requestVerification}>Request creator verification</button></div>}
    </section>}
    {view === "browse" && <MarketBrowse refresh={refresh} onError={setError} onSelect={(id) => { setSelected(id); setView("market"); }} />}
    {view === "market" && selected && <MarketPage key={`${selected}:${account?.id || "guest"}`} id={selected} account={account} refresh={refresh} onTrade={() => setRefresh((n) => n + 1)} onError={setError} onBack={() => setView("browse")} />}
    {view === "portfolio" && <Portfolio account={account} refresh={refresh} onError={setError} onSelect={(id) => { setSelected(id); setView("market"); }} />}
    <footer><span>PolyNTU · Outcome markets</span>{account && <button className="link-button" onClick={signOut}>Sign out</button>}</footer>
    {account && <details className="account-settings"><summary>Account access</summary><p>Logging in again invalidates every other session. Demo accounts restore only through their token; registered accounts simply log in again.</p><button onClick={copyToken}>Copy account token</button></details>}
    {config?.demo_mode && <details className="demo-controls"><summary>Demo clock controls</summary><p>Advance simulated time to observe closing and settlement. Administrator access is required.</p><label htmlFor="admin-token">Administrator token</label><input id="admin-token" type="password" value={admin} autoComplete="off" onChange={(e) => setAdmin(e.target.value)} /><div className="button-row"><button disabled={busy || !admin} onClick={() => advance(60)}>Advance 1 hour</button><button disabled={busy || !admin} onClick={() => advance(1440)}>Advance 1 day</button></div></details>}
  </div>;
}
