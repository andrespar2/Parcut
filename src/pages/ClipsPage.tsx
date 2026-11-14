import { useCallback, useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  Clapperboard,
  Filter,
  Hash,
  Loader2,
  PlusCircle,
  Send,
  Share2,
  Trash2,
} from "lucide-react";
import { api, Clip, errorMessage } from "../lib/api";
import {
  defaultPartTitle,
  formatBytes,
  formatDuration,
  normalizeHashtags,
  platformLabel,
} from "../lib/format";
import { useAppStore } from "../store/app";
import { Alert, EmptyState, Modal, Pill, Segmented } from "../components/ui";
import { ClipCard } from "../components/ClipCard";
import { PlayerModal } from "../components/PlayerModal";

export function ClipsPage() {
  const projects = useAppStore((state) => state.projects);
  const navigate = useAppStore((state) => state.navigate);
  const toast = useAppStore((state) => state.toast);
  const refreshAll = useAppStore((state) => state.refreshAll);
  const settings = useAppStore((state) => state.settings);
  const setBrowserStartAssisted = useAppStore((state) => state.setBrowserStartAssisted);

  const [clipsByProject, setClipsByProject] = useState<Record<string, Clip[]>>({});
  const [loading, setLoading] = useState(true);
  const [filter, setFilter] = useState<string>("all");
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [playing, setPlaying] = useState<Clip | null>(null);
  const [clipToDelete, setClipToDelete] = useState<Clip | null>(null);
  const [publishOpen, setPublishOpen] = useState(false);
  const [publishPlatform, setPublishPlatform] = useState<"youtube" | "facebook" | "tiktok">(
    "youtube",
  );
  const [titles, setTitles] = useState<Record<string, string>>({});
  const [publishTags, setPublishTags] = useState<string[]>([]);
  const [publishDescription, setPublishDescription] = useState("");
  const [tagDraft, setTagDraft] = useState("");
  const [enqueueing, setEnqueueing] = useState(false);

  const load = useCallback(async () => {
    if (projects.length === 0) {
      setClipsByProject({});
      setLoading(false);
      return;
    }
    setLoading(true);
    try {
      const entries = await Promise.all(
        projects.map(async (project) => {
          try {
            const clips = await api.clipsList(project.id);
            return [project.id, clips] as const;
          } catch {
            return [project.id, [] as Clip[]] as const;
          }
        }),
      );
      setClipsByProject(Object.fromEntries(entries));
      setSelected((current) => {
        const valid = new Set(entries.flatMap(([, clips]) => clips.map((clip) => clip.id)));
        return new Set([...current].filter((id) => valid.has(id)));
      });
    } finally {
      setLoading(false);
    }
  }, [projects]);

  useEffect(() => {
    void load();
  }, [load]);

  useEffect(() => {
    const unlisteners = [
      listen("clips:changed", () => void load()),
      listen("project:updated", () => void load()),
    ];
    return () => {
      void Promise.all(unlisteners).then((fns) => fns.forEach((fn) => fn()));
    };
  }, [load]);

  const projectsWithClips = useMemo(
    () => projects.filter((project) => (clipsByProject[project.id]?.length ?? 0) > 0),
    [projects, clipsByProject],
  );

  const visible = useMemo(() => {
    const list = projectsWithClips
      .filter((project) => filter === "all" || project.id === filter)
      .map((project) => ({
        project,
        clips: [...(clipsByProject[project.id] ?? [])].sort(
          (a, b) => a.partIndex - b.partIndex,
        ),
      }));
    return list;
  }, [projectsWithClips, filter, clipsByProject]);

  const totalClips = visible.reduce((sum, group) => sum + group.clips.length, 0);
  const selectedClips = visible.flatMap((group) =>
    group.clips.filter((clip) => selected.has(clip.id)),
  );
  const uniqueProject = useMemo(() => {
    const ids = new Set(selectedClips.map((clip) => clip.projectId));
    return ids.size === 1 ? [...ids][0] : null;
  }, [selectedClips]);

  const toggle = (clipId: string) => {
    setSelected((current) => {
      const next = new Set(current);
      if (next.has(clipId)) next.delete(clipId);
      else next.add(clipId);
      return next;
    });
  };

  const toggleGroup = (clips: Clip[]) => {
    const allSelected = clips.every((clip) => selected.has(clip.id));
    setSelected((current) => {
      const next = new Set(current);
      for (const clip of clips) {
        if (allSelected) next.delete(clip.id);
        else next.add(clip.id);
      }
      return next;
    });
  };

  const goPublish = () => {
    if (selectedClips.length === 0) return;
    if (!uniqueProject) {
      toast("info", "Selecciona clips de un solo proyecto para publicarlos juntos.");
      return;
    }
    const project = projects.find((item) => item.id === uniqueProject);
    const tags = normalizeHashtags(project?.hashtags ?? []);
    setPublishTags(tags);
    setPublishDescription("");
    setTagDraft("");
    setPublishPlatform("youtube");
    setTitles(
      Object.fromEntries(
        selectedClips.map((clip) => [
          clip.id,
          defaultPartTitle(project?.name ?? "Video", clip.partIndex),
        ]),
      ),
    );
    setPublishOpen(true);
  };

  const addPublishTag = () => {
    const parsed = normalizeHashtags([tagDraft]);
    if (parsed.length === 0) return;
    setPublishTags((current) => normalizeHashtags([...current, ...parsed]));
    setTagDraft("");
  };

  const enqueueAssisted = async () => {
    if (!uniqueProject) return;
    setEnqueueing(true);
    try {
      const ordered = [...selectedClips].sort((a, b) => a.partIndex - b.partIndex);
      const count = await api.assistedEnqueue({
        projectId: uniqueProject,
        platform: publishPlatform,
        clips: ordered.map((clip) => ({
          clipId: clip.id,
          title: titles[clip.id]?.trim() || defaultPartTitle("Video", clip.partIndex),
          description: publishDescription,
          hashtags: publishTags,
        })),
      });
      toast(
        "success",
        `${count} clip(s) listos. Se abrirá ${platformLabel(publishPlatform)} para publicar la primera parte.`,
      );
      setPublishOpen(false);
      setBrowserStartAssisted(true);
      navigate("browser");
    } catch (err) {
      toast("error", errorMessage(err));
    } finally {
      setEnqueueing(false);
    }
  };

  const confirmDelete = async () => {
    if (!clipToDelete) return;
    try {
      await api.clipDelete(clipToDelete.id);
      toast("success", "Clip eliminado.");
      setClipToDelete(null);
      await Promise.all([load(), refreshAll()]);
    } catch (err) {
      toast("error", errorMessage(err));
    }
  };

  return (
    <div className="page">
      <div className="section-head" style={{ marginBottom: 0 }}>
        <div>
          <h2>Mis clips</h2>
          <p>
            {totalClips} clips en {visible.length} proyecto(s), siempre ordenados por número de
            parte.
          </p>
        </div>
        <div className="spacer" />
        <button
          className="btn btn-sm btn-primary"
          disabled={selectedClips.length === 0}
          onClick={goPublish}
        >
          <Share2 />
          Publicar en redes sociales ({selectedClips.length})
        </button>
        <button className="btn btn-sm" onClick={() => navigate("new")}>
          <PlusCircle /> Nuevo proyecto
        </button>
      </div>

      {selectedClips.length > 0 && !uniqueProject ? (
        <Alert tone="warning">
          Hay clips seleccionados de varios proyectos. La cola publica un proyecto a la vez;
          selecciona clips de un único proyecto.
        </Alert>
      ) : null}

      {projectsWithClips.length > 1 ? (
        <div className="row wrap" style={{ gap: 6 }}>
          <span className="small faint row" style={{ gap: 6 }}>
            <Filter size={13} /> Filtrar
          </span>
          <button
            className={`btn btn-sm${filter === "all" ? " btn-primary" : ""}`}
            onClick={() => setFilter("all")}
          >
            Todos
          </button>
          {projectsWithClips.map((project) => (
            <button
              key={project.id}
              className={`btn btn-sm${filter === project.id ? " btn-primary" : ""}`}
              onClick={() => setFilter(project.id)}
            >
              {project.name.length > 28 ? `${project.name.slice(0, 28)}…` : project.name}
            </button>
          ))}
        </div>
      ) : null}

      {loading ? (
        <div className="card row" style={{ justifyContent: "center", padding: 38 }}>
          <Loader2 className="spin" /> <span className="muted">Cargando clips…</span>
        </div>
      ) : totalClips === 0 ? (
        <EmptyState
          icon={<Clapperboard size={24} />}
          title="No hay clips todavía"
          description="Abre un proyecto, elige la duración de las partes y divide el video. Los clips aparecerán aquí automáticamente."
          action={
            <button className="btn btn-primary" onClick={() => navigate("new")}>
              <PlusCircle /> Crear proyecto
            </button>
          }
        />
      ) : (
        visible.map(({ project, clips }) => (
          <section key={project.id}>
            <div className="section-head" style={{ marginBottom: 10 }}>
              <div>
                <h3 style={{ margin: 0, fontSize: 15, display: "flex", alignItems: "center", gap: 8 }}>
                  {project.name}
                  <Pill tone="accent">{platformLabel(project.platform ?? "local")}</Pill>
                </h3>
                <p>
                  {clips.length} partes · {formatDuration(project.duration)} ·{" "}
                  {formatBytes(project.sizeBytes)}
                </p>
              </div>
              <div className="spacer" />
              <button className="btn btn-sm" onClick={() => toggleGroup(clips)}>
                {clips.every((clip) => selected.has(clip.id)) ? "Deseleccionar" : "Seleccionar"}
              </button>
              <button
                className="btn btn-sm btn-ghost"
                onClick={() => navigate("project", { projectId: project.id })}
              >
                Abrir proyecto
              </button>
            </div>
            <div className="clip-grid">
              {clips.map((clip) => (
                <ClipCard
                  key={clip.id}
                  clip={clip}
                  selected={selected.has(clip.id)}
                  onToggle={() => toggle(clip.id)}
                  onPlay={() => setPlaying(clip)}
                  onDelete={() => setClipToDelete(clip)}
                />
              ))}
            </div>
          </section>
        ))
      )}

      <PlayerModal
        open={playing !== null}
        src={playing?.path ?? null}
        title={
          playing
            ? `${projects.find((project) => project.id === playing.projectId)?.name ?? "Clip"} · Parte ${playing.partIndex}`
            : ""
        }
        subtitle={playing ? `${formatDuration(playing.duration)} · ${formatBytes(playing.sizeBytes)}` : ""}
        muted={settings?.previewMuted ?? false}
        autoPlay={settings?.previewAutoplay ?? false}
        onClose={() => setPlaying(null)}
      />

      <Modal
        open={clipToDelete !== null}
        title={`Eliminar la parte ${clipToDelete?.partIndex ?? ""}`}
        subtitle="El archivo y su miniatura se borrarán del disco."
        onClose={() => setClipToDelete(null)}
        actions={
          <>
            <button className="btn btn-ghost" onClick={() => setClipToDelete(null)}>
              Cancelar
            </button>
            <button className="btn btn-danger" onClick={confirmDelete}>
              <Trash2 /> Eliminar clip
            </button>
          </>
        }
      >
        <p className="small muted">
          Esta acción no afecta al video original ni al resto de clips del proyecto.
        </p>
      </Modal>

      <Modal
        open={publishOpen}
        title="Publicar en redes sociales"
        subtitle="Publicación asistida: ParCut abre la plataforma y tú confirmas cada parte."
        onClose={() => setPublishOpen(false)}
        wide
        busy={enqueueing}
        actions={
          <>
            <button
              className="btn btn-ghost"
              onClick={() => setPublishOpen(false)}
              disabled={enqueueing}
            >
              Cancelar
            </button>
            <button
              className="btn btn-primary"
              onClick={enqueueAssisted}
              disabled={enqueueing || selectedClips.length === 0}
            >
              {enqueueing ? <Loader2 className="spin" /> : <Send />}
              Añadir a la cola y abrir {platformLabel(publishPlatform)}
            </button>
          </>
        }
      >
        <div className="col" style={{ gap: 14 }}>
          <Alert tone="info">
            Se usará la sesión que ya hayas iniciado en el navegador de ParCut. No se necesitan
            credenciales de desarrollador. Tú eliges el archivo y confirmas en ParCut cuando la
            plataforma haya publicado la parte.
          </Alert>

          <div className="field">
            <label>Plataforma de destino</label>
            <Segmented
              value={publishPlatform}
              onChange={setPublishPlatform}
              options={[
                { value: "youtube", label: "YouTube" },
                { value: "facebook", label: "Facebook" },
                { value: "tiktok", label: "TikTok" },
              ]}
            />
            <span className="field-hint">
              Si no tienes sesión iniciada, podrás hacerlo en la página oficial que se abrirá en el
              navegador integrado.
            </span>
          </div>

          <div className="field">
            <label>Clips y títulos (orden ascendente)</label>
            <div className="col" style={{ gap: 8, maxHeight: 260, overflowY: "auto", paddingRight: 4 }}>
              {[...selectedClips]
                .sort((a, b) => a.partIndex - b.partIndex)
                .map((clip) => (
                  <div key={clip.id} className="row" style={{ gap: 8 }}>
                    <span className="pill muted" style={{ flex: "none" }}>
                      Parte {clip.partIndex}
                    </span>
                    <input
                      className="input"
                      value={titles[clip.id] ?? ""}
                      onChange={(event) =>
                        setTitles((current) => ({ ...current, [clip.id]: event.target.value }))
                      }
                      spellCheck={false}
                      aria-label={`Título de la parte ${clip.partIndex}`}
                    />
                    <span className="small faint" style={{ flex: "none" }}>
                      {formatDuration(clip.duration)}
                    </span>
                  </div>
                ))}
            </div>
          </div>

          <div className="field">
            <label>Hashtags generales</label>
            <div className="chips">
              {publishTags.map((tag) => (
                <span key={tag} className="chip">
                  #{tag}
                  <button
                    onClick={() => setPublishTags(publishTags.filter((item) => item !== tag))}
                    aria-label={`Quitar ${tag}`}
                  >
                    ×
                  </button>
                </span>
              ))}
              {publishTags.length === 0 ? <span className="small faint">Sin hashtags</span> : null}
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
                    addPublishTag();
                  }
                }}
              />
              <button className="btn" onClick={addPublishTag}>
                <Hash /> Añadir
              </button>
            </div>
          </div>

          <div className="field">
            <label>Descripción general</label>
            <textarea
              className="textarea"
              placeholder="Se copiará junto con el título para que la pegues en la plataforma."
              value={publishDescription}
              onChange={(event) => setPublishDescription(event.target.value)}
              maxLength={5000}
            />
          </div>
        </div>
      </Modal>
    </div>
  );
}
