import { ReactNode, useEffect } from "react";
import { X } from "lucide-react";

/* --------------------------------------------------------------- Progress */

export function ProgressBar({
  value,
  thin,
}: {
  value: number;
  thin?: boolean;
}) {
  return (
    <div className={`progress${thin ? " thin" : ""}`}>
      <span style={{ width: `${Math.max(0, Math.min(100, value))}%` }} />
    </div>
  );
}

/* ---------------------------------------------------------------- Empty */

export function EmptyState({
  icon,
  title,
  description,
  action,
}: {
  icon: ReactNode;
  title: string;
  description: string;
  action?: ReactNode;
}) {
  return (
    <div className="empty">
      <div className="empty-icon">{icon}</div>
      <h3>{title}</h3>
      <p>{description}</p>
      {action}
    </div>
  );
}

/* ---------------------------------------------------------------- Modal */

export function Modal({
  open,
  title,
  subtitle,
  children,
  actions,
  onClose,
  wide,
  busy,
}: {
  open: boolean;
  title: string;
  subtitle?: string;
  children?: ReactNode;
  actions?: ReactNode;
  onClose: () => void;
  wide?: boolean;
  busy?: boolean;
}) {
  useEffect(() => {
    if (!open) return;
    const handler = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !busy) onClose();
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [open, busy, onClose]);

  if (!open) return null;
  return (
    <div
      className="overlay"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget && !busy) onClose();
      }}
    >
      <div className={`modal${wide ? " wide" : ""}`} role="dialog" aria-modal="true">
        <div className="modal-head">
          <div className="grow">
            <h2>{title}</h2>
            {subtitle ? <p>{subtitle}</p> : null}
          </div>
          <button className="icon-btn" onClick={onClose} disabled={busy} aria-label="Cerrar">
            <X />
          </button>
        </div>
        {children}
        {actions ? <div className="modal-actions">{actions}</div> : null}
      </div>
    </div>
  );
}

/* ---------------------------------------------------------------- Pill */

export function Pill({
  tone = "default",
  children,
}: {
  tone?: "default" | "accent" | "success" | "warning" | "danger" | "muted";
  children: ReactNode;
}) {
  return <span className={`pill ${tone === "default" ? "" : tone}`}>{children}</span>;
}

/* -------------------------------------------------------------- Segmented */

export function Segmented<T extends string>({
  value,
  options,
  onChange,
}: {
  value: T;
  options: { value: T; label: string; icon?: ReactNode }[];
  onChange: (value: T) => void;
}) {
  return (
    <div className="segmented" role="tablist">
      {options.map((option) => (
        <button
          key={option.value}
          role="tab"
          aria-selected={value === option.value}
          className={value === option.value ? "active" : ""}
          onClick={() => onChange(option.value)}
        >
          {option.icon}
          {option.label}
        </button>
      ))}
    </div>
  );
}

/* ------------------------------------------------------------- Switch row */

export function SwitchRow({
  label,
  hint,
  checked,
  onChange,
}: {
  label: string;
  hint?: string;
  checked: boolean;
  onChange: (value: boolean) => void;
}) {
  return (
    <div className="row between" style={{ gap: 18, alignItems: "flex-start" }}>
      <div className="col" style={{ gap: 2 }}>
        <span style={{ fontSize: 13, fontWeight: 550 }}>{label}</span>
        {hint ? <span className="field-hint">{hint}</span> : null}
      </div>
      <button
        type="button"
        className={`switch${checked ? " on" : ""}`}
        role="switch"
        aria-checked={checked}
        aria-label={label}
        onClick={() => onChange(!checked)}
      />
    </div>
  );
}

/* ---------------------------------------------------------------- Alert */

export function Alert({
  tone = "info",
  icon,
  children,
}: {
  tone?: "info" | "warning" | "danger" | "success";
  icon?: ReactNode;
  children: ReactNode;
}) {
  return (
    <div className={`alert${tone === "info" ? "" : ` ${tone}`}`}>
      {icon}
      <div>{children}</div>
    </div>
  );
}
