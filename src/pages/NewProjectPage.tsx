import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import {
  ArrowRight,
  Link2,
  Loader2,
  ShieldCheck,
  Upload,
  Video,
  X,
} from "lucide-react";
import { api, DownloadMetadata, errorMessage } from "../lib/api";
import { formatBytes, formatDuration, isValidHttpUrl } from "../lib/format";
import { useAppStore } from "../store/app";
import { Alert, ProgressBar, Segmented } from "../components/ui";

const VIDEO_EXTENSIONS = [
  "mp4",
  "mov",
  "mkv",
  "webm",
  "avi",
  "m4v",
  "mpg",
  "mpeg",
  "wmv",
  "ts",
];

export function NewProjectPage() {
  const settings = useAppStore((state) => state.settings);
  const appInfo = useAppStore((state) => state.appInfo);
  const navigate = useAppStore((state) => state.navigate);
  const refreshAll = useAppStore((state) => state.refreshAll);
  const toast = useAppStore((state) => state.toast);
  const setTask = useAppStore((state) => state.setTask);
  const clearProjectTasks = useAppStore((state) => state.clearProjectTasks);

  const [mode, setMode] = useState<"download" | "import">("download");
  const [url, setUrl] = useState("");
  const [analyzing, setAnalyzing] = useState(false);
  const [metadata, setMetadata] = useState<DownloadMetadata | null>(null);
  const [quality, setQuality] = useState("best");
  const [error, setError] = useState<string | null>(null);
  const [downloading, setDownloading] = useState(false);
  const [importing, setImporting] = useState(false);
  const [dragActive, setDragActive] = useState(false);
  const [progress, setProgress] = useState<{ percent: number; message: string } | null>(null);

  const activeProject = useRef<string | null>(null);

  useEffect(() => {
    const unlisteners = [
      listen<{ projectId: string; percent: number; stage: string; message: string }>(
        "download:progress",
        (event) => {
          if (event.payload.projectId !== activeProject.current) return;
          setProgress({ percent: event.payload.percent, message: event.payload.message });
          setTask({
            key: `download:${event.payload.projectId}`,
            kind: "download",
            projectId: event.payload.projectId,
            percent: event.payload.percent,
            message: event.payload.message,
          });
        },
      ),
      listen<{ project: { id: string; name: string } }>("download:done", (event) => {
        if (event.payload.project.id !== activeProject.current) return;
        clearProjectTasks(event.payload.project.id);
        activeProject.current = null;
        setDownloading(false);
        setProgress(null);
        toast("success", "Video descargado y listo para dividir.");
        void refreshAll();
        navigate("project", { projectId: event.payload.project.id });
      }),
      listen<{ projectId: string; message: string }>("download:error", (event) => {
        if (event.payload.projectId !== activeProject.current) return;
        clearProjectTasks(event.payload.projectId);
        activeProject.current = null;
        setDownloading(false);
        setProgress(null);
        setError(event.payload.message);
        toast("error", event.payload.message);
        void refreshAll();
      }),
      listen<{ projectId: string; percent: number; message: string }>(
        "import:progress",
        (event) => {
          setProgress({ percent: event.payload.percent, message: event.payload.message });
          setTask({
            key: `import:${event.payload.projectId}`,
            kind: "import",
            projectId: event.payload.projectId,
            percent: event.payload.percent,
            message: event.payload.message,
          });
        },
      ),
    ];
    return () => {
      void Promise.all(unlisteners).then((fns) => fns.forEach((fn) => fn()));
    };
  }, [navigate, refreshAll, toast, setTask, clearProjectTasks]);

  const analyze = async () => {
    setError(null);
    if (!isValidHttpUrl(url)) {
      setError("Introduce una URL válida que comience por http:// o https://.");
      return;
    }
    setAnalyzing(true);
    setMetadata(null);
    try {
      const meta = await api.downloadMetadata(url.trim());
      setMetadata(meta);
      const preferred = settings?.defaultQuality ?? "best";
      const available = meta.formats.some((format) => format.id === preferred);
      setQuality(available ? preferred : meta.formats[0]?.id ?? "best");
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setAnalyzing(false);
    }
  };

  const startDownload = async () => {
    if (!metadata) return;
    setError(null);
    setDownloading(true);
    setProgress({ percent: 0, message: "Preparando la descarga…" });
    try {
      const project = await api.downloadStart({
        url: metadata.url,
        quality,
        title: metadata.title,
        platform: metadata.platform,
        duration: metadata.duration,
      });
      activeProject.current = project.id;
      void refreshAll();
    } catch (err) {
      setError(errorMessage(err));
      setDownloading(false);
      setProgress(null);
    }
  };

  const cancelDownload = async () => {
    if (!activeProject.current) return;
    try {
      await api.downloadCancel(activeProject.current);
    } catch (err) {
      setError(errorMessage(err));
    }
  };

  const importFile = async (path: string) => {
    setError(null);
    setImporting(true);
    setProgress({ percent: 0, message: "Preparando la importación…" });
    try {
      const project = await api.projectImport(path);
      setProgress({ percent: 100, message: "Video listo" });
      setTask({
        key: `import:${project.id}`,
        kind: "import",
        projectId: project.id,
        percent: 100,
        message: "Video importado",
      });
      toast("success", "Video importado correctamente.");
      await refreshAll();
      clearProjectTasks(project.id);
      navigate("project", { projectId: project.id });
    } catch (err) {
      setError(errorMessage(err));
      toast("error", errorMessage(err));
    } finally {
      setImporting(false);
      setProgress(null);
    }
  };

  const pickFile = async () => {
    const selected = await open({
      multiple: false,
      directory: false,
      title: "Selecciona un video",
      filters: [{ name: "Videos", extensions: VIDEO_EXTENSIONS }],
    });
    if (typeof selected === "string") {
      await importFile(selected);
    }
  };

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void getCurrentWebview()
      .onDragDropEvent((event) => {
        if (event.payload.type === "over") {
          setDragActive(true);
        } else if (event.payload.type === "leave") {
          setDragActive(false);
        } else if (event.payload.type === "drop") {
          setDragActive(false);
          const paths = event.payload.paths ?? [];
          const video = paths.find((path) => {
            const ext = path.split(".").pop()?.toLowerCase() ?? "";
            return VIDEO_EXTENSIONS.includes(ext);
          });
          if (video) {
            setMode("import");
            void importFile(video);
          } else if (paths.length > 0) {
            setError("El archivo arrastrado no es un formato de video compatible.");
          }
        }
      })
      .then((fn) => {
        unlisten = fn;
      });
    return () => {
      if (unlisten) unlisten();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const missingTools = appInfo && (!appInfo.tools.ytdlp || !appInfo.tools.ffmpeg);

  return (
    <div className="page">
      <div className="section-head">
        <div>
          <h2>Crear un proyecto</h2>
          <p>Descarga desde una plataforma compatible o usa un archivo de tu equipo.</p>
        </div>
        <div className="spacer" />
        <Segmented
          value={mode}
          onChange={setMode}
          options={[
            { value: "download", label: "Descargar", icon: <Link2 /> },
            { value: "import", label: "Importar archivo", icon: <Upload /> },
          ]}
        />
      </div>

      {error ? (
        <Alert tone="danger" icon={<X size={16} />}>
          {error}
        </Alert>
      ) : null}

      {missingTools ? (
        <Alert tone="warning" icon={<ShieldCheck size={16} />}>
          Faltan herramientas externas. La importación local necesita FFmpeg/FFprobe y las
          descargas necesitan yt-dlp. Ejecuta <code className="mono">npm run tools:fetch</code> o
          revisa Configuración → Herramientas.
        </Alert>
      ) : null}

      {mode === "download" ? (
        <>
          <section className="card">
            <div className="card-header">
              <div className="grow">
                <h3 className="card-title">
                  <Link2 size={16} /> Enlace del video
                </h3>
                <p className="card-sub">
                  YouTube, Facebook y otros sitios que permitan descargas autorizadas.
                </p>
              </div>
            </div>
            <div className="row wrap" style={{ alignItems: "flex-start" }}>
              <div className="grow" style={{ minWidth: 260 }}>
                <div className="input-with-icon">
                  <Link2 />
                  <input
                    className="input"
                    placeholder="https://www.youtube.com/watch?v=…"
                    value={url}
                    onChange={(event) => setUrl(event.target.value)}
                    onKeyDown={(event) => {
                      if (event.key === "Enter" && !analyzing && !downloading) void analyze();
                    }}
                    disabled={downloading}
                    spellCheck={false}
                  />
                </div>
              </div>
              <button
                className="btn btn-primary"
                onClick={analyze}
                disabled={analyzing || downloading || url.trim().length === 0}
              >
                {analyzing ? <Loader2 className="spin" /> : <ArrowRight />}
                {analyzing ? "Analizando…" : "Analizar enlace"}
              </button>
            </div>
            <p className="field-hint" style={{ marginTop: 10 }}>
              ParCut no evade DRM, inicios de sesión ni restricciones de pago. Descarga solo
              contenido propio o con autorización expresa. Si un sitio no lo permite, importa el
              archivo local.
            </p>
          </section>

          {metadata ? (
            <section className="card">
              <div className="row wrap" style={{ gap: 18, alignItems: "flex-start" }}>
                <div
                  style={{
                    width: 260,
                    maxWidth: "100%",
                    borderRadius: 14,
                    overflow: "hidden",
                    border: "1px solid var(--border)",
                    flex: "none",
                    aspectRatio: "16 / 9",
                    background: "var(--surface)",
                  }}
                >
                  {metadata.thumbnail ? (
                    <img
                      src={metadata.thumbnail}
                      alt=""
                      style={{ width: "100%", height: "100%", objectFit: "cover" }}
                    />
                  ) : (
                    <div style={{ display: "grid", placeItems: "center", height: "100%" }}>
                      <Video size={26} className="faint" />
                    </div>
                  )}
                </div>
                <div className="grow col" style={{ minWidth: 240, gap: 8 }}>
                  <div className="row wrap" style={{ gap: 8 }}>
                    <span className="pill accent">{metadata.platform}</span>
                    <span className="pill">{formatDuration(metadata.duration)}</span>
                    {metadata.isLive ? <span className="pill danger">En directo</span> : null}
                  </div>
                  <h3 style={{ margin: "2px 0 0", fontSize: 17, lineHeight: 1.35 }}>
                    {metadata.title}
                  </h3>
                  <span className="small faint">
                    {metadata.uploader ? `Canal: ${metadata.uploader}` : "Autor desconocido"}
                  </span>
                  <div className="field" style={{ marginTop: 4 }}>
                    <label htmlFor="quality">Calidad de descarga</label>
                    <select
                      id="quality"
                      className="select"
                      value={quality}
                      onChange={(event) => setQuality(event.target.value)}
                      disabled={downloading}
                    >
                      {metadata.formats.map((format) => (
                        <option key={format.id} value={format.id}>
                          {format.label} · {format.note}
                          {format.filesize > 0 ? ` · ~${formatBytes(format.filesize)}` : ""}
                        </option>
                      ))}
                    </select>
                    <span className="field-hint">
                      Se descargará el mejor video disponible hasta la resolución elegida y se
                      unirá con su audio original.
                    </span>
                  </div>
                  <div className="row" style={{ marginTop: 6 }}>
                    <button
                      className="btn btn-primary"
                      onClick={startDownload}
                      disabled={downloading || importing}
                    >
                      {downloading ? <Loader2 className="spin" /> : <Upload />}
                      {downloading ? "Descargando…" : "Descargar video"}
                    </button>
                    {downloading ? (
                      <button className="btn btn-ghost" onClick={cancelDownload}>
                        Cancelar
                      </button>
                    ) : (
                      <button className="btn btn-ghost" onClick={() => setMetadata(null)}>
                        Cambiar enlace
                      </button>
                    )}
                  </div>
                </div>
              </div>

              {downloading && progress ? (
                <div style={{ marginTop: 18 }}>
                  <div className="row between small" style={{ marginBottom: 6 }}>
                    <span>{progress.message}</span>
                    <span className="mono-num faint">{Math.round(progress.percent)}%</span>
                  </div>
                  <ProgressBar value={progress.percent} />
                </div>
              ) : null}
            </section>
          ) : null}
        </>
      ) : (
        <section className="card">
          <div className="card-header">
            <div className="grow">
              <h3 className="card-title">
                <Video size={16} /> Importar desde tu equipo
              </h3>
              <p className="card-sub">
                El archivo se copia al espacio de ParCut para no modificarlo nunca.
              </p>
            </div>
          </div>
          <div
            className={`dropzone${dragActive ? " active" : ""}`}
            onDragOver={(event) => {
              event.preventDefault();
              setDragActive(true);
            }}
            onDragLeave={() => setDragActive(false)}
            onDrop={(event) => {
              event.preventDefault();
              setDragActive(false);
            }}
          >
            <div className="empty-icon" style={{ marginBottom: 0 }}>
              <Upload size={24} />
            </div>
            <h3>Arrastra un video aquí</h3>
            <p>MP4, MOV, MKV, WEBM, AVI y otros formatos habituales.</p>
            <button className="btn btn-primary" onClick={pickFile} disabled={importing}>
              {importing ? <Loader2 className="spin" /> : <Video />}
              {importing ? "Importando…" : "Seleccionar archivo"}
            </button>
          </div>
          {importing && progress ? (
            <div style={{ marginTop: 18 }}>
              <div className="row between small" style={{ marginBottom: 6 }}>
                <span>{progress.message}</span>
                <span className="mono-num faint">{Math.round(progress.percent)}%</span>
              </div>
              <ProgressBar value={progress.percent} />
            </div>
          ) : null}
        </section>
      )}

      <section className="card flat">
        <div className="row wrap" style={{ gap: 14 }}>
          <ShieldCheck size={18} style={{ color: "var(--success)" }} />
          <div className="grow" style={{ minWidth: 240 }}>
            <strong style={{ fontSize: 12.5 }}>Contenido autorizado</strong>
            <p className="field-hint" style={{ margin: "2px 0 0" }}>
              Usa ParCut únicamente con material propio, con licencia o con permiso del titular.
              Las cuentas sociales se gestionan por separado y nunca se guardan contraseñas.
            </p>
          </div>
        </div>
      </section>
    </div>
  );
}
