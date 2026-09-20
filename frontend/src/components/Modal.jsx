import { useEffect } from "react";

/// A centred overlay card over a blurred backdrop, used for the account entry
/// and demo clock dialogs instead of panels that push the page down. Escape
/// and backdrop clicks close it.
export default function Modal({ label, onClose, children }) {
  useEffect(() => {
    const onKey = (event) => { if (event.key === "Escape") onClose(); };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);
  return <div className="modal-backdrop" onClick={onClose}>
    <section className="modal-card" role="dialog" aria-modal="true" aria-label={label} onClick={(event) => event.stopPropagation()}>
      <button className="modal-close" aria-label="Close" onClick={onClose}>×</button>
      {children}
    </section>
  </div>;
}
