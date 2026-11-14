import { convertFileSrc } from "@tauri-apps/api/core";
import { Film, Play } from "lucide-react";
import { Project } from "../lib/api";
import { formatBytes, formatDuration, relativeTime, resolutionLabel } from "../lib/format";
import { useAppStore } from "../store/app";

export function ProjectCard({ project }: { project: Project }) {
  const navigate = useAppStore((state) => state.navigate);
  const source = project.thumbnailPath ? convertFileSrc(project.thumbnailPath) : null;

  return (
    <button className="project-card" onClick={() => navigate("project", { projectId: project.id })}>
      <div className="thumb">
        {source ? (
          <img src={source} alt="" loading="lazy" />
        ) : (
          <div className="fallback">
            <Film size={32} />
          </div>
        )}
        <span className="badge-float pill accent">{project.platform ?? "Local"}</span>
        <span className="duration">{formatDuration(project.duration)}</span>
        <div className="play" style={{ opacity: 0 }}>
          <span>
            <Play size={18} />
          </span>
        </div>
      </div>
      <div className="project-card-body">
        <h3 title={project.name}>{project.name}</h3>
        <div className="project-card-meta">
          <span>{project.clipCount} clips</span>
          <span>{resolutionLabel(project.width, project.height)}</span>
          <span>{formatBytes(project.sizeBytes)}</span>
        </div>
        <div className="project-card-meta">
          <span>Actualizado {relativeTime(project.updatedAt)}</span>
          {project.publishedCount > 0 ? (
            <span style={{ color: "var(--success)" }}>{project.publishedCount} publicados</span>
          ) : null}
        </div>
      </div>
    </button>
  );
}
