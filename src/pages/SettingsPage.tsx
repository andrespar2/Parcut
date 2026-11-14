import { useCallback, useEffect, useMemo, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import {
  Activity,
  AlertTriangle,
  CheckCircle2,
  FolderOpen,
  Gauge,
  HardDrive,
  Info,
  Loader2,
  Monitor,
  Moon,
  RefreshCw,
  Save,
  Settings2,
  ShieldCheck,
  Sun,
  Trash2,
  Wrench,
} from "lucide-react";
import { api, ActivityEntry, errorMessage, Settings, StorageStats } from "../lib/api";
import {
  formatBytes,
  formatDuration,
  partsToSeconds,
  relativeTime,
  timeParts,
} from "../lib/format";
import { useAppStore } from "../store/app";
import { Alert, Modal, Pill, ProgressBar, Segmented, SwitchRow } from "../components/ui";

export function SettingsPage() {
  const settings = useAppStore((state) => state.settings);
  const appInfo = useAppStore((state) => state.appInfo);
  const saveSettings = useAppStore((state) => state.saveSettings);
  const loadAppInfo = useAppStore((state) => state.loadAppInfo);
  const loadProjects = useAppStore((state) => state.loadProjects);
  const toast = useAppStore((state) => state.toast);

  const [form, setForm] = useState<Settings | null>(settings);
  const [stats, setStats] = useState<StorageStats | null>(null);
  const [activity, setActivity] = useState<ActivityEntry[]>([]);
  const [saving, setSaving] = useState(false);
  const [cleaning, setCleaning] = useState(false);
  const [cleanOpen, setCleanOpen] = useState(false);
  const [refreshingTools, setRefreshingTools] = useState(false);

  useEffect(() => {
    setForm(settings);
  }, [settings]);

  const loadStats = useCallback(async () => {
    try {
      const [loadedStats, loadedActivity] = await Promise.all([
        api.storageStats(),
        api.activityRecent(30),
      ]);
      setStats(loadedStats);
      setActivity(loadedActivity);
    } catch (err) {
      toast("error", errorMessage(err));
    }
  }, [toast]);

  useEffect(() => {
    void loadStats();
  }, [loadStats]);

  const dirty = useMemo(() => {
    if (!form || !settings) return false;
    return JSON.stringify(form) !== JSON.stringify(settings);
  }, [form, settings]);

  if (!form) {
    return (
      <div className="page">
        <div className="card row" style={{ justifyContent: "center", padding: 40 }}>
          <Loader2 className="spin" /> <span className="muted">Cargando configuración…</span>
        </div>
      </div>
    );
  }

  const update = (patch: Partial<Settings>) => setForm({ ...form, ...patch });

  const save = async () => {
    setSaving(true);
    try {
      await saveSettings(form);
      toast("success", "Configuración guardada.");
      await Promise.all([loadStats(), loadProjects()]);
    } catch (err) {
      toast("error", errorMessage(err));
    } finally {
      setSaving(false);
    }
  };

  const pickDirectory = async () => {
    const selected = await open({
      directory: true,
      multiple: false,
      title: "Selecciona la carpeta de almacenamiento",
      defaultPath: form.storageDir,
    });
    if (typeof selected === "string") {
      update({ storageDir: selected });
    }
  };

  const cleanTemporals = async () => {
    setCleaning(true);
    try {
      const freed = await api.tempCleanup();
      toast("success", `Temporales eliminados. ${formatBytes(freed)} liberados.`);
      setCleanOpen(false);
      await loadStats();
    } catch (err) {
      toast("error", errorMessage(err));
    } finally {
      setCleaning(false);
    }
  };

  const refreshTools = async () => {
    setRefreshingTools(true);
    try {
      await api.toolsRefresh();
      await loadAppInfo();
      toast("info", "Detección de herramientas actualizada.");
    } catch (err) {
      toast("error", errorMessage(err));
    } finally {
      setRefreshingTools(false);
    }
  };

  const clipParts = timeParts(form.defaultClipSeconds);
  const tools = appInfo?.tools;

  return (
    <div className="page">
      <div className="section-head">
        <div>
          <h2>Configuración</h2>
          <p>Ajusta el almacenamiento, la calidad de los clips y tus preferencias.</p>
        </div>
        <div className="spacer" />
        {dirty ? <Pill tone="warning">Cambios sin guardar</Pill> : null}
        <button className="btn btn-primary" onClick={save} disabled={!dirty || saving}>
          {saving ? <Loader2 className="spin" /> : <Save />} Guardar cambios
        </button>
      </div>

      {form.storageDir !== settings?.storageDir ? (
        <Alert tone="warning" icon={<AlertTriangle size={16} />}>
          Al cambiar el directorio, los proyectos nuevos se guardarán en la nueva ubicación. Los
          proyectos existentes conservan sus archivos y siguen siendo accesibles.
        </Alert>
      ) : null}

      <section className="card">
        <div className="card-header">
          <div className="grow">
            <h3 className="card-title">
              <HardDrive size={16} /> Almacenamiento
            </h3>
            <p className="card-sub">Carpeta administrada por ParCut para videos, clips y caché.</p>
          </div>
          <button
            className="btn btn-sm btn-ghost"
            onClick={() => void api.revealPath(form.storageDir)}
          >
            <FolderOpen /> Abrir carpeta
          </button>
        </div>
        <div className="row wrap" style={{ alignItems: "flex-end" }}>
          <div className="grow" style={{ minWidth: 260 }}>
            <div className="field">
              <label htmlFor="storage-dir">Directorio de almacenamiento</label>
              <input
                id="storage-dir"
                className="input"
                value={form.storageDir}
                onChange={(event) => update({ storageDir: event.target.value })}
                spellCheck={false}
              />
            </div>
          </div>
          <button className="btn" onClick={pickDirectory}>
            <FolderOpen /> Elegir carpeta
          </button>
        </div>

        {stats ? (
          <>
            <div className="stats-grid" style={{ marginTop: 16 }}>
              <div className="stat">
                <div className="stat-label">Espacio de trabajo</div>
                <div className="stat-value" style={{ fontSize: 22 }}>
                  {formatBytes(stats.workspaceBytes)}
                </div>
                <div className="stat-hint">{stats.projectCount} proyectos</div>
              </div>
              <div className="stat">
                <div className="stat-label">Videos originales</div>
                <div className="stat-value" style={{ fontSize: 22 }}>
                  {formatBytes(stats.sourceBytes)}
                </div>
              </div>
              <div className="stat">
                <div className="stat-label">Clips</div>
                <div className="stat-value" style={{ fontSize: 22 }}>
                  {formatBytes(stats.clipsBytes)}
                </div>
              </div>
              <div className="stat">
                <div className="stat-label">Temporales</div>
                <div className="stat-value" style={{ fontSize: 22 }}>
                  {formatBytes(stats.tempBytes)}
                </div>
                <div className="stat-hint">Seguros de eliminar</div>
              </div>
            </div>
            {stats.totalBytes > 0 ? (
              <div style={{ marginTop: 16 }}>
                <div className="row between small" style={{ marginBottom: 6 }}>
                  <span>Disco libre</span>
                  <span className="mono-num">
                    {formatBytes(stats.freeBytes)} de {formatBytes(stats.totalBytes)}
                  </span>
                </div>
                <ProgressBar
                  value={
                    stats.totalBytes > 0
                      ? ((stats.totalBytes - stats.freeBytes) / stats.totalBytes) * 100
                      : 0
                  }
                  thin
                />
              </div>
            ) : null}
          </>
        ) : null}

        <div className="row wrap" style={{ marginTop: 16 }}>
          <button
            className="btn btn-sm"
            onClick={() => setCleanOpen(true)}
            disabled={(stats?.tempBytes ?? 0) === 0}
          >
            <Trash2 /> Limpiar archivos temporales
          </button>
          <span className="small faint">
            No afecta a videos, clips ni metadatos de proyectos.
          </span>
        </div>
      </section>

      <div
        style={{
          display: "grid",
          gridTemplateColumns: "repeat(auto-fit, minmax(340px, 1fr))",
          gap: 16,
          alignItems: "start",
        }}
      >
        <section className="card col" style={{ gap: 16 }}>
          <div>
            <h3 className="card-title">
              <Gauge size={16} /> Video y clips
            </h3>
            <p className="card-sub">Valores iniciales para nuevos proyectos.</p>
          </div>

          <div className="field">
            <label>Duración predeterminada de cada clip</label>
            <div className="time-input">
              <input
                className="input"
                type="number"
                min={0}
                max={23}
                value={clipParts.hours}
                onChange={(event) =>
                  update({
                    defaultClipSeconds: partsToSeconds(
                      Number(event.target.value) || 0,
                      clipParts.minutes,
                      clipParts.seconds,
                    ),
                  })
                }
                aria-label="Horas por defecto"
              />
              <span>h</span>
              <input
                className="input"
                type="number"
                min={0}
                max={59}
                value={clipParts.minutes}
                onChange={(event) =>
                  update({
                    defaultClipSeconds: partsToSeconds(
                      clipParts.hours,
                      Number(event.target.value) || 0,
                      clipParts.seconds,
                    ),
                  })
                }
                aria-label="Minutos por defecto"
              />
              <span>min</span>
              <input
                className="input"
                type="number"
                min={0}
                max={59}
                value={clipParts.seconds}
                onChange={(event) =>
                  update({
                    defaultClipSeconds: partsToSeconds(
                      clipParts.hours,
                      clipParts.minutes,
                      Number(event.target.value) || 0,
                    ),
                  })
                }
                aria-label="Segundos por defecto"
              />
              <span>s</span>
            </div>
            <span className="field-hint">
              Actualmente {formatDuration(form.defaultClipSeconds)} por parte.
            </span>
          </div>

          <div className="field">
            <label htmlFor="default-quality">Calidad de descarga preferida</label>
            <select
              id="default-quality"
              className="select"
              value={form.defaultQuality}
              onChange={(event) => update({ defaultQuality: event.target.value })}
            >
              <option value="best">Mejor disponible</option>
              <option value="2160">Hasta 4K (2160p)</option>
              <option value="1440">Hasta 2K (1440p)</option>
              <option value="1080">Hasta Full HD (1080p)</option>
              <option value="720">Hasta HD (720p)</option>
              <option value="480">Hasta SD (480p)</option>
            </select>
          </div>

          <SwitchRow
            label="Priorizar cortes exactos"
            hint="Activa la recodificación controlada por defecto en la pantalla del proyecto."
            checked={form.preferPreciseCuts}
            onChange={(value) => update({ preferPreciseCuts: value })}
          />
          <SwitchRow
            label="Conservar el video original tras dividir"
            hint="Al desactivarlo, el video original se elimina después de dividir para ahorrar espacio."
            checked={form.keepSourceAfterSplit}
            onChange={(value) => update({ keepSourceAfterSplit: value })}
          />
        </section>

        <section className="card col" style={{ gap: 16 }}>
          <div>
            <h3 className="card-title">
              <Monitor size={16} /> Apariencia y previsualización
            </h3>
            <p className="card-sub">La interfaz se adapta a tu pantalla y a tu tema del sistema.</p>
          </div>

          <div className="field">
            <label>Tema</label>
            <Segmented
              value={form.theme}
              onChange={(value) => update({ theme: value })}
              options={[
                { value: "dark", label: "Oscuro", icon: <Moon /> },
                { value: "light", label: "Claro", icon: <Sun /> },
                { value: "system", label: "Sistema", icon: <Monitor /> },
              ]}
            />
          </div>

          <SwitchRow
            label="Reproducir clips automáticamente"
            hint="Al abrir el reproductor de un clip."
            checked={form.previewAutoplay}
            onChange={(value) => update({ previewAutoplay: value })}
          />
          <SwitchRow
            label="Silenciar previsualizaciones"
            hint="Útil para revisar clips sin interrumpir."
            checked={form.previewMuted}
            onChange={(value) => update({ previewMuted: value })}
          />
        </section>
      </div>

      <section className="card col" style={{ gap: 16 }}>
        <div>
          <h3 className="card-title">
            <ShieldCheck size={16} /> Publicación
          </h3>
          <p className="card-sub">
            Ajustes aplicados al preparar cada publicación. Los títulos se adaptan a los límites
            de cada plataforma.
          </p>
        </div>
        <div className="input-row">
          <div className="field">
            <label htmlFor="default-privacy">Privacidad en YouTube / Facebook</label>
            <select
              id="default-privacy"
              className="select"
              value={form.defaultPrivacy}
              onChange={(event) =>
                update({ defaultPrivacy: event.target.value as Settings["defaultPrivacy"] })
              }
            >
              <option value="private">Privado (recomendado al probar)</option>
              <option value="public">Público</option>
            </select>
          </div>
          <div className="field">
            <label htmlFor="tiktok-privacy">Privacidad en TikTok</label>
            <select
              id="tiktok-privacy"
              className="select"
              value={form.tiktokPrivacyLevel}
              onChange={(event) =>
                update({
                  tiktokPrivacyLevel: event.target.value as Settings["tiktokPrivacyLevel"],
                })
              }
            >
              <option value="SELF_ONLY">Solo yo (apps sin auditar)</option>
              <option value="PUBLIC_TO_EVERYONE">Público</option>
              <option value="MUTUAL_FOLLOW_FRIENDS">Amigos</option>
            </select>
          </div>
          <div className="field">
            <label htmlFor="yt-category">Categoría de YouTube (ID)</label>
            <input
              id="yt-category"
              className="input"
              value={form.youtubeCategoryId}
              onChange={(event) => update({ youtubeCategoryId: event.target.value })}
              placeholder="22"
            />
            <span className="field-hint">22 = Personas y blogs. Se usa si el clip es genérico.</span>
          </div>
          <div className="field">
            <label htmlFor="fb-page">ID de Página de Facebook</label>
            <input
              id="fb-page"
              className="input"
              value={form.facebookPageId}
              onChange={(event) => update({ facebookPageId: event.target.value })}
              placeholder="Opcional: si está vacío se usa la primera Página"
              spellCheck={false}
            />
            <span className="field-hint">
              ParCut publica en Páginas de Facebook, no en perfiles personales.
            </span>
          </div>
        </div>
      </section>

      <section className="card col" style={{ gap: 14 }}>
        <div className="row wrap" style={{ alignItems: "flex-start" }}>
          <div className="grow" style={{ minWidth: 240 }}>
            <h3 className="card-title">
              <Wrench size={16} /> Herramientas externas
            </h3>
            <p className="card-sub">
              FFmpeg procesa el video; yt-dlp permite descargas autorizadas.
            </p>
          </div>
          <button className="btn btn-sm" onClick={refreshTools} disabled={refreshingTools}>
            {refreshingTools ? <Loader2 className="spin" /> : <RefreshCw />} Volver a detectar
          </button>
        </div>
        <div className="col" style={{ gap: 8 }}>
          <ToolRow label="FFmpeg" ok={!!tools?.ffmpeg} path={tools?.ffmpegPath} />
          <ToolRow label="FFprobe" ok={!!tools?.ffprobe} path={tools?.ffprobe ? "Incluido con FFmpeg" : null} />
          <ToolRow label="yt-dlp" ok={!!tools?.ytdlp} path={tools?.ytdlpPath} />
        </div>
        {!tools?.ffmpeg || !tools?.ytdlp ? (
          <Alert tone="warning" icon={<AlertTriangle size={16} />}>
            Si falta alguna herramienta, ejecuta <code className="mono">npm run tools:fetch</code>{" "}
            en la carpeta del proyecto y vuelve a detectar. En la versión instalada, los binarios
            se incluyen empaquetados.
          </Alert>
        ) : (
          <span className="small faint">
            Las herramientas empaquetadas se ejecutan como procesos separados con argumentos
            validados.
          </span>
        )}
      </section>

      <section className="card">
        <div className="card-header">
          <div className="grow">
            <h3 className="card-title">
              <Activity size={16} /> Actividad reciente
            </h3>
            <p className="card-sub">Registro local. No se envía a ningún servidor.</p>
          </div>
          <button className="btn btn-sm btn-ghost" onClick={() => void loadStats()}>
            <RefreshCw /> Actualizar
          </button>
        </div>
        {activity.length === 0 ? (
          <p className="small faint">Sin actividad registrada todavía.</p>
        ) : (
          <div className="col" style={{ gap: 6, maxHeight: 280, overflowY: "auto" }}>
            {activity.map((entry) => (
              <div
                key={entry.id}
                className="row"
                style={{
                  gap: 10,
                  padding: "8px 10px",
                  borderRadius: 10,
                  border: "1px solid var(--border)",
                  background: "var(--surface)",
                }}
              >
                <span
                  className="dot"
                  style={{
                    color:
                      entry.level === "error"
                        ? "var(--danger)"
                        : entry.level === "warn"
                          ? "var(--warning)"
                          : "var(--success)",
                  }}
                />
                <div className="grow" style={{ minWidth: 0 }}>
                  <div style={{ fontSize: 12.5 }}>{entry.message}</div>
                  {entry.detail ? (
                    <div className="small faint" style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                      {entry.detail}
                    </div>
                  ) : null}
                </div>
                <span className="small faint" style={{ flex: "none" }}>
                  {relativeTime(entry.createdAt)}
                </span>
              </div>
            ))}
          </div>
        )}
      </section>

      <section className="card">
        <div className="row wrap" style={{ gap: 16, alignItems: "flex-start" }}>
          <div className="empty-icon" style={{ marginBottom: 0 }}>
            <Info size={22} />
          </div>
          <div className="grow" style={{ minWidth: 260 }}>
            <h3 style={{ margin: "0 0 2px", fontSize: 15 }}>ParCut {appInfo?.version ?? ""}</h3>
            <p className="small muted" style={{ margin: 0 }}>
              Desarrollado por <strong>andrespar2</strong> · {appInfo?.platform ?? "Windows x64"}
            </p>
            <p className="field-hint" style={{ marginTop: 8, lineHeight: 1.6 }}>
              Aplicación de escritorio para Windows 10 y 11. FFmpeg se distribuye bajo LGPL como
              binario independiente; yt-dlp es de dominio público (Unlicense). El código de ParCut
              se distribuye con licencia MIT. Consulta <code className="mono">docs/BUILD.md</code>{" "}
              para las obligaciones de terceros.
            </p>
            <div className="row wrap" style={{ gap: 8, marginTop: 10 }}>
              <button
                className="btn btn-sm btn-ghost"
                onClick={() => void api.openExternal("https://github.com/andrespar2/Parcut")}
              >
                <Settings2 /> Repositorio
              </button>
              <button
                className="btn btn-sm btn-ghost"
                onClick={() => void api.openExternal("https://ffmpeg.org/legal.html")}
              >
                <CheckCircle2 /> Licencia de FFmpeg
              </button>
            </div>
          </div>
        </div>
      </section>

      <Modal
        open={cleanOpen}
        title="Limpiar archivos temporales"
        subtitle="Se eliminarán solo los archivos de la carpeta temporal de cada proyecto."
        onClose={() => setCleanOpen(false)}
        busy={cleaning}
        actions={
          <>
            <button className="btn btn-ghost" onClick={() => setCleanOpen(false)} disabled={cleaning}>
              Cancelar
            </button>
            <button className="btn btn-primary" onClick={cleanTemporals} disabled={cleaning}>
              {cleaning ? <Loader2 className="spin" /> : <Trash2 />} Limpiar
            </button>
          </>
        }
      >
        <p className="small muted">
          Se liberarán aproximadamente {formatBytes(stats?.tempBytes ?? 0)}. Los videos originales,
          clips, miniaturas y la cola de publicaciones no se modifican.
        </p>
      </Modal>
    </div>
  );
}

function ToolRow({ label, ok, path }: { label: string; ok: boolean; path?: string | null }) {
  return (
    <div className="account-row">
      <span style={{ color: ok ? "var(--success)" : "var(--danger)", display: "flex" }}>
        {ok ? <CheckCircle2 size={15} /> : <AlertTriangle size={15} />}
      </span>
      <div className="grow" style={{ minWidth: 0 }}>
        <div style={{ fontSize: 12.5, fontWeight: 550 }}>{label}</div>
        <div className="small faint" style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
          {path ?? (ok ? "Detectado" : "No encontrado")}
        </div>
      </div>
      <Pill tone={ok ? "success" : "danger"}>{ok ? "Disponible" : "Falta"}</Pill>
    </div>
  );
}
