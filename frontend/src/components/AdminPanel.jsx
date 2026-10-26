import { useState } from "react";
import Modal from "./Modal";
import { api } from "../api";

/// The administrator console. Every operation goes through one of the
/// `/admin/*` routes, so it needs the shared administrator token or an
/// admin-role session; the token is held in this dialog only and is never
/// persisted.
export default function AdminPanel({ account, onClose, onError, onChanged }) {
  const [token, setToken] = useState("");
  const [busy, setBusy] = useState(false);
  const [requests, setRequests] = useState(null);
  const [reason, setReason] = useState("");
  const [displayName, setDisplayName] = useState("");
  const [message, setMessage] = useState("");
  const [reconcile, setReconcile] = useState(null);

  const run = async (action, success) => {
    setBusy(true); setMessage(""); onError("");
    try { const result = await action(); if (success) setMessage(success); return result; }
    catch (e) { onError(e.message); return null; }
    finally { setBusy(false); }
  };
  const loadRequests = () => run(async () => {
    const rows = await api.adminVerifications("pending", token);
    setRequests(rows);
    return rows;
  }, "Loaded the pending verification queue.");
  const decide = (id, approve) => run(async () => {
    await api.adminDecide(id, { approve, reason: reason.trim() || null }, token);
    setReason("");
    await loadRequests();
    onChanged?.();
  }, approve ? "Approved; the account now holds the creator role." : "Rejected with your recorded reason.");
  const provision = () => run(async () => {
    const created = await api.adminCreateAccount(displayName.trim(), token);
    setDisplayName("");
    onChanged?.();
    return created;
  }, "Provisioned an account with its welcome grant.");
  const checkReconcile = () => run(async () => {
    const report = await api.adminReconcile(token);
    setReconcile(report);
    return report;
  });
  const tick = () => run(async () => {
    const result = await api.adminTick(token);
    onChanged?.();
    return result;
  }, "Ran one settlement pass.");

  return <Modal label="Administrator console" onClose={onClose}>
    <h2>Administrator console</h2>
    <p className="muted small">Use the shared administrator token, or sign in as an administrator account and paste its session token.</p>
    <label htmlFor="console-token">Administrator token</label>
    <input id="console-token" type="password" value={token} autoComplete="off" onChange={(e) => setToken(e.target.value)} />
    {message && <p className="muted small" role="status">{message}</p>}

    <section aria-label="Creator verification queue">
      <h3>Creator verification queue</h3>
      <button disabled={busy || !token} onClick={loadRequests}>Load pending requests</button>
      {requests && requests.length === 0 && <p className="muted small">No pending requests.</p>}
      {requests && requests.length > 0 && <ul className="admin-list">{requests.map((request) => <li key={request.id}>
        <div><strong>{request.display_name || request.account_id}</strong><span className="muted small">Requested {new Date(request.created_ms).toLocaleString("en-SG")}</span></div>
        <div className="button-row">
          <button disabled={busy} onClick={() => decide(request.id, true)}>Approve</button>
          <button disabled={busy} onClick={() => decide(request.id, false)}>Reject</button>
        </div>
      </li>)}</ul>}
      <label htmlFor="decision-reason">Reason (required to reject)</label>
      <input id="decision-reason" value={reason} maxLength={500} onChange={(e) => setReason(e.target.value)} />
    </section>

    <section aria-label="Account provisioning">
      <h3>Provision an account</h3>
      <label htmlFor="provision-name">Display name</label>
      <input id="provision-name" value={displayName} maxLength={80} onChange={(e) => setDisplayName(e.target.value)} />
      <button disabled={busy || !token || !displayName.trim()} onClick={provision}>Create with welcome grant</button>
    </section>

    <section aria-label="Operations">
      <h3>Operations</h3>
      <div className="button-row">
        <button disabled={busy || !token} onClick={tick}>Run settlement pass</button>
        <button disabled={busy || !token} onClick={checkReconcile}>Reconcile</button>
      </div>
      {reconcile && <p className="muted small">{reconcile.ok ? "Reconciliation found no discrepancies." : "Reconciliation found discrepancies; see the report."}</p>}
    </section>
    {account?.role === "admin" && <p className="muted small">Signed in as an administrator; the token field is optional.</p>}
  </Modal>;
}
