import { Activity, Moon, Settings, Sun } from "lucide-react";
import { activeTaskCount, useAppStore, View } from "../store/app";

const TITLES: Record<View, { title: string; subtitle: string }> = {
  home: { title: "Inicio", subtitle: "Tu espacio de trabajo" },
  new: { title: "Nuevo proyecto", subtitle: "Descarga o importa un video para empezar" },
  project: { title: "Proyecto", subtitle: "Detalles, división y clips" },
  clips: { title: "Mis clips", subtitle: "Todos los fragmentos generados" },
  publish: { title: "Publicaciones", subtitle: "Cola secuencial y confirmación por plataforma" },
  accounts: { title: "Cuentas conectadas", subtitle: "Autorización oficial de YouTube, Facebook y TikTok" },
  settings: { title: "Configuración", subtitle: "Almacenamiento, calidad y preferencias" },
};

export function TopBar() {
  const view = useAppStore((state) => state.view);
  const projectId = useAppStore((state) => state.projectId);
  const projects = useAppStore((state) => state.projects);
  const tasks = useAppStore((state) => state.tasks);
  const queue = useAppStore((state) => state.queue);
  const settings = useAppStore((state) => state.settings);
  const saveSettings = useAppStore((state) => state.saveSettings);
  const navigate = useAppStore((state) => state.navigate);

  const project = projectId ? projects.find((item) => item.id === projectId) : undefined;
  const base = TITLES[view];
  const title = view === "project" && project ? project.name : base.title;
  const subtitle =
    view === "project" && project
      ? `${project.clipCount} clips · ${project.platform ?? "archivo local"}`
      : base.subtitle;

  const count = activeTaskCount({ tasks, queue });
  const taskLabel =
    queue?.running && queue.activeItemId
      ? "Publicando en cola"
      : Object.values(tasks)[0]?.message ?? "Tareas activas";

  const toggleTheme = async () => {
    if (!settings) return;
    const next = settings.theme === "light" ? "dark" : "light";
    try {
      await saveSettings({ ...settings, theme: next });
    } catch {
      /* el toast lo gestiona el store */
    }
  };

  return (
    <header className="topbar">
      <div className="topbar-title">
        <h1>{title}</h1>
        <span>{subtitle}</span>
      </div>
      <div className="topbar-actions">
        <button
          className={`tasks-indicator${count === 0 ? " idle" : ""}`}
          onClick={() => navigate(queue?.running ? "publish" : "clips")}
          title={taskLabel}
        >
          <span className="pulse" />
          <Activity size={14} />
          <span>{count > 0 ? `${count} en curso` : "Sin tareas activas"}</span>
        </button>
        <button className="icon-btn" onClick={toggleTheme} title="Cambiar tema">
          {settings?.theme === "light" ? <Moon /> : <Sun />}
        </button>
        <button className="icon-btn" onClick={() => navigate("settings")} title="Configuración">
          <Settings />
        </button>
      </div>
    </header>
  );
}
