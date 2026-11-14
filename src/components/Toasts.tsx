import { AlertCircle, CheckCircle2, Info, X } from "lucide-react";
import { useAppStore } from "../store/app";

export function Toasts() {
  const toasts = useAppStore((state) => state.toasts);
  const dismiss = useAppStore((state) => state.dismissToast);

  return (
    <div className="toast-stack" aria-live="polite">
      {toasts.map((toast) => (
        <div key={toast.id} className={`toast ${toast.level}`}>
          <span className="toast-icon">
            {toast.level === "success" ? (
              <CheckCircle2 size={17} />
            ) : toast.level === "error" ? (
              <AlertCircle size={17} />
            ) : (
              <Info size={17} />
            )}
          </span>
          <p>{toast.message}</p>
          <button className="icon-btn" onClick={() => dismiss(toast.id)} aria-label="Cerrar">
            <X size={14} />
          </button>
        </div>
      ))}
    </div>
  );
}
