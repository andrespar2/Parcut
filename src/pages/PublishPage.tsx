import { useCallback, useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  AlertCircle,
  ArrowUpRight,
  Check,
  CheckCircle2,
  ChevronDown,
  ClipboardCopy,
  Clock,
  Globe,
  Hash,
  Loader2,
  Pause,
  Play,
  Plus,
  RotateCcw,
  Send,
  ShieldCheck,
  Sparkles,
  Square,
  X,
} from "lucide-react";
import {
  api,
  AssistedSnapshot,
  Clip,
  EnqueueClip,
  errorMessage,
  Project,
  QueueItem,
  QueueSnapshot,
} from "../lib/api";
import {
  defaultPartTitle,
  formatBytes,
  formatDuration,
  normalizeHashtags,
  platformLabel,
  QUEUE_STATUS_LABELS,
} from "../lib/format";
import { useAppStore } from "../store/app";
import { Alert, EmptyState, Modal, Pill, ProgressBar, Segmented } from "../components/ui";
import { PlatformLogo } from "../components/PlatformLogo";

type Draft = { title: string; description: string; hashtags: string[] };

function statusTone(status: string): "accent" | "success" | "warning" | "danger" | "muted" {
  switch (status) {
    case "uploading":
    case "preparing":
      return "accent";
    case "published":
      return "success";
    case "error":
      return "danger";
    case "canceled":
      return "muted";
    default:
      return "warning";
  }
}

const ASSISTED_STATUS_LABELS: Record<string, string> = {
  pending: "Pendiente",
  preparing: "Preparando publicación",
  waiting: "Esperando tu confirmación",
  published: "Publicado",
  error: "Error",
  canceled: "Cancelado",
};

function assistedTone(status: string): "accent" | "success" | "warning" | "danger" | "muted" {
  switch (status) {
    case "published":
      return "success";
    case "error":
      return "danger";
    case "canceled":
      return "muted";
    case "preparing":
    case "waiting":
      return "accent";
    default:
      return "warning";
  }
}

export function PublishPage() {
  const projects = useAppStore((state) => state.projects);
  const accounts = useAppStore((state) => state.accounts);
  const queue = useAppStore((state) => state.queue);
  const publishProjectId = useAppStore((state) => state.publishProjectId);
  const publishClipIds = useAppStore((state) => state.publishClipIds);
  const navigate = useAppStore((state) => state.navigate);
  const toast = useAppStore((state) => state.toast);
  const loadQueue = useAppStore((state) => state.loadQueue);
  const setQueue = useAppStore((state) => state.setQueue);
  const setBrowserStartAssisted = useAppStore((state) => state.setBrowserStartAssisted);

  const [assisted, setAssisted] = useState<AssistedSnapshot | null>(null);
  const [assistedBusy, setAssistedBusy] = useState<string | null>(null);
  const [assistedCancelOpen, setAssistedCancelOpen] = useState(false);

  const [projectId, setProjectId] = useState<string>(publishProjectId ?? "");
  const [project, setProject] = useState<Project | null>(null);
  const [clips, setClips] = useState<Clip[]>([]);
  const [platform, setPlatform] = useState<string>("youtube");
  const [accountId, setAccountId] = useState<string>("");
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [drafts, setDrafts] = useState<Record<string, Draft>>({});
  const [generalTags, setGeneralTags] = useState<string[]>([]);
  const [generalDescription, setGeneralDescription] = useState("");
  const [tagDraft, setTagDraft] = useState("");
  const [enqueueing, setEnqueueing] = useState(false);
  const [liveProgress, setLiveProgress] = useState<Record<string, number>>({});
  const [cancelOpen, setCancelOpen] = useState(false);
  const [loadingClips, setLoadingClips] = useState(false);
  const [expanded, setExpanded] = useState<Set<string>>(new Set());

  useEffect(() => {
    if (projects.length === 0) return;
    if (!projectId || !projects.some((item) => item.id === projectId)) {
      setProjectId(publishProjectId ?? projects[0].id);
    }
  }, [projects, publishProjectId, projectId]);

  const loadProject = useCallback(async () => {
    if (!projectId) return;
    setLoadingClips(true);
    try {
      const [loadedProject, loadedClips] = await Promise.all([
        api.projectGet(projectId),
        api.clipsList(projectId),
      ]);
      setProject(loadedProject);
      setClips(loadedClips);
      const preset = publishClipIds?.filter((id) => loadedClips.some((clip) => clip.id === id));
      setSelected(
        new Set(preset && preset.length > 0 ? preset : loadedClips.map((clip) => clip.id)),
      );
      const tags = normalizeHashtags(loadedProject.hashtags ?? []);
      setGeneralTags(tags);
      setDrafts((current) => {
        const next: Record<string, Draft> = {};
        for (const clip of loadedClips) {
          next[clip.id] = current[clip.id] ?? {
            title: defaultPartTitle(loadedProject.name, clip.partIndex),
            description: "",
            hashtags: tags,
          };
        }
        return next;
      });
    } catch (err) {
      toast("error", errorMessage(err));
    } finally {
      setLoadingClips(false);
    }
  }, [projectId, publishClipIds, toast]);

  useEffect(() => {
    void loadProject();
  }, [loadProject]);

  useEffect(() => {
    void loadQueue();
  }, [loadQueue]);

  const loadAssisted = useCallback(async () => {
    try {
      setAssisted(await api.assistedSnapshot());
    } catch {
      /* la cola asistida es independiente de las conexiones OAuth */
    }
  }, []);

  useEffect(() => {
    void loadAssisted();
    const unlisten = listen<AssistedSnapshot>("assisted:changed", (event) => {
      setAssisted(event.payload);
    });
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, [loadAssisted]);

  useEffect(() => {
    const unlisteners = [
      listen<QueueSnapshot>("queue:changed", (event) => setQueue(event.payload)),
      listen<{ itemId: string; progress: number }>("queue:progress", (event) => {
        setLiveProgress((current) => ({
          ...current,
          [event.payload.itemId]: event.payload.progress,
        }));
      }),
      listen("queue:finished", () => {
        setLiveProgress({});
        void loadQueue();
      }),
    ];
    return () => {
      void Promise.all(unlisteners).then((fns) => fns.forEach((fn) => fn()));
    };
  }, [loadQueue, setQueue]);

  const platformAccounts = useMemo(
    () => accounts.filter((account) => account.platform === platform),
    [accounts, platform],
  );

  useEffect(() => {
    if (platformAccounts.length === 0) {
      setAccountId("");
      return;
    }
    if (!platformAccounts.some((account) => account.id === accountId)) {
      setAccountId(platformAccounts[0].id);
    }
  }, [platformAccounts, accountId]);

  const selectedClips = clips.filter((clip) => selected.has(clip.id));

  const queueItems = useMemo(() => {
    const items = [...(queue?.items ?? [])];
    items.sort((a, b) => a.partIndex - b.partIndex || a.createdAt.localeCompare(b.createdAt));
    return items;
  }, [queue]);

  const activeItem = queueItems.find((item) => item.id === queue?.activeItemId);

  const overallProgress = useMemo(() => {
    if (!queue || queue.total === 0) return 0;
    const base = queue.published;
    const activePercent = activeItem ? (liveProgress[activeItem.id] ?? activeItem.progress) / 100 : 0;
    return Math.min(100, ((base + activePercent) / queue.total) * 100);
  }, [queue, activeItem, liveProgress]);

  const toggleClip = (clipId: string) => {
    setSelected((current) => {
      const next = new Set(current);
      if (next.has(clipId)) next.delete(clipId);
      else next.add(clipId);
      return next;
    });
  };

  const applyToAll = () => {
    setDrafts((current) => {
      const next = { ...current };
      for (const clip of clips) {
        next[clip.id] = {
          title: defaultPartTitle(project?.name ?? "", clip.partIndex),
          description: generalDescription,
          hashtags: generalTags,
        };
      }
      return next;
    });
    toast("info", "Títulos, descripción y hashtags aplicados a todos los clips.");
  };

  const addTag = () => {
    const parsed = normalizeHashtags([tagDraft]);
    if (parsed.length === 0) return;
    setGeneralTags((current) => normalizeHashtags([...current, ...parsed]));
    setTagDraft("");
  };

  const enqueue = async () => {
    if (!project) return;
    if (!accountId) {
      toast("error", "Conecta una cuenta de la plataforma elegida antes de publicar.");
      return;
    }
    if (selectedClips.length === 0) {
      toast("error", "Selecciona al menos un clip.");
      return;
    }
    setEnqueueing(true);
    try {
      const payload: EnqueueClip[] = selectedClips.map((clip) => {
        const draft = drafts[clip.id];
        return {
          clipId: clip.id,
          title: draft?.title.trim() || defaultPartTitle(project.name, clip.partIndex),
          description: draft?.description?.trim() ? draft.description : generalDescription,
          hashtags: draft?.hashtags?.length ? draft.hashtags : generalTags,
        };
      });
      const count = await api.queueEnqueue({
        projectId: project.id,
        platform,
        accountId,
        clips: payload,
      });
      toast("success", `${count} clip(s) añadidos a la cola.`);
      await loadQueue();
    } catch (err) {
      toast("error", errorMessage(err));
    } finally {
      setEnqueueing(false);
    }
  };

  const startQueue = async () => {
    try {
      await api.queueStart();
      toast("info", "Cola iniciada. Se publicará una parte a la vez.");
    } catch (err) {
      toast("error", errorMessage(err));
    }
  };

  const pauseQueue = async () => {
    try {
      await api.queuePause();
      toast("info", "La cola se pausará al terminar la parte actual.");
    } catch (err) {
      toast("error", errorMessage(err));
    }
  };

  const cancelRemaining = async () => {
    try {
      const count = await api.queueCancelRemaining();
      setCancelOpen(false);
      toast("info", `${count} publicaciones pendientes canceladas.`);
      await loadQueue();
    } catch (err) {
      toast("error", errorMessage(err));
    }
  };

  const retryItem = async (item: QueueItem) => {
    try {
      await api.queueRetry(item.id);
      toast("info", "Reintentando la publicación desde el último punto confirmado.");
      await loadQueue();
    } catch (err) {
      toast("error", errorMessage(err));
    }
  };

  const cancelItem = async (item: QueueItem) => {
    try {
      await api.queueCancelItem(item.id);
      await loadQueue();
    } catch (err) {
      toast("error", errorMessage(err));
    }
  };

  const updateDraft = (clipId: string, patch: Partial<Draft>) => {
    setDrafts((current) => ({
      ...current,
      [clipId]: {
        ...(current[clipId] ?? { title: "", description: "", hashtags: generalTags }),
        ...patch,
      },
    }));
  };

  const toggleExpanded = (clipId: string) => {
    setExpanded((current) => {
      const next = new Set(current);
      if (next.has(clipId)) next.delete(clipId);
      else next.add(clipId);
      return next;
    });
  };

  const assistedItems = useMemo(
    () =>
      [...(assisted?.items ?? [])].sort(
        (a, b) => a.partIndex - b.partIndex || a.createdAt.localeCompare(b.createdAt),
      ),
    [assisted],
  );
  const assistedActive = assistedItems.find((item) => item.id === assisted?.activeId) ?? null;
  const assistedNext = assistedItems.find((item) => item.status === "pending") ?? null;
  const assistedFailed = assistedItems.find((item) => item.status === "error") ?? null;

  const openAssistedInBrowser = () => {
    setBrowserStartAssisted(true);
    navigate("browser");
  };

  const runAssisted = async (key: string, action: () => Promise<AssistedSnapshot>) => {
    setAssistedBusy(key);
    try {
      setAssisted(await action());
    } catch (err) {
      toast("error", errorMessage(err));
    } finally {
      setAssistedBusy(null);
    }
  };

  const confirmAssisted = () =>
    assistedActive && runAssisted("confirm", () => api.assistedConfirm(assistedActive.id));

  const retryAssisted = () =>
    assistedFailed && runAssisted("retry", () => api.assistedRetry(assistedFailed.id));

  const cancelAssisted = async () => {
    setAssistedBusy("cancel");
    try {
      const count = await api.assistedCancelRemaining();
      setAssistedCancelOpen(false);
      toast("info", `${count} publicaciones asistidas canceladas.`);
      await loadAssisted();
    } catch (err) {
      toast("error", errorMessage(err));
    } finally {
      setAssistedBusy(null);
    }
  };

  if (projects.length === 0) {
    return (
      <div className="page">
        <EmptyState
          icon={<Sparkles size={24} />}
          title="No hay nada que publicar"
          description="Crea un proyecto, genera sus clips y vuelve aquí para programar la publicación secuencial."
          action={
            <button className="btn btn-primary" onClick={() => navigate("new")}>
              <Plus /> Nuevo proyecto
            </button>
          }
        />
      </div>
    );
  }

  return (
    <div className="page">
      <div className="section-head">
        <div>
          <h2>Publicación secuencial</h2>
          <p>
            Una parte a la vez, esperando la confirmación de la plataforma antes de continuar.
          </p>
        </div>
        <div className="spacer" />
        <button className="btn btn-sm" onClick={() => navigate("accounts")}>
          <ShieldCheck /> Cuentas
        </button>
      </div>

      <section className="card col" style={{ gap: 12 }}>
        <div className="row wrap" style={{ alignItems: "flex-start" }}>
          <div className="grow" style={{ minWidth: 240 }}>
            <h3 className="card-title">
              <Globe size={16} /> Publicación asistida por navegador
            </h3>
            <p className="card-sub" style={{ marginTop: 4 }}>
              ParCut abre la página oficial de subida con la sesión del navegador integrado. Tú
              eliges el archivo y confirmas cada parte; no se automatizan clics ni verificaciones.
            </p>
          </div>
          <div className="row wrap" style={{ gap: 6 }}>
            <button
              className="btn btn-sm"
              onClick={() =>
                void runAssisted(
                  assisted?.paused ? "resume" : "pause",
                  assisted?.paused ? () => api.assistedStart() : () => api.assistedPause(),
                )
              }
              disabled={assistedBusy !== null || (!assistedActive && !assisted?.paused)}
            >
              {assisted?.paused ? <Play /> : <Pause />}
              {assisted?.paused ? "Reanudar" : "Pausar"}
            </button>
            <button className="btn btn-sm" onClick={openAssistedInBrowser}>
              <Globe /> Abrir navegador
            </button>
          </div>
        </div>

        {assistedItems.length === 0 ? (
          <span className="small faint">
            No hay publicaciones asistidas. Selecciona clips en <strong>Mis clips</strong> y usa
            «Publicar en redes sociales» para preparar una cola.
          </span>
        ) : (
          <>
            <div className="row wrap" style={{ gap: 8 }}>
              <Pill tone="success">{assisted?.published ?? 0} publicadas</Pill>
              <Pill tone="warning">{assisted?.pending ?? 0} pendientes</Pill>
              {(assisted?.failed ?? 0) > 0 ? (
                <Pill tone="danger">{assisted?.failed} con error</Pill>
              ) : null}
              {(assisted?.canceled ?? 0) > 0 ? (
                <Pill tone="muted">{assisted?.canceled} canceladas</Pill>
              ) : null}
              {assisted?.paused ? <Pill tone="muted">En pausa</Pill> : null}
              <div className="spacer" />
              {assistedActive ? (
                <button
                  className="btn btn-sm btn-primary"
                  onClick={() => void confirmAssisted()}
                  disabled={assistedBusy !== null}
                >
                  {assistedBusy === "confirm" ? <Loader2 className="spin" /> : <Check />}
                  Confirmar parte {assistedActive.partIndex}
                </button>
              ) : assistedFailed ? (
                <button
                  className="btn btn-sm btn-primary"
                  onClick={() => void retryAssisted()}
                  disabled={assistedBusy !== null}
                >
                  {assistedBusy === "retry" ? <Loader2 className="spin" /> : <Play />}
                  Reintentar parte {assistedFailed.partIndex}
                </button>
              ) : assistedNext ? (
                <button className="btn btn-sm btn-primary" onClick={openAssistedInBrowser}>
                  <Send /> Preparar parte {assistedNext.partIndex}
                </button>
              ) : null}
              <button
                className="btn btn-sm btn-ghost"
                onClick={() => setAssistedCancelOpen(true)}
                disabled={assistedBusy !== null || (assisted?.pending ?? 0) === 0}
              >
                <X /> Cancelar restantes
              </button>
            </div>

            <div className="col" style={{ gap: 6, maxHeight: 250, overflowY: "auto" }}>
              {assistedItems.slice(0, 60).map((item) => (
                <div key={item.id} className="account-row">
                  <Pill tone={assistedTone(item.status)}>Parte {item.partIndex}</Pill>
                  <span style={{ display: "flex", flex: "none" }}>
                    <PlatformLogo platform={item.platform} size={14} />
                  </span>
                  <div className="grow" style={{ minWidth: 0 }}>
                    <div
                      style={{
                        fontSize: 12.5,
                        overflow: "hidden",
                        textOverflow: "ellipsis",
                        whiteSpace: "nowrap",
                      }}
                    >
                      {item.title}
                    </div>
                    <div
                      className="small faint"
                      style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}
                    >
                      {platformLabel(item.platform)} · {ASSISTED_STATUS_LABELS[item.status] ?? item.status}
                      {item.error ? ` · ${item.error}` : ""}
                    </div>
                  </div>
                  <div className="row" style={{ gap: 4, flex: "none" }}>
                    <button
                      className="icon-btn"
                      title="Copiar metadatos"
                      onClick={() =>
                        void navigator.clipboard.writeText(
                          [
                            item.title,
                            item.description.trim(),
                            item.hashtags.map((tag) => `#${tag}`).join(" "),
                          ]
                            .filter(Boolean)
                            .join("\n\n"),
                        )
                      }
                    >
                      <ClipboardCopy />
                    </button>
                    <button
                      className="icon-btn"
                      title="Abrir ubicación del archivo"
                      onClick={() => item.clipPath && void api.revealPath(item.clipPath)}
                      disabled={!item.clipPath}
                    >
                      <ArrowUpRight />
                    </button>
                  </div>
                </div>
              ))}
            </div>
            <span className="field-hint">
              La cola vive en SQLite y se recupera al reiniciar ParCut. Solo se marca como publicada
              una parte cuando tú la confirmas aquí.
            </span>
          </>
        )}
      </section>

      <div
        style={{
          display: "grid",
          gridTemplateColumns: "minmax(0, 1.25fr) minmax(340px, 1fr)",
          gap: 16,
          alignItems: "start",
        }}
      >
        <section className="card col" style={{ gap: 16 }}>
          <div>
            <h3 className="card-title">
              <Plus size={16} /> Preparar publicación
            </h3>
            <p className="card-sub" style={{ marginTop: 4 }}>
              Revisa títulos, descripción y hashtags antes de añadir los clips a la cola.
            </p>
          </div>

          <div className="input-row">
            <div className="field">
              <label htmlFor="publish-project">Proyecto</label>
              <select
                id="publish-project"
                className="select"
                value={projectId}
                onChange={(event) => setProjectId(event.target.value)}
              >
                {projects.map((item) => (
                  <option key={item.id} value={item.id}>
                    {item.name} ({item.clipCount} clips)
                  </option>
                ))}
              </select>
            </div>
            <div className="field">
              <label>Plataforma</label>
              <Segmented
                value={platform}
                onChange={(value) => setPlatform(value)}
                options={[
                  { value: "youtube", label: "YouTube" },
                  { value: "facebook", label: "Facebook" },
                  { value: "tiktok", label: "TikTok" },
                ]}
              />
            </div>
            <div className="field">
              <label htmlFor="publish-account">Cuenta conectada</label>
              <select
                id="publish-account"
                className="select"
                value={accountId}
                onChange={(event) => setAccountId(event.target.value)}
                disabled={platformAccounts.length === 0}
              >
                {platformAccounts.length === 0 ? (
                  <option value="">Sin cuentas disponibles</option>
                ) : (
                  platformAccounts.map((account) => (
                    <option key={account.id} value={account.id}>
                      {account.displayName}
                    </option>
                  ))
                )}
              </select>
            </div>
          </div>

          {platformAccounts.length === 0 ? (
            <Alert tone="warning" icon={<AlertCircle size={16} />}>
              No hay una cuenta de {platformLabel(platform)} conectada. ParCut no simula
              conexiones: ve a <strong>Cuentas conectadas</strong> para autorizar la plataforma con
              tus propias credenciales de desarrollador.
              <div style={{ marginTop: 8 }}>
                <button className="btn btn-sm" onClick={() => navigate("accounts")}>
                  <ShieldCheck /> Configurar {platformLabel(platform)}
                </button>
              </div>
            </Alert>
          ) : null}

          {loadingClips ? (
            <div className="row" style={{ justifyContent: "center", padding: 24 }}>
              <Loader2 className="spin" />
            </div>
          ) : clips.length === 0 ? (
            <Alert tone="warning">Este proyecto no tiene clips. Divide el video primero.</Alert>
          ) : (
            <>
              <div className="row between">
                <span className="small muted">
                  {selected.size} de {clips.length} clips seleccionados
                </span>
                <div className="row" style={{ gap: 6 }}>
                  <button
                    className="btn btn-sm btn-ghost"
                    onClick={() =>
                      setSelected(
                        selected.size === clips.length
                          ? new Set()
                          : new Set(clips.map((clip) => clip.id)),
                      )
                    }
                  >
                    {selected.size === clips.length ? "Deseleccionar" : "Seleccionar todo"}
                  </button>
                </div>
              </div>

              <div className="col" style={{ gap: 8, maxHeight: 320, overflowY: "auto", paddingRight: 4 }}>
                {clips.map((clip) => {
                  const draft = drafts[clip.id];
                  const isSelected = selected.has(clip.id);
                  const isExpanded = expanded.has(clip.id);
                  return (
                    <div
                      key={clip.id}
                      style={{
                        border: "1px solid var(--border)",
                        borderRadius: 12,
                        background: isSelected ? "var(--surface)" : "transparent",
                        padding: "10px 12px",
                      }}
                    >
                      <div className="row" style={{ gap: 10 }}>
                        <button
                          className={`clip-check${isSelected ? " on" : ""}`}
                          style={{
                            position: "static",
                            width: 20,
                            height: 20,
                            borderRadius: 6,
                            background: isSelected ? "var(--grad)" : "var(--surface)",
                            border: "1px solid var(--border-strong)",
                          }}
                          onClick={() => toggleClip(clip.id)}
                          aria-label="Seleccionar clip"
                        >
                          {isSelected ? <CheckCircle2 size={13} /> : null}
                        </button>
                        <span className="pill muted" style={{ flex: "none" }}>
                          Parte {clip.partIndex}
                        </span>
                        <input
                          className="input"
                          style={{ height: 34, minHeight: 34, fontSize: 12.5 }}
                          value={draft?.title ?? ""}
                          onChange={(event) =>
                            updateDraft(clip.id, { title: event.target.value })
                          }
                          maxLength={120}
                          aria-label={`Título de la parte ${clip.partIndex}`}
                        />
                        <span className="small faint mono-num" style={{ flex: "none" }}>
                          {formatDuration(clip.duration)}
                        </span>
                        <button
                          className="icon-btn"
                          onClick={() => toggleExpanded(clip.id)}
                          title="Descripción y hashtags propios"
                        >
                          <ChevronDown
                            size={15}
                            style={{ transform: isExpanded ? "rotate(180deg)" : undefined }}
                          />
                        </button>
                      </div>
                      {isExpanded ? (
                        <div className="col" style={{ gap: 8, marginTop: 10 }}>
                          <textarea
                            className="textarea"
                            style={{ minHeight: 64, fontSize: 12.5 }}
                            placeholder="Descripción solo para esta parte (opcional)"
                            value={draft?.description ?? ""}
                            onChange={(event) =>
                              updateDraft(clip.id, { description: event.target.value })
                            }
                          />
                          <input
                            className="input"
                            style={{ height: 34, minHeight: 34, fontSize: 12.5 }}
                            placeholder="Hashtags para esta parte, separados por espacios"
                            value={(draft?.hashtags ?? []).join(" ")}
                            onChange={(event) =>
                              updateDraft(clip.id, {
                                hashtags: normalizeHashtags([event.target.value]),
                              })
                            }
                          />
                        </div>
                      ) : null}
                    </div>
                  );
                })}
              </div>

              <div className="divider" />

              <div className="field">
                <label>Hashtags generales</label>
                <div className="chips">
                  {generalTags.map((tag) => (
                    <span key={tag} className="chip">
                      #{tag}
                      <button
                        onClick={() =>
                          setGeneralTags(generalTags.filter((item) => item !== tag))
                        }
                        aria-label={`Quitar ${tag}`}
                      >
                        <X size={11} />
                      </button>
                    </span>
                  ))}
                  {generalTags.length === 0 ? (
                    <span className="small faint">Sin hashtags generales</span>
                  ) : null}
                </div>
                <div className="chip-add">
                  <input
                    className="input"
                    placeholder="#viaje #colombia"
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

              <div className="field">
                <label htmlFor="general-description">Descripción general</label>
                <textarea
                  id="general-description"
                  className="textarea"
                  placeholder="Se añadirá al pie de cada publicación junto con los hashtags."
                  value={generalDescription}
                  onChange={(event) => setGeneralDescription(event.target.value)}
                  maxLength={5000}
                />
              </div>

              <div className="row wrap">
                <button className="btn" onClick={applyToAll}>
                  <Sparkles /> Aplicar a todos
                </button>
                <span className="small faint">
                  Los títulos mantienen el formato «NOMBRE - PARTE N».
                </span>
              </div>

              <button
                className="btn btn-primary btn-lg btn-block"
                onClick={enqueue}
                disabled={enqueueing || selectedClips.length === 0 || !accountId}
              >
                {enqueueing ? <Loader2 className="spin" /> : <Plus />}
                Añadir {selectedClips.length} clip(s) a la cola
              </button>
            </>
          )}
        </section>

        <section className="card col" style={{ gap: 14 }}>
          <div className="row between" style={{ alignItems: "flex-start" }}>
            <div>
              <h3 className="card-title">
                <Clock size={16} /> Seguimiento de la cola
              </h3>
              <p className="card-sub" style={{ marginTop: 4 }}>
                Persistente: si cierras ParCut, el progreso se conserva.
              </p>
            </div>
            <Pill
              tone={
                queue?.running
                  ? "accent"
                  : queue?.failed
                    ? "danger"
                    : queue && queue.pending > 0
                      ? "warning"
                      : "muted"
              }
            >
              {queue?.running
                ? "En curso"
                : queue?.failed
                  ? "Pausada por error"
                  : queue && queue.pending > 0
                    ? "Pausada"
                    : "Sin tareas"}
            </Pill>
          </div>

          {queue && queue.total > 0 ? (
            <>
              <div>
                <div className="row between small" style={{ marginBottom: 6 }}>
                  <span>
                    {queue.published} de {queue.total} publicados
                  </span>
                  <span className="mono-num faint">{Math.round(overallProgress)}%</span>
                </div>
                <ProgressBar value={overallProgress} />
              </div>

              {activeItem ? (
                <div className="card flat">
                  <div className="row between small" style={{ marginBottom: 6 }}>
                    <span>
                      Subiendo parte {activeItem.partIndex} · {platformLabel(activeItem.platform)}
                    </span>
                    <span className="mono-num faint">
                      {Math.round(liveProgress[activeItem.id] ?? activeItem.progress)}%
                    </span>
                  </div>
                  <ProgressBar
                    value={liveProgress[activeItem.id] ?? activeItem.progress}
                    thin
                  />
                </div>
              ) : null}

              <div className="row wrap" style={{ gap: 8 }}>
                {!queue.running ? (
                  <button
                    className="btn btn-primary btn-sm"
                    onClick={startQueue}
                    disabled={queue.pending === 0}
                  >
                    <Play /> {queue.paused && queue.pending > 0 ? "Reanudar cola" : "Iniciar cola"}
                  </button>
                ) : (
                  <button className="btn btn-sm" onClick={pauseQueue}>
                    <Pause /> Pausar tras la actual
                  </button>
                )}
                <button
                  className="btn btn-sm btn-ghost"
                  onClick={() => setCancelOpen(true)}
                  disabled={queue.pending === 0 && !queue.activeItemId}
                >
                  <Square /> Cancelar restantes
                </button>
                <button className="btn btn-sm btn-ghost" onClick={() => void loadQueue()}>
                  <RotateCcw /> Actualizar
                </button>
              </div>
            </>
          ) : (
            <p className="small faint">
              Aún no hay publicaciones en cola. Añade clips desde el panel izquierdo.
            </p>
          )}

          <div className="divider" />

          {queueItems.length === 0 ? (
            <p className="small faint">La cola está vacía.</p>
          ) : (
            <div className="col" style={{ gap: 8, maxHeight: 460, overflowY: "auto", paddingRight: 4 }}>
              {queueItems.map((item) => {
                const progress = liveProgress[item.id] ?? item.progress;
                const isActive = item.id === queue?.activeItemId;
                return (
                  <div
                    key={item.id}
                    className="col"
                    style={{
                      gap: 6,
                      padding: "10px 12px",
                      borderRadius: 11,
                      border: `1px solid ${isActive ? "rgba(110,231,255,0.35)" : "var(--border)"}`,
                      background: isActive
                        ? "linear-gradient(120deg, rgba(94,231,255,0.08), rgba(127,140,255,0.05))"
                        : "var(--surface)",
                    }}
                  >
                    <div className="row" style={{ gap: 8 }}>
                      <span className="part" style={{ flex: "none" }}>
                        {item.partIndex}
                      </span>
                      <div className="grow" style={{ minWidth: 0 }}>
                        <div className="q-title" title={item.title}>
                          {item.title}
                        </div>
                        <div className="q-sub">
                          {platformLabel(item.platform)} · {item.accountName || "cuenta"}
                          {item.attempts > 1 ? ` · intento ${item.attempts}` : ""}
                        </div>
                      </div>
                      <Pill tone={statusTone(item.status)}>
                        {QUEUE_STATUS_LABELS[item.status] ?? item.status}
                      </Pill>
                    </div>
                    {item.status === "uploading" ? (
                      <ProgressBar value={progress} thin />
                    ) : null}
                    {item.error ? (
                      <div className="small" style={{ color: "var(--danger)", lineHeight: 1.45 }}>
                        {item.error}
                      </div>
                    ) : null}
                    {item.status === "published" && item.remoteUrl ? (
                      <button
                        className="btn btn-sm btn-ghost"
                        style={{ alignSelf: "flex-start", height: 26 }}
                        onClick={() => void api.openExternal(item.remoteUrl as string)}
                      >
                        <ArrowUpRight size={13} /> Ver publicación
                      </button>
                    ) : null}
                    <div className="row" style={{ gap: 4 }}>
                      {item.status === "error" || item.status === "canceled" ? (
                        <button
                          className="btn btn-sm"
                          onClick={() => void retryItem(item)}
                          disabled={queue?.running && isActive}
                        >
                          <RotateCcw /> Reintentar
                        </button>
                      ) : null}
                      {item.status === "pending" || item.status === "error" ? (
                        <button
                          className="btn btn-sm btn-ghost"
                          onClick={() => void cancelItem(item)}
                        >
                          <X /> Cancelar
                        </button>
                      ) : null}
                    </div>
                  </div>
                );
              })}
            </div>
          )}

          <div className="alert">
            <ShieldCheck size={16} />
            <div>
              Solo se confirma una publicación cuando la plataforma devuelve su identificador.
              Si algo falla, la cola se pausa para evitar duplicados y podrás reintentar esa parte.
            </div>
          </div>
        </section>
      </div>

      <Modal
        open={cancelOpen}
        title="Cancelar publicaciones restantes"
        subtitle="Se cancelarán las partes pendientes y se detendrá la subida activa."
        onClose={() => setCancelOpen(false)}
        actions={
          <>
            <button className="btn btn-ghost" onClick={() => setCancelOpen(false)}>
              Volver
            </button>
            <button className="btn btn-danger" onClick={cancelRemaining}>
              <Square /> Cancelar restantes
            </button>
          </>
        }
      >
        <p className="small muted">
          Las publicaciones ya completadas permanecen intactas y enlazadas a su ID real. Esta
          acción no elimina clips ni archivos del proyecto.
        </p>
      </Modal>

      <Modal
        open={assistedCancelOpen}
        title="Cancelar publicaciones asistidas restantes"
        subtitle="Se cancelarán las partes pendientes de la cola del navegador."
        onClose={() => setAssistedCancelOpen(false)}
        busy={assistedBusy === "cancel"}
        actions={
          <>
            <button
              className="btn btn-ghost"
              onClick={() => setAssistedCancelOpen(false)}
              disabled={assistedBusy === "cancel"}
            >
              Volver
            </button>
            <button
              className="btn btn-danger"
              onClick={() => void cancelAssisted()}
              disabled={assistedBusy === "cancel"}
            >
              {assistedBusy === "cancel" ? <Loader2 className="spin" /> : <X />}
              Cancelar restantes
            </button>
          </>
        }
      >
        <p className="small muted">
          Las partes ya confirmadas como publicadas no se modifican. Los clips y archivos del
          proyecto permanecen en disco.
        </p>
      </Modal>

      <div className="row" style={{ gap: 8 }}>
        <PlatformLogo platform={platform} size={16} />
        <span className="small faint">
          Los límites de título y descripción se ajustan automáticamente a {platformLabel(platform)}.
        </span>
        {project ? (
          <span className="small faint">
            · {formatBytes(project.sizeBytes)} de material · {formatDuration(project.duration)}
          </span>
        ) : null}
      </div>
    </div>
  );
}
