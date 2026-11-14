/** Utilidades de formato y validación compartidas por la interfaz. */

export function formatDuration(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds < 0) return "00:00";
  const total = Math.round(seconds);
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const pad = (value: number) => String(value).padStart(2, "0");
  return h > 0 ? `${pad(h)}:${pad(m)}:${pad(s)}` : `${pad(m)}:${pad(s)}`;
}

export function formatDurationLong(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds <= 0) return "—";
  const total = Math.round(seconds);
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const parts: string[] = [];
  if (h > 0) parts.push(`${h} h`);
  if (m > 0) parts.push(`${m} min`);
  if (s > 0 && h === 0) parts.push(`${s} s`);
  return parts.join(" ") || "0 s";
}

export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let value = bytes;
  let index = 0;
  while (value >= 1024 && index < units.length - 1) {
    value /= 1024;
    index += 1;
  }
  const decimals = index === 0 ? 0 : value >= 100 ? 0 : 1;
  return `${value.toFixed(decimals)} ${units[index]}`;
}

export function resolutionLabel(width: number, height: number): string {
  if (!width || !height) return "—";
  const known: Record<string, string> = {
    "7680x4320": "8K",
    "3840x2160": "4K",
    "2560x1440": "2K",
    "1920x1080": "Full HD",
    "1280x720": "HD",
    "854x480": "SD",
    "640x360": "360p",
  };
  const label = known[`${width}x${height}`];
  return label ? `${width}×${height} · ${label}` : `${width}×${height}`;
}

export function defaultPartTitle(projectName: string, part: number): string {
  const name = projectName.trim() || "Video";
  return `${name} - PARTE ${part}`;
}

export function normalizeHashtags(tags: string[]): string[] {
  const seen = new Set<string>();
  const out: string[] = [];
  for (const raw of tags) {
    for (const piece of String(raw).split(/[#,\s;]+/)) {
      const clean = piece.replace(/[^\p{L}\p{N}_]/gu, "");
      if (!clean || clean.length > 60) continue;
      const key = clean.toLowerCase();
      if (seen.has(key)) continue;
      seen.add(key);
      out.push(clean);
      if (out.length >= 30) return out;
    }
  }
  return out;
}

export function parseHashtagInput(value: string): string[] {
  return normalizeHashtags([value]);
}

export function isValidHttpUrl(value: string): boolean {
  try {
    const url = new URL(value.trim());
    return url.protocol === "http:" || url.protocol === "https:";
  } catch {
    return false;
  }
}

export function fileName(path: string): string {
  const normalized = path.replace(/\\/g, "/");
  return normalized.slice(normalized.lastIndexOf("/") + 1);
}

export function fileStem(path: string): string {
  const name = fileName(path);
  const dot = name.lastIndexOf(".");
  return dot > 0 ? name.slice(0, dot) : name;
}

export function timeParts(seconds: number): { hours: number; minutes: number; seconds: number } {
  const total = Math.max(0, Math.round(seconds));
  return {
    hours: Math.floor(total / 3600),
    minutes: Math.floor((total % 3600) / 60),
    seconds: total % 60,
  };
}

export function partsToSeconds(hours: number, minutes: number, seconds: number): number {
  const h = Number.isFinite(hours) ? Math.max(0, hours) : 0;
  const m = Number.isFinite(minutes) ? Math.max(0, minutes) : 0;
  const s = Number.isFinite(seconds) ? Math.max(0, seconds) : 0;
  return Math.max(0, h * 3600 + m * 60 + s);
}

export const QUEUE_STATUS_LABELS: Record<string, string> = {
  pending: "Pendiente",
  preparing: "Preparando",
  uploading: "Subiendo",
  published: "Publicado",
  error: "Error",
  canceled: "Cancelado",
};

export const PLATFORM_LABELS: Record<string, string> = {
  youtube: "YouTube",
  facebook: "Facebook",
  tiktok: "TikTok",
};

export function platformLabel(platform: string): string {
  return PLATFORM_LABELS[platform.toLowerCase()] ?? platform;
}

export function relativeTime(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return "—";
  const diff = Date.now() - date.getTime();
  const minutes = Math.floor(diff / 60000);
  if (minutes < 1) return "ahora";
  if (minutes < 60) return `hace ${minutes} min`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `hace ${hours} h`;
  const days = Math.floor(hours / 24);
  if (days < 30) return `hace ${days} d`;
  return date.toLocaleDateString("es-ES", { day: "2-digit", month: "short", year: "numeric" });
}
