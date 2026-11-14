import { useEffect, useMemo, useState } from "react";
import {
  ArrowRight,
  Clapperboard,
  Download,
  HardDrive,
  PlusCircle,
  Share2,
  Sparkles,
  UploadCloud,
  Wrench,
} from "lucide-react";
import { api, StorageStats } from "../lib/api";
import { formatBytes } from "../lib/format";
import { useAppStore } from "../store/app";
import { ProjectCard } from "../components/ProjectCard";
import { Alert, EmptyState, Pill, ProgressBar } from "../components/ui";

export function HomePage() {
  const projects = useAppStore((state) => state.projects);
  const queue = useAppStore((state) => state.queue);
  const tasks = useAppStore((state) => state.tasks);
  const appInfo = useAppStore((state) => state.appInfo);
  const navigate = useAppStore((state) => state.navigate);
  const [stats, setStats] = useState<StorageStats | null>(null);

  useEffect(() => {
    api
      .storageStats()
      .then(setStats)
      .catch(() => setStats(null));
  }, [projects.length, queue?.total]);

  const totals = useMemo(() => {
    const clips = projects.reduce((sum, project) => sum + project.clipCount, 0);
    const published = projects.reduce((sum, project) => sum + project.publishedCount, 0);
    return { clips, published };
  }, [projects]);

  const activeTasks = Object.values(tasks);
  const recent = projects.slice(0, 6);
  const toolIssue = appInfo && (!appInfo.tools.ffmpeg || !appInfo.tools.ffprobe);

  return (
    <div className="page">
      <section className="card" style={{ padding: 28, overflow: "hidden" }}>
        <div
          style={{
            position: "absolute",
            inset: "-60% 55% 30% -10%",
            background: "radial-gradient(closest-side, rgba(110,231,255,0.22), transparent)",
            pointerEvents: "none",
          }}
        />
        <div className="row wrap" style={{ alignItems: "flex-end", gap: 22 }}>
          <div className="grow" style={{ minWidth: 260 }}>
            <span className="pill accent" style={{ marginBottom: 12 }}>
              <Sparkles size={12} /> Del video largo al clip perfecto
            </span>
            <h2 style={{ margin: "10px 0 6px", fontSize: 30, letterSpacing: "-0.03em", lineHeight: 1.15 }}>
              Divide, organiza y publica
              <br />
              sin salir de ParCut.
            </h2>
            <p className="muted" style={{ margin: 0, maxWidth: 520, fontSize: 13 }}>
              Importa o descarga un video, elige la duración de cada parte y publica los clips
              uno por uno en YouTube, Facebook y TikTok con confirmación real de cada plataforma.
            </p>
          </div>
          <div className="row wrap">
            <button className="btn btn-primary btn-lg" onClick={() => navigate("new")}>
              <PlusCircle /> Nuevo proyecto
            </button>
            <button className="btn btn-lg" onClick={() => navigate("clips")}>
              <Clapperboard /> Ver mis clips
            </button>
          </div>
        </div>
      </section>

      <section className="stats-grid">
        <div className="stat">
          <div className="stat-label">
            <Clapperboard size={14} /> Proyectos
          </div>
          <div className="stat-value">{projects.length}</div>
          <div className="stat-hint">{totals.clips} clips generados</div>
        </div>
        <div className="stat">
          <div className="stat-label">
            <Share2 size={14} /> Publicaciones
          </div>
          <div className="stat-value">{totals.published}</div>
          <div className="stat-hint">
            {queue?.pending ? `${queue.pending} en espera` : "Cola al día"}
          </div>
        </div>
        <div className="stat">
          <div className="stat-label">
            <HardDrive size={14} /> Espacio utilizado
          </div>
          <div className="stat-value">{formatBytes(stats?.workspaceBytes ?? 0)}</div>
          <div className="stat-hint">
            {stats?.freeBytes ? `${formatBytes(stats.freeBytes)} libres en disco` : "Espacio de trabajo local"}
          </div>
        </div>
        <div className="stat">
          <div className="stat-label">
            <Download size={14} /> Descargas
          </div>
          <div className="stat-value" style={{ fontSize: 21, marginTop: 10 }}>
            {appInfo ? (appInfo.tools.ytdlp ? "Disponibles" : "Solo importación") : "—"}
          </div>
          <div className="stat-hint">
            {appInfo?.tools.ffmpeg ? "FFmpeg y yt-dlp listos" : "FFmpeg no detectado"}
          </div>
        </div>
      </section>

      {toolIssue ? (
        <Alert tone="warning" icon={<Wrench size={16} />}>
          No se detectó FFmpeg/FFprobe. La importación y la división de video requieren estas
          herramientas. Ejecuta <code className="mono">npm run tools:fetch</code> o revisa la
          sección de herramientas en Configuración.
        </Alert>
      ) : null}

      {activeTasks.length > 0 ? (
        <section className="card">
          <div className="card-header">
            <div className="grow">
              <h3 className="card-title">
                <UploadCloud size={16} /> Tareas en curso
              </h3>
              <p className="card-sub">Los procesos continúan aunque cambies de pantalla.</p>
            </div>
          </div>
          <div className="col" style={{ gap: 14 }}>
            {activeTasks.map((task) => (
              <div key={task.key}>
                <div className="row between" style={{ marginBottom: 6 }}>
                  <span className="small">{task.message}</span>
                  <span className="small mono-num faint">{Math.round(task.percent)}%</span>
                </div>
                <ProgressBar value={task.percent} thin />
              </div>
            ))}
          </div>
        </section>
      ) : null}

      <section>
        <div className="section-head" style={{ marginBottom: 12 }}>
          <div>
            <h2>Proyectos recientes</h2>
            <p>Retoma donde lo dejaste.</p>
          </div>
          <div className="spacer" />
          {projects.length > 0 ? (
            <button className="btn btn-sm btn-ghost" onClick={() => navigate("clips")}>
              Ver todo <ArrowRight size={14} />
            </button>
          ) : null}
        </div>
        {projects.length === 0 ? (
          <EmptyState
            icon={<Clapperboard size={24} />}
            title="Aún no hay proyectos"
            description="Crea tu primer proyecto importando un video local o pegando el enlace de un video autorizado."
            action={
              <button className="btn btn-primary" onClick={() => navigate("new")}>
                <PlusCircle /> Crear el primero
              </button>
            }
          />
        ) : (
          <div className="project-grid">
            {recent.map((project) => (
              <ProjectCard key={project.id} project={project} />
            ))}
          </div>
        )}
      </section>

      {queue && queue.pending > 0 ? (
        <section className="card">
          <div className="row wrap" style={{ justifyContent: "space-between", gap: 14 }}>
            <div>
              <h3 className="card-title">
                <Share2 size={16} /> Cola de publicaciones
              </h3>
              <p className="card-sub" style={{ marginTop: 4 }}>
                {queue.pending} pendientes · {queue.published} publicados ·{" "}
                {queue.failed} con error
              </p>
            </div>
            <div className="row">
              <Pill tone={queue.running ? "accent" : queue.failed ? "danger" : "muted"}>
                {queue.running ? "En curso" : queue.failed ? "En pausa por error" : "En pausa"}
              </Pill>
              <button className="btn btn-sm" onClick={() => navigate("publish")}>
                Abrir seguimiento <ArrowRight size={14} />
              </button>
            </div>
          </div>
        </section>
      ) : null}
    </div>
  );
}
