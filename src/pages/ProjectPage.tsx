import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  AlertTriangle,
  ChevronLeft,
  Clapperboard,
  Columns3,
  Film,
  FolderOpen,
  Hash,
  Loader2,
  Pencil,
  Scissors,
  Share2,
  Sparkles,
  Trash2,
  Video,
  Zap,
} from "lucide-react";
import { api, Clip, errorMessage, Project, SplitPlan } from "../lib/api";
import {
  formatBytes,
  formatDuration,
  formatDurationLong,
  partsToSeconds,
  resolutionLabel,
  timeParts,
} from "../lib/format";
import { useAppStore } from "../store/app";
import { Alert, EmptyState, Modal, Pill, ProgressBar, Segmented } from "../components/ui";
import { ClipCard } from "../components/ClipCard";
import { PlayerModal } from "../components/PlayerModal";

const PRESETS = [
  { label: "30 s", seconds: 30 },
  { label: "1 min", seconds: 60 },
  { label: "1:30", seconds: 90 },
  { label: "2 min", seconds: 120 },
  { label: "3 min", seconds: 180 },
  { label: "5 min", seconds: 300 },
];

export function ProjectPage() {
  const projectId = useAppStore((state) => state.projectId);
  const navigate = useAppStore((state) => state.navigate);
  const settings = useAppStore((state) => state.settings);
  const toast = useAppStore((state) => state.toast);
  const refreshAll = useAppStore((state) => state.refreshAll);
  const setTask = useAppStore((state) => state.setTask);
  const clearProjectTasks = useAppStore((state) => state.clearProjectTasks);

  const [project, setProject] = useState<Project | null>(null);
  const [clips, setClips] = useState<Clip[]>([]);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);

  const initial = timeParts(90);
  const [hours, setHours] = useState(initial.hours);
  const [minutes, setMinutes] = useState(initial.minutes);
  const [seconds, setSeconds] = useState(initial.seconds);
  const [rangeStart, setRangeStart] = useState(0);
  const [rangeEnd, setRangeEnd] = useState(0);
  const [mode, setMode] = useState<"copy" | "precise">("copy");
  const [plan, setPlan] = useState<SplitPlan | null>(null);
  const [planError, setPlanError] = useState<string | null>(null);
  const [splitting, setSplitting] = useState(false);
  const [splitProgress, setSplitProgress] = useState<{ percent: number; message: string } | null>(
    null,
  );

  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [playing, setPlaying] = useState<{ src: string; title: string } | null>(null);
  const [deleteOpen, setDeleteOpen] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [editOpen, setEditOpen] = useState(false);
  const [editName, setEditName] = useState("");
  const [editTags, setEditTags] = useState<string[]>([]);
  const [tagDraft, setTagDraft] = useState("");
  const [clipToDelete, setClipToDelete] = useState<Clip | null>(null);

  const splitSeconds = useMemo(
    () => partsToSeconds(hours, minutes, seconds),
    [hours, minutes, seconds],
  );
  const videoDuration = project?.duration ?? 0;
  const customRange =
    videoDuration > 0 && (rangeStart > 0.05 || rangeEnd < videoDuration - 0.05);

  const activeProject = useRef<string | null>(null);
  const playerRef = useRef<HTMLVideoElement | null>(null);

  const load = useCallback(async () => {
    if (!projectId) return;
    setLoading(true);
    setLoadError(null);
    try {
      const [loadedProject, loadedClips] = await Promise.all([
        api.projectGet(projectId),
        api.clipsList(projectId),
      ]);
      setProject(loadedProject);
      setClips(loadedClips);
      setSelected(new Set(loadedClips.map((clip) => clip.id)));
      if (activeProject.current !== loadedProject.id) {
        // Solo se reinician los campos al abrir otro proyecto, no en cada refresco.
        const parts = timeParts(loadedProject.defaultClipSeconds || 90);
        setHours(parts.hours);
        setMinutes(parts.minutes);
        setSeconds(parts.seconds);
        setRangeStart(0);
        setRangeEnd(loadedProject.duration);
        setMode(settings?.preferPreciseCuts ? "precise" : "copy");
        activeProject.current = loadedProject.id;
      }
    } catch (err) {
      setLoadError(errorMessage(err));
    } finally {
      setLoading(false);
    }
  }, [projectId, settings?.preferPreciseCuts]);

  useEffect(() => {
    void load();
  }, [load]);

  useEffect(() => {
    if (!projectId) return;
    const unlisteners = [
      listen<{ projectId: string; percent: number; message: string }>("split:progress", (event) => {
        if (event.payload.projectId !== projectId) return;
        setSplitProgress({ percent: event.payload.percent, message: event.payload.message });
        setTask({
          key: `split:${projectId}`,
          kind: "split",
          projectId,
          percent: event.payload.percent,
          message: event.payload.message,
        });
      }),
      listen<{ message: string }>("split:notice", (event) => {
        toast("info", event.payload.message);
      }),
      listen<{ projectId: string; clipCount: number }>("split:done", (event) => {
        if (event.payload.projectId !== projectId) return;
        clearProjectTasks(projectId);
        setSplitting(false);
        setSplitProgress(null);
        void load();
        void refreshAll();
      }),
      listen<{ projectId: string; message: string }>("split:error", (event) => {
        if (event.payload.projectId !== projectId) return;
        clearProjectTasks(projectId);
        setSplitting(false);
        setSplitProgress(null);
        toast("error", event.payload.message);
      }),
      listen<{ clipId: string }>("clips:changed", () => {
        void load();
        void refreshAll();
      }),
      listen<{ id: string }>("project:updated", () => {
        void load();
      }),
    ];
    return () => {
      void Promise.all(unlisteners).then((fns) => fns.forEach((fn) => fn()));
    };
  }, [projectId, load, refreshAll, setTask, clearProjectTasks, toast]);

  // Estimación del plan (con retardo para no llamar en cada tecla).
  useEffect(() => {
    if (!projectId || splitSeconds <= 0) {
      setPlan(null);
      return;
    }
    if (rangeEnd <= rangeStart) {
      setPlan(null);
      setPlanError("El punto final del rango debe ser posterior al punto inicial.");
      return;
    }
    if (rangeEnd - rangeStart < 2) {
      setPlan(null);
      setPlanError("Selecciona al menos 2 segundos de video para generar clips.");
      return;
    }
    const timer = window.setTimeout(() => {
      api
        .splitPlan(projectId, splitSeconds, rangeStart, rangeEnd)
        .then((value) => {
          setPlan(value);
          setPlanError(null);
        })
        .catch((err) => {
          setPlan(null);
          setPlanError(errorMessage(err));
        });
    }, 300);
    return () => window.clearTimeout(timer);
  }, [projectId, splitSeconds, rangeStart, rangeEnd]);

  // Un rango personalizado exige corte exacto para respetar los límites.
  useEffect(() => {
    if (customRange) setMode("precise");
  }, [customRange]);

  const startSplit = async () => {
    if (!projectId || !plan) return;
    setSplitting(true);
    setSplitProgress({ percent: 0, message: "Preparando la división…" });
    try {
      const generated = await api.splitStart(
        projectId,
        plan.clipSeconds,
        mode,
        plan.rangeStart,
        plan.rangeEnd,
      );
      setClips(generated);
      setSelected(new Set(generated.map((clip) => clip.id)));
      toast(
        "success",
        `${generated.length} clips creados. Ya puedes revisarlos y publicarlos.`,
      );
      await Promise.all([load(), refreshAll()]);
    } catch (err) {
      const message = errorMessage(err);
      if (!message.toLowerCase().includes("cancelado")) {
        toast("error", message);
      } else {
        toast("info", "División cancelada. No se guardaron clips parciales.");
      }
    } finally {
      setSplitting(false);
      setSplitProgress(null);
      clearProjectTasks(projectId);
    }
  };

  const cancelSplit = async () => {
    if (!projectId) return;
    try {
      await api.splitCancel(projectId);
    } catch (err) {
      toast("error", errorMessage(err));
    }
  };

  const clampRangeStart = (value: number) => {
    const maxStart = Math.max(0, Math.min(videoDuration, rangeEnd || videoDuration) - 2);
    setRangeStart(Math.max(0, Math.min(value, maxStart)));
  };
  const clampRangeEnd = (value: number) => {
    const minEnd = Math.min(videoDuration, rangeStart + 2);
    setRangeEnd(Math.max(minEnd, Math.min(value, videoDuration)));
  };
  const resetRange = () => {
    setRangeStart(0);
    setRangeEnd(videoDuration);
  };
  const markRangeStart = () => {
    const current = playerRef.current?.currentTime;
    if (current === undefined) {
      toast("info", "Reproduce el video y coloca el cursor en el punto deseado.");
      return;
    }
    clampRangeStart(Math.floor(current * 10) / 10);
    playerRef.current?.pause();
  };
  const markRangeEnd = () => {
    const current = playerRef.current?.currentTime;
    if (current === undefined) {
      toast("info", "Reproduce el video y coloca el cursor en el punto deseado.");
      return;
    }
    clampRangeEnd(Math.ceil(current * 10) / 10);
    playerRef.current?.pause();
  };

  const deleteProject = async () => {
    if (!projectId) return;
    setDeleting(true);
    try {
      const report = await api.projectDeleteAll(projectId);
      toast(
        "success",
        `Proyecto eliminado: ${report.removedFiles} archivos y ${formatBytes(
          report.freedBytes,
        )} liberados.${report.warnings.length ? ` ${report.warnings.join(" ")}` : ""}`,
      );
      setDeleteOpen(false);
      await refreshAll();
      navigate("home");
    } catch (err) {
      toast("error", errorMessage(err));
    } finally {
      setDeleting(false);
    }
  };

  const saveProjectEdit = async () => {
    if (!projectId) return;
    try {
      const updated = await api.projectUpdate(projectId, {
        name: editName,
        hashtags: editTags,
        defaultClipSeconds: splitSeconds > 0 ? splitSeconds : undefined,
      });
      setProject(updated);
      setEditOpen(false);
      toast("success", "Proyecto actualizado.");
      void refreshAll();
    } catch (err) {
      toast("error", errorMessage(err));
    }
  };

  const toggleClip = (clipId: string) => {
    setSelected((current) => {
      const next = new Set(current);
      if (next.has(clipId)) next.delete(clipId);
      else next.add(clipId);
      return next;
    });
  };

  const deleteClip = async () => {
    if (!clipToDelete) return;
    try {
      await api.clipDelete(clipToDelete.id);
      setClipToDelete(null);
      toast("success", "Clip eliminado.");
      await Promise.all([load(), refreshAll()]);
    } catch (err) {
      toast("error", errorMessage(err));
    }
  };

  const addTag = () => {
    const clean = tagDraft
      .split(/[#,\s]+/)
      .map((tag) => tag.replace(/[^\p{L}\p{N}_]/gu, ""))
      .filter(Boolean);
    if (clean.length === 0) return;
    setEditTags((current) => {
      const merged = [...current];
      for (const tag of clean) {
        if (!merged.some((existing) => existing.toLowerCase() === tag.toLowerCase())) {
          merged.push(tag);
        }
      }
      return merged.slice(0, 30);
    });
    setTagDraft("");
  };

  if (!projectId) {
    return (
      <div className="page">
        <EmptyState
          icon={<Film size={24} />}
          title="Selecciona un proyecto"
          description="Abre un proyecto desde Inicio o crea uno nuevo para dividirlo en clips."
          action={
            <button className="btn btn-primary" onClick={() => navigate("home")}>
              Ir a Inicio
            </button>
          }
        />
      </div>
    );
  }

  if (loading) {
    return (
      <div className="page">
        <div className="card row" style={{ justifyContent: "center", padding: 40 }}>
          <Loader2 className="spin" /> <span className="muted">Cargando proyecto…</span>
        </div>
      </div>
    );
  }

  if (loadError || !project) {
    return (
      <div className="page">
        <Alert tone="danger" icon={<AlertTriangle size={16} />}>
          {loadError ?? "No se encontró el proyecto."}
        </Alert>
        <button className="btn" onClick={() => navigate("home")}>
          <ChevronLeft /> Volver al inicio
        </button>
      </div>
    );
  }

  const source = project.originalPath ? convertFileSrc(project.originalPath) : null;
  const rangeStartPct =
    videoDuration > 0 ? Math.min(100, Math.max(0, (rangeStart / videoDuration) * 100)) : 0;
  const rangeEndPct =
    videoDuration > 0 ? Math.min(100, Math.max(0, (rangeEnd / videoDuration) * 100)) : 0;
  const selectedClips = clips.filter((clip) => selected.has(clip.id));
  const totalSelectedDuration = selectedClips.reduce((sum, clip) => sum + clip.duration, 0);

  return (
    <div className="page">
      <div className="row wrap" style={{ gap: 10 }}>
        <button className="icon-btn" onClick={() => navigate("home")} title="Volver">
          <ChevronLeft />
        </button>
        <div className="grow" style={{ minWidth: 220 }}>
          <div className="row wrap" style={{ gap: 8 }}>
            <h2 style={{ margin: 0, fontSize: 23, letterSpacing: "-0.02em" }}>{project.name}</h2>
            <Pill tone="accent">{project.platform ?? "Archivo local"}</Pill>
            <Pill tone={project.status === "ready" ? "success" : "warning"}>{project.status}</Pill>
          </div>
          <span className="small faint">
            Creado {new Date(project.createdAt).toLocaleDateString("es-ES")} · {clips.length} clips
            · {formatDurationLong(project.duration)}
          </span>
        </div>
        <div className="row wrap">
          <button
            className="btn"
            onClick={() => {
              setEditName(project.name);
              setEditTags(project.hashtags ?? []);
              setEditOpen(true);
            }}
          >
            <Pencil /> Editar
          </button>
          <button
            className="btn btn-primary"
            onClick={() => navigate("publish", { publishProjectId: project.id })}
            disabled={clips.length === 0}
          >
            <Share2 /> Publicar clips
          </button>
          <button className="btn btn-danger" onClick={() => setDeleteOpen(true)}>
            <Trash2 /> Eliminar todo
          </button>
        </div>
      </div>

      <div className="detail-grid">
        <div className="detail">
          <div className="detail-label">Duración</div>
          <div className="detail-value">{formatDuration(project.duration)}</div>
        </div>
        <div className="detail">
          <div className="detail-label">Resolución</div>
          <div className="detail-value">{resolutionLabel(project.width, project.height)}</div>
        </div>
        <div className="detail">
          <div className="detail-label">Fotogramas</div>
          <div className="detail-value">{project.fps ? `${project.fps} fps` : "—"}</div>
        </div>
        <div className="detail">
          <div className="detail-label">Tamaño</div>
          <div className="detail-value">{formatBytes(project.sizeBytes)}</div>
        </div>
        <div className="detail">
          <div className="detail-label">Códecs</div>
          <div className="detail-value">
            {project.videoCodec ?? "—"} / {project.audioCodec ?? "—"}
          </div>
        </div>
        <div className="detail">
          <div className="detail-label">Contenedor</div>
          <div className="detail-value">{project.container ?? "—"}</div>
        </div>
      </div>

      <div
        style={{
          display: "grid",
          gridTemplateColumns: "minmax(0, 1.35fr) minmax(320px, 1fr)",
          gap: 16,
          alignItems: "start",
        }}
      >
        <section className="card" style={{ padding: 16 }}>
          {source ? (
            <>
              <div className="player-frame">
                <video
                  key={source}
                  ref={playerRef}
                  src={source}
                  controls
                  playsInline
                  preload="metadata"
                />
              </div>
              <div className="row wrap" style={{ justifyContent: "space-between", marginTop: 12, gap: 8 }}>
                <div className="row" style={{ gap: 8, minWidth: 0 }}>
                  <Video size={14} className="faint" />
                  <span className="small faint" style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", maxWidth: 420 }}>
                    {project.originalPath?.split(/[\\/]/).pop()}
                  </span>
                </div>
                <button
                  className="btn btn-sm btn-ghost"
                  onClick={() => project.originalPath && api.revealPath(project.originalPath)}
                >
                  <FolderOpen /> Abrir ubicación
                </button>
              </div>
            </>
          ) : (
            <EmptyState
              icon={<Video size={22} />}
              title="Video no disponible"
              description="El archivo original no está asociado a este proyecto."
            />
          )}
        </section>

        <section className="card col" style={{ gap: 14 }}>
          <div>
            <h3 className="card-title">
              <Scissors size={16} /> Dividir en clips
            </h3>
            <p className="card-sub" style={{ marginTop: 4 }}>
              Recorta el fragmento que te interesa y elige la duración de cada parte.
            </p>
          </div>

          <div className="field">
            <div className="row between">
              <label>Rango del video original</label>
              <button
                className="btn btn-sm btn-ghost"
                onClick={resetRange}
                disabled={splitting}
              >
                Usar video completo
              </button>
            </div>
            <div className="range-labels">
              <span>{formatDuration(rangeStart)}</span>
              <span>{formatDuration(Math.max(0, rangeEnd - rangeStart))} seleccionados</span>
              <span>{formatDuration(rangeEnd)}</span>
            </div>
            <div className="range-track" aria-hidden>
              <span
                className="range-fill"
                style={{
                  left: `${rangeStartPct}%`,
                  width: `${Math.max(0, rangeEndPct - rangeStartPct)}%`,
                }}
              />
            </div>
            <div className="range-inputs">
              <RangeTimeFields
                label="Desde"
                seconds={rangeStart}
                max={videoDuration}
                onChange={clampRangeStart}
                disabled={splitting}
              />
              <RangeTimeFields
                label="Hasta"
                seconds={rangeEnd}
                max={videoDuration}
                onChange={clampRangeEnd}
                disabled={splitting}
              />
            </div>
            <div className="row wrap" style={{ gap: 6, marginTop: 8 }}>
              <button
                className="btn btn-sm"
                onClick={markRangeStart}
                disabled={splitting || !project.originalPath}
              >
                Marcar inicio en el reproductor
              </button>
              <button
                className="btn btn-sm"
                onClick={markRangeEnd}
                disabled={splitting || !project.originalPath}
              >
                Marcar fin
              </button>
            </div>
            <span className="field-hint">
              Los clips se generan solo dentro de este rango y la numeración empieza en 1.
            </span>
          </div>

          <div className="field">
            <label>Duración de cada clip</label>
            <div className="time-input" aria-label="Duración de cada clip">
              <input
                className="input"
                type="number"
                min={0}
                max={23}
                value={hours}
                onChange={(event) => setHours(Math.max(0, Math.min(23, Number(event.target.value) || 0)))}
                disabled={splitting}
                aria-label="Horas"
              />
              <span>h</span>
              <input
                className="input"
                type="number"
                min={0}
                max={59}
                value={minutes}
                onChange={(event) => setMinutes(Math.max(0, Math.min(59, Number(event.target.value) || 0)))}
                disabled={splitting}
                aria-label="Minutos"
              />
              <span>min</span>
              <input
                className="input"
                type="number"
                min={0}
                max={59}
                value={seconds}
                onChange={(event) => setSeconds(Math.max(0, Math.min(59, Number(event.target.value) || 0)))}
                disabled={splitting}
                aria-label="Segundos"
              />
              <span>s</span>
            </div>
          </div>

          <div className="row wrap" style={{ gap: 6 }}>
            {PRESETS.map((preset) => (
              <button
                key={preset.seconds}
                className="btn btn-sm"
                onClick={() => {
                  const parts = timeParts(preset.seconds);
                  setHours(parts.hours);
                  setMinutes(parts.minutes);
                  setSeconds(parts.seconds);
                }}
                disabled={splitting}
              >
                {preset.label}
              </button>
            ))}
          </div>

          <Segmented
            value={mode}
            onChange={setMode}
            options={[
              {
                value: "copy",
                label: "Sin recodificar",
                icon: <Zap />,
                disabled: customRange,
                title: customRange
                  ? "El rango personalizado requiere corte exacto"
                  : "División instantánea sin pérdida",
              },
              { value: "precise", label: "Corte exacto", icon: <Sparkles /> },
            ]}
          />
          <p className="field-hint">
            {mode === "copy"
              ? "División instantánea sin pérdida de calidad; los cortes se alinean al fotograma clave más cercano."
              : "Recodificación controlada H.264/AAC con CRF 20 para cortes exactos al milisegundo."}
          </p>
          {customRange ? (
            <Alert tone="info">
              Al recortar un rango personalizado se usa «Corte exacto» para respetar el inicio y el
              final seleccionados. «Sin recodificar» solo está disponible para el video completo.
            </Alert>
          ) : null}

          {plan ? (
            <div className="card flat col" style={{ gap: 8 }}>
              <div className="row between">
                <span className="small">Rango seleccionado</span>
                <strong className="mono-num">
                  {formatDuration(plan.rangeStart)} – {formatDuration(plan.rangeEnd)}
                </strong>
              </div>
              <div className="row between">
                <span className="small">Duración del rango</span>
                <strong className="mono-num">{formatDuration(plan.rangeSeconds)}</strong>
              </div>
              <div className="row between">
                <span className="small">Clips estimados</span>
                <strong className="mono-num">{plan.clipCount}</strong>
              </div>
              <div className="row between">
                <span className="small">Duración de cada parte</span>
                <strong className="mono-num">{formatDuration(plan.clipSeconds)}</strong>
              </div>
              <div className="row between">
                <span className="small">Último fragmento</span>
                <strong className="mono-num">{formatDuration(plan.lastClipSeconds)}</strong>
              </div>
              <div className="row between">
                <span className="small">Espacio estimado</span>
                <strong className="mono-num">{formatBytes(plan.estimatedTotalBytes)}</strong>
              </div>
            </div>
          ) : planError ? (
            <Alert tone="warning">{planError}</Alert>
          ) : null}

          {splitProgress ? (
            <div>
              <div className="row between small" style={{ marginBottom: 6 }}>
                <span>{splitProgress.message}</span>
                <span className="mono-num faint">{Math.round(splitProgress.percent)}%</span>
              </div>
              <ProgressBar value={splitProgress.percent} />
            </div>
          ) : null}

          <div className="row">
            <button
              className="btn btn-primary grow"
              onClick={startSplit}
              disabled={splitting || !plan || splitSeconds <= 0}
            >
              {splitting ? <Loader2 className="spin" /> : <Scissors />}
              {splitting
                ? "Dividiendo…"
                : plan
                  ? plan.clipCount === 1
                    ? "Crear 1 clip"
                    : `Crear ${plan.clipCount} clips`
                  : "Dividir video"}
            </button>
            {splitting ? (
              <button className="btn btn-ghost" onClick={cancelSplit}>
                Cancelar
              </button>
            ) : null}
          </div>
          {clips.length > 0 && !splitting ? (
            <p className="field-hint">
              Volver a dividir reemplaza los clips actuales del proyecto.
            </p>
          ) : null}
        </section>
      </div>

      <section>
        <div className="section-head" style={{ marginBottom: 12 }}>
          <div>
            <h2>Clips del proyecto</h2>
            <p>
              {clips.length > 0
                ? `${clips.length} partes · ${formatDuration(totalSelectedDuration)} seleccionadas`
                : "Todavía no se ha dividido este video."}
            </p>
          </div>
          <div className="spacer" />
          {clips.length > 0 ? (
            <div className="row wrap">
              <button
                className="btn btn-sm"
                onClick={() =>
                  setSelected(
                    selected.size === clips.length
                      ? new Set()
                      : new Set(clips.map((clip) => clip.id)),
                  )
                }
              >
                <Columns3 />
                {selected.size === clips.length ? "Deseleccionar todo" : "Seleccionar todo"}
              </button>
              <button
                className="btn btn-sm btn-primary"
                disabled={selected.size === 0}
                onClick={() => navigate("publish", { publishProjectId: project.id })}
              >
                <Share2 /> Publicar selección ({selected.size})
              </button>
            </div>
          ) : null}
        </div>

        {clips.length === 0 ? (
          <EmptyState
            icon={<Clapperboard size={24} />}
            title="Sin clips por ahora"
            description="Elige una duración y pulsa «Dividir video». Verás cada parte aquí, en orden ascendente y lista para publicar."
          />
        ) : (
          <div className="clip-grid">
            {clips.map((clip) => (
              <ClipCard
                key={clip.id}
                clip={clip}
                selected={selected.has(clip.id)}
                onToggle={() => toggleClip(clip.id)}
                onPlay={() =>
                  setPlaying({
                    src: clip.path,
                    title: `${project.name} · Parte ${clip.partIndex}`,
                    // el subtítulo se pasa abajo
                  })
                }
                onDelete={() => setClipToDelete(clip)}
              />
            ))}
          </div>
        )}
      </section>

      <PlayerModal
        open={playing !== null}
        src={playing?.src ?? null}
        title={playing?.title ?? ""}
        subtitle={`${formatDuration(
          clips.find((clip) => clip.path === playing?.src)?.duration ?? 0,
        )} · ${formatBytes(clips.find((clip) => clip.path === playing?.src)?.sizeBytes ?? 0)}`}
        muted={settings?.previewMuted}
        autoPlay={settings?.previewAutoplay ?? false}
        onClose={() => setPlaying(null)}
      />

      <Modal
        open={editOpen}
        title="Editar proyecto"
        subtitle="El nombre y los hashtags se usan en los títulos de publicación."
        onClose={() => setEditOpen(false)}
        actions={
          <>
            <button className="btn btn-ghost" onClick={() => setEditOpen(false)}>
              Cancelar
            </button>
            <button className="btn btn-primary" onClick={saveProjectEdit}>
              Guardar cambios
            </button>
          </>
        }
      >
        <div className="col" style={{ gap: 14 }}>
          <div className="field">
            <label htmlFor="project-name">Nombre del video</label>
            <input
              id="project-name"
              className="input"
              value={editName}
              onChange={(event) => setEditName(event.target.value)}
              maxLength={160}
            />
            <span className="field-hint">
              Ejemplo: «Mi viaje por Colombia - PARTE 1». El número de parte se añade solo.
            </span>
          </div>
          <div className="field">
            <label>Hashtags generales</label>
            <div className="chips">
              {editTags.map((tag) => (
                <span key={tag} className="chip">
                  #{tag}
                  <button
                    onClick={() => setEditTags(editTags.filter((item) => item !== tag))}
                    aria-label={`Quitar ${tag}`}
                  >
                    ×
                  </button>
                </span>
              ))}
              {editTags.length === 0 ? <span className="small faint">Sin hashtags</span> : null}
            </div>
            <div className="chip-add">
              <input
                className="input"
                placeholder="Añadir hashtag y pulsar Enter"
                value={tagDraft}
                onChange={(event) => setTagDraft(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === "Enter") {
                    event.preventDefault();
                    addTag();
                  }
                }}
              />
              <button className="btn" onClick={addTag}>
                <Hash /> Añadir
              </button>
            </div>
          </div>
        </div>
      </Modal>

      <Modal
        open={deleteOpen}
        title="Eliminar todo el proyecto"
        subtitle="Esta acción libera el espacio ocupado y no se puede deshacer."
        onClose={() => setDeleteOpen(false)}
        busy={deleting}
        actions={
          <>
            <button className="btn btn-ghost" onClick={() => setDeleteOpen(false)} disabled={deleting}>
              Cancelar
            </button>
            <button className="btn btn-danger" onClick={deleteProject} disabled={deleting}>
              {deleting ? <Loader2 className="spin" /> : <Trash2 />}
              Eliminar definitivamente
            </button>
          </>
        }
      >
        <div className="col" style={{ gap: 12 }}>
          <Alert tone="warning" icon={<AlertTriangle size={16} />}>
            Se eliminarán de forma conjunta:
          </Alert>
          <ul className="small muted" style={{ margin: 0, paddingLeft: 20, lineHeight: 1.9 }}>
            <li>El video original descargado o importado</li>
            <li>Los {clips.length} clips generados y sus miniaturas</li>
            <li>Archivos temporales, caché y previsualizaciones</li>
            <li>Metadatos locales y entradas de este proyecto en la cola</li>
          </ul>
          <p className="field-hint">
            Las cuentas sociales y sus credenciales se gestionan por separado y no se verán
            afectadas. ParCut no puede garantizar un borrado forense irrecuperable en unidades SSD.
          </p>
        </div>
      </Modal>

      <Modal
        open={clipToDelete !== null}
        title={`Eliminar la parte ${clipToDelete?.partIndex ?? ""}`}
        subtitle="Se borrará el archivo del clip y su miniatura."
        onClose={() => setClipToDelete(null)}
        actions={
          <>
            <button className="btn btn-ghost" onClick={() => setClipToDelete(null)}>
              Cancelar
            </button>
            <button className="btn btn-danger" onClick={deleteClip}>
              <Trash2 /> Eliminar clip
            </button>
          </>
        }
      >
        <p className="small muted">
          El video original y el resto de partes no se modifican. Si el clip está en la cola, sus
          publicaciones pendientes también se eliminarán.
        </p>
      </Modal>
    </div>
  );
}
function RangeTimeFields({
  label,
  seconds,
  max,
  onChange,
  disabled,
}: {
  label: string;
  seconds: number;
  max: number;
  onChange: (value: number) => void;
  disabled?: boolean;
}) {
  const parts = timeParts(seconds);
  const apply = (h: number, m: number, s: number) => {
    const total = partsToSeconds(
      Math.max(0, Math.min(23, h)),
      Math.max(0, Math.min(59, m)),
      Math.max(0, Math.min(59, s)),
    );
    onChange(Math.min(Math.max(0, max), total));
  };
  return (
    <div className="field">
      <label>{label}</label>
      <div className="time-input">
        <input
          className="input"
          type="number"
          min={0}
          max={23}
          value={parts.hours}
          onChange={(event) => apply(Number(event.target.value) || 0, parts.minutes, parts.seconds)}
          disabled={disabled}
          aria-label={`${label}: horas`}
        />
        <span>h</span>
        <input
          className="input"
          type="number"
          min={0}
          max={59}
          value={parts.minutes}
          onChange={(event) => apply(parts.hours, Number(event.target.value) || 0, parts.seconds)}
          disabled={disabled}
          aria-label={`${label}: minutos`}
        />
        <span>min</span>
        <input
          className="input"
          type="number"
          min={0}
          max={59}
          value={parts.seconds}
          onChange={(event) => apply(parts.hours, parts.minutes, Number(event.target.value) || 0)}
          disabled={disabled}
          aria-label={`${label}: segundos`}
        />
        <span>s</span>
      </div>
    </div>
  );
}
