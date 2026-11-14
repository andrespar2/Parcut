import { convertFileSrc } from "@tauri-apps/api/core";
import { Check, Copy, Film, FolderOpen, Play, Trash2 } from "lucide-react";
import { api, Clip } from "../lib/api";
import { formatBytes, formatDuration, fileName, resolutionLabel } from "../lib/format";
import { useAppStore } from "../store/app";

export function ClipCard({
  clip,
  projectName,
  selected,
  selectionEnabled = true,
  onToggle,
  onPlay,
  onDelete,
}: {
  clip: Clip;
  projectName?: string;
  selected?: boolean;
  selectionEnabled?: boolean;
  onToggle?: () => void;
  onPlay: () => void;
  onDelete?: () => void;
}) {
  const toast = useAppStore((state) => state.toast);
  const source = clip.thumbnailPath ? convertFileSrc(clip.thumbnailPath) : null;

  const copyName = async () => {
    try {
      await navigator.clipboard.writeText(fileName(clip.path));
      toast("success", "Nombre del clip copiado.");
    } catch {
      toast("error", "No se pudo copiar el nombre al portapapeles.");
    }
  };

  const reveal = async () => {
    try {
      await api.revealPath(clip.path);
    } catch (error) {
      toast("error", String(error));
    }
  };

  return (
    <article className={`clip-card${selected ? " selected" : ""}`}>
      <div className="clip-thumb" onClick={onPlay} role="button" tabIndex={0}
        onKeyDown={(event) => {
          if (event.key === "Enter" || event.key === " ") {
            event.preventDefault();
            onPlay();
          }
        }}
      >
        {source ? (
          <img src={source} alt={`Miniatura de la parte ${clip.partIndex}`} loading="lazy" />
        ) : (
          <div className="thumb">
            <div className="fallback">
              <Film size={30} />
            </div>
          </div>
        )}
        <span className="part-badge">PARTE {clip.partIndex}</span>
        {selectionEnabled ? (
          <button
            className={`clip-check${selected ? " on" : ""}`}
            aria-label={selected ? "Quitar de la selección" : "Seleccionar clip"}
            onClick={(event) => {
              event.stopPropagation();
              onToggle?.();
            }}
          >
            {selected ? <Check size={14} /> : null}
          </button>
        ) : null}
        <div className="play">
          <span>
            <Play size={19} fill="currentColor" />
          </span>
        </div>
      </div>
      <div className="clip-body">
        <div className="clip-title" title={projectName ? `${projectName} · Parte ${clip.partIndex}` : clip.path}>
          {projectName ? `${projectName} · Parte ${clip.partIndex}` : `Parte ${clip.partIndex}`}
        </div>
        <div className="clip-meta">
          <span>{formatDuration(clip.duration)}</span>
          <span>{resolutionLabel(clip.width, clip.height)}</span>
          <span>{formatBytes(clip.sizeBytes)}</span>
        </div>
        <div className="clip-actions">
          <button className="icon-btn" title="Reproducir" onClick={onPlay}>
            <Play />
          </button>
          <button className="icon-btn" title="Abrir ubicación" onClick={reveal}>
            <FolderOpen />
          </button>
          <button className="icon-btn" title="Copiar nombre" onClick={copyName}>
            <Copy />
          </button>
          {onDelete ? (
            <button
              className="icon-btn danger"
              title="Eliminar clip"
              onClick={onDelete}
              style={{ marginLeft: "auto" }}
            >
              <Trash2 />
            </button>
          ) : null}
        </div>
      </div>
    </article>
  );
}
