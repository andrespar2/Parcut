import {
  Clapperboard,
  FolderOpen,
  Globe,
  Home,
  PlusCircle,
  Settings,
  Share2,
  Users,
} from "lucide-react";
import { useAppStore, View } from "../store/app";

const NAV: { view: View; label: string; icon: typeof Home }[] = [
  { view: "home", label: "Inicio", icon: Home },
  { view: "new", label: "Nuevo proyecto", icon: PlusCircle },
  { view: "browser", label: "Navegador", icon: Globe },
  { view: "clips", label: "Mis clips", icon: Clapperboard },
  { view: "publish", label: "Publicaciones", icon: Share2 },
  { view: "accounts", label: "Cuentas conectadas", icon: Users },
];

export function LogoMark({ size = 24 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 32 32" aria-hidden>
      <defs>
        <linearGradient id="pc-mark" x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" stopColor="#5ee7ff" />
          <stop offset="0.55" stopColor="#7f8cff" />
          <stop offset="1" stopColor="#c86bff" />
        </linearGradient>
      </defs>
      <path d="M7 4.5 14.4 8.9v14.2L7 27.5Z" fill="url(#pc-mark)" />
      <path d="M16.3 8.9 25 16l-8.7 7.1Z" fill="url(#pc-mark)" />
      <rect x="14.6" y="5" width="1.5" height="22" rx="0.75" fill="#05060a" opacity="0.9" />
    </svg>
  );
}

export function Sidebar() {
  const view = useAppStore((state) => state.view);
  const navigate = useAppStore((state) => state.navigate);
  const queue = useAppStore((state) => state.queue);
  const pending = queue?.pending ?? 0;

  return (
    <aside className="sidebar">
      <div className="brand">
        <span className="brand-mark">
          <LogoMark size={26} />
        </span>
        <div className="brand-text">
          <span className="brand-name">ParCut</span>
          <span className="brand-tag">Estudio de clips</span>
        </div>
      </div>

      <nav className="nav">
        <div className="nav-section">Espacio de trabajo</div>
        {NAV.map((item) => {
          const Icon = item.icon;
          const active = view === item.view;
          return (
            <button
              key={item.view}
              className={`nav-item${active ? " active" : ""}`}
              onClick={() => navigate(item.view)}
              title={item.label}
            >
              <Icon />
              <span>{item.label}</span>
              {item.view === "publish" && pending > 0 ? (
                <span className="nav-badge">{pending}</span>
              ) : null}
            </button>
          );
        })}

        <div className="nav-section">Sistema</div>
        <button
          className={`nav-item${view === "settings" ? " active" : ""}`}
          onClick={() => navigate("settings")}
          title="Configuración"
        >
          <Settings />
          <span>Configuración</span>
        </button>
      </nav>

      <div className="sidebar-footer">
        <button className="btn btn-ghost" onClick={() => navigate("new")} title="Nuevo proyecto">
          <PlusCircle />
          <span>Nuevo proyecto</span>
        </button>
        <div className="credit">
          <FolderOpen size={12} />
          <span>
            Desarrollado por <strong>andrespar2</strong>
          </span>
        </div>
      </div>
    </aside>
  );
}
