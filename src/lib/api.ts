import { invoke } from "@tauri-apps/api/core";

/* ------------------------------------------------------------------ Tipos */

export interface Project {
  id: string;
  name: string;
  sourceUrl: string | null;
  platform: string | null;
  originalPath: string | null;
  thumbnailPath: string | null;
  duration: number;
  width: number;
  height: number;
  fps: number;
  sizeBytes: number;
  videoCodec: string | null;
  audioCodec: string | null;
  container: string | null;
  status: string;
  hashtags: string[];
  defaultClipSeconds: number;
  createdAt: string;
  updatedAt: string;
  clipCount: number;
  publishedCount: number;
}

export interface Clip {
  id: string;
  projectId: string;
  partIndex: number;
  path: string;
  thumbnailPath: string | null;
  startSeconds: number;
  endSeconds: number;
  duration: number;
  width: number;
  height: number;
  sizeBytes: number;
  status: string;
  createdAt: string;
}

export interface Account {
  id: string;
  platform: string;
  displayName: string;
  externalId: string;
  scopes: string;
  connectedAt: string;
  updatedAt: string;
}

export type QueueStatus =
  | "pending"
  | "preparing"
  | "uploading"
  | "published"
  | "error"
  | "canceled";

export interface QueueItem {
  id: string;
  projectId: string;
  clipId: string;
  projectName: string;
  partIndex: number;
  platform: string;
  accountId: string;
  accountName: string;
  title: string;
  description: string;
  hashtags: string[];
  status: QueueStatus;
  progress: number;
  remoteId: string | null;
  remoteUrl: string | null;
  error: string | null;
  attempts: number;
  createdAt: string;
  updatedAt: string;
}

export interface QueueSnapshot {
  items: QueueItem[];
  running: boolean;
  paused: boolean;
  activeItemId: string | null;
  published: number;
  pending: number;
  failed: number;
  canceled: number;
  total: number;
}

export interface SplitBoundary {
  part: number;
  start: number;
  end: number;
  duration: number;
}

export interface SplitPlan {
  totalSeconds: number;
  rangeStart: number;
  rangeEnd: number;
  rangeSeconds: number;
  clipSeconds: number;
  clipCount: number;
  lastClipSeconds: number;
  estimatedTotalBytes: number;
  estimatedClipBytes: number;
  boundaries: SplitBoundary[];
}

export interface DownloadFormat {
  id: string;
  label: string;
  height: number;
  ext: string;
  fps: number;
  filesize: number;
  note: string;
}

export interface DownloadMetadata {
  url: string;
  title: string;
  platform: string;
  uploader: string | null;
  duration: number;
  thumbnail: string | null;
  formats: DownloadFormat[];
  isLive: boolean;
}

export interface DownloadRequest {
  url: string;
  quality: string;
  title?: string;
  platform?: string;
  duration?: number;
}

export interface ToolStatus {
  ffmpeg: boolean;
  ffprobe: boolean;
  ytdlp: boolean;
  ffmpegPath: string | null;
  ytdlpPath: string | null;
}

export interface AppInfo {
  version: string;
  name: string;
  author: string;
  storageDir: string;
  databasePath: string;
  tools: ToolStatus;
  platform: string;
}

export interface StorageStats {
  workspaceBytes: number;
  databaseBytes: number;
  tempBytes: number;
  sourceBytes: number;
  clipsBytes: number;
  projectCount: number;
  freeBytes: number;
  totalBytes: number;
}

export interface Settings {
  storageDir: string;
  defaultClipSeconds: number;
  defaultQuality: string;
  theme: "dark" | "light" | "system";
  previewAutoplay: boolean;
  previewMuted: boolean;
  preferPreciseCuts: boolean;
  keepSourceAfterSplit: boolean;
  youtubeClientId: string;
  youtubeCategoryId: string;
  facebookAppId: string;
  facebookPageId: string;
  tiktokClientKey: string;
  tiktokPrivacyLevel: "SELF_ONLY" | "PUBLIC_TO_EVERYONE" | "MUTUAL_FOLLOW_FRIENDS";
  defaultPrivacy: "private" | "public";
  browserKeepSession: boolean;
  webProtectionEnabled: boolean;
  webProtectionLevel: "standard" | "intensive";
  browserDownloadDir: string;
  browserAutoOpenDownloads: boolean;
}

export interface PlatformStatus {
  platform: string;
  displayName: string;
  hasClientId: boolean;
  hasClientSecret: boolean;
  configured: boolean;
  scopes: string[];
  redirectUri: string;
  docsUrl: string;
  requirements: string;
  connectedAccounts: number;
}

export interface ManualAuth {
  platform: string;
  authUrl: string;
  redirectUri: string;
  state: string;
}

export interface CleanupReport {
  projectId: string;
  removedFiles: number;
  freedBytes: number;
  removedDirectories: number;
  warnings: string[];
  verified: boolean;
}

export interface ActivityEntry {
  id: number;
  level: string;
  message: string;
  detail: string | null;
  createdAt: string;
}

export interface EnqueueClip {
  clipId: string;
  title: string;
  description: string;
  hashtags: string[];
}

export interface EnqueueRequest {
  projectId: string;
  platform: string;
  accountId: string;
  clips: EnqueueClip[];
}

export interface TaskProgress {
  projectId: string;
  percent: number;
  stage: string;
  message: string;
  currentPart?: number;
  totalParts?: number;
}

/* ------------------------------------------------------------ Navegador */

export interface BrowserProfileInfo {
  id: string;
  label: string;
  sizeBytes: number;
  hasData: boolean;
}

export interface BrowserTabInfo {
  id: string;
  profile: string;
  title: string;
  url: string;
  loading: boolean;
  homeUrl: string;
}

export interface BrowserDownloadInfo {
  id: string;
  profile: string;
  url: string;
  path: string;
  status: string;
  createdAt: string;
}

export interface FilterListInfo {
  id: string;
  name: string;
  url: string;
  license: string;
  bytes: number;
  updatedAt: string | null;
  intensiveOnly: boolean;
  installed: boolean;
}

export interface SiteRule {
  profile: string;
  host: string;
  enabled: boolean;
}

export interface BrowserProtectionStatus {
  enabled: boolean;
  level: "standard" | "intensive";
  engineReady: boolean;
  rules: number;
  lists: FilterListInfo[];
  sites: SiteRule[];
  blockedTotal: number;
  blockedByProfile: Record<string, number>;
  updateRunning: boolean;
  lastUpdate: string | null;
  lastError: string | null;
}

export interface BrowserSnapshot {
  tabs: BrowserTabInfo[];
  activeId: string | null;
  visible: boolean;
  profiles: BrowserProfileInfo[];
  protection: BrowserProtectionStatus;
  downloads: BrowserDownloadInfo[];
  keepSession: boolean;
}

/* ------------------------------------------------- Publicación asistida */

export type AssistedStatus =
  | "pending"
  | "preparing"
  | "waiting"
  | "published"
  | "error"
  | "canceled";

export interface AssistedItem {
  id: string;
  projectId: string;
  clipId: string;
  projectName: string;
  partIndex: number;
  platform: string;
  title: string;
  description: string;
  hashtags: string[];
  status: AssistedStatus;
  error: string | null;
  remoteUrl: string | null;
  sortIndex: number;
  createdAt: string;
  updatedAt: string;
  clipPath: string;
}

export interface AssistedSnapshot {
  items: AssistedItem[];
  activeId: string | null;
  paused: boolean;
  published: number;
  pending: number;
  failed: number;
  canceled: number;
}

export interface AssistedClipInput {
  clipId: string;
  title: string;
  description: string;
  hashtags: string[];
}

export interface AssistedRequest {
  projectId: string;
  platform: string;
  clips: AssistedClipInput[];
}

/* ------------------------------------------------------------------- API */

export const api = {
  appInfo: () => invoke<AppInfo>("app_info"),
  toolStatus: () => invoke<ToolStatus>("tool_status"),
  toolsRefresh: () => invoke<ToolStatus>("tools_refresh"),
  activityRecent: (limit = 40) => invoke<ActivityEntry[]>("activity_recent", { limit }),

  settingsGet: () => invoke<Settings>("settings_get"),
  settingsSave: (settings: Settings) => invoke<Settings>("settings_save", { settings }),

  storageStats: () => invoke<StorageStats>("storage_stats"),
  tempCleanup: () => invoke<number>("temp_cleanup"),
  revealPath: (path: string) => invoke<void>("reveal_path", { path }),
  openExternal: (url: string) => invoke<void>("open_external_url", { url }),

  projectsList: () => invoke<Project[]>("projects_list"),
  projectGet: (projectId: string) => invoke<Project>("project_get", { projectId }),
  projectUpdate: (
    projectId: string,
    patch: { name?: string; hashtags?: string[]; defaultClipSeconds?: number },
  ) => invoke<Project>("project_update", { projectId, ...patch }),
  projectImport: (path: string) => invoke<Project>("project_import", { path }),
  projectDeleteAll: (projectId: string) =>
    invoke<CleanupReport>("project_delete_all", { projectId }),

  clipsList: (projectId: string) => invoke<Clip[]>("clips_list", { projectId }),
  clipDelete: (clipId: string) => invoke<void>("clip_delete", { clipId }),

  splitPlan: (projectId: string, clipSeconds: number, rangeStart: number, rangeEnd: number) =>
    invoke<SplitPlan>("split_plan", { projectId, clipSeconds, rangeStart, rangeEnd }),
  splitStart: (
    projectId: string,
    clipSeconds: number,
    mode: "copy" | "precise",
    rangeStart: number,
    rangeEnd: number,
  ) => invoke<Clip[]>("split_start", { projectId, clipSeconds, mode, rangeStart, rangeEnd }),
  splitCancel: (projectId: string) => invoke<void>("split_cancel", { projectId }),

  downloadMetadata: (url: string) => invoke<DownloadMetadata>("download_metadata", { url }),
  downloadStart: (request: DownloadRequest) =>
    invoke<Project>("download_start", { request }),
  downloadCancel: (projectId: string) => invoke<void>("download_cancel", { projectId }),

  platformStatuses: () => invoke<PlatformStatus[]>("platform_statuses"),
  accountsList: () => invoke<Account[]>("accounts_list"),
  oauthConnect: (platform: string) => invoke<Account>("oauth_connect", { platform }),
  oauthStartManual: (platform: string) =>
    invoke<ManualAuth>("oauth_start_manual", { platform }),
  oauthCompleteManual: (platform: string, redirectUrl: string) =>
    invoke<Account>("oauth_complete_manual", { platform, redirectUrl }),
  accountDisconnect: (accountId: string) =>
    invoke<void>("account_disconnect", { accountId }),
  platformSaveCredentials: (platform: string, clientId: string, clientSecret: string) =>
    invoke<void>("platform_save_credentials", { platform, clientId, clientSecret }),
  platformClearCredentials: (platform: string) =>
    invoke<void>("platform_clear_credentials", { platform }),

  queueSnapshot: () => invoke<QueueSnapshot>("queue_snapshot"),
  queueEnqueue: (request: EnqueueRequest) =>
    invoke<number>("queue_enqueue", { request }),
  queueStart: () => invoke<void>("queue_start"),
  queuePause: () => invoke<void>("queue_pause"),
  queueCancelRemaining: () => invoke<number>("queue_cancel_remaining"),
  queueRetry: (itemId: string) => invoke<void>("queue_retry", { itemId }),
  queueCancelItem: (itemId: string) => invoke<void>("queue_cancel_item", { itemId }),

  browserOpen: () => invoke<BrowserSnapshot>("browser_open"),
  browserSnapshot: () => invoke<BrowserSnapshot>("browser_snapshot"),
  browserHide: () => invoke<void>("browser_hide"),
  browserSetBounds: (x: number, y: number, width: number, height: number) =>
    invoke<void>("browser_set_bounds", { x, y, width, height }),
  browserActivate: (tabId: string) => invoke<void>("browser_activate", { tabId }),
  browserNewTab: (profile: string) => invoke<BrowserTabInfo>("browser_new_tab", { profile }),
  browserCloseTab: (tabId: string) => invoke<void>("browser_close_tab", { tabId }),
  browserNavigate: (input: string, tabId?: string | null) =>
    invoke<void>("browser_navigate", { input, tabId: tabId ?? null }),
  browserAction: (action: "back" | "forward" | "reload" | "stop", tabId?: string | null) =>
    invoke<void>("browser_action", { action, tabId: tabId ?? null }),
  browserOpenPlatform: (profile: string, url?: string) =>
    invoke<BrowserTabInfo>("browser_open_platform", { profile, url: url ?? null }),
  browserClearData: (profile: string, mode: "session" | "cache" | "profile") =>
    invoke<void>("browser_clear_data", { profile, mode }),
  browserProtectionStatus: () => invoke<BrowserProtectionStatus>("browser_protection_status"),
  browserProtectionSet: (enabled: boolean) =>
    invoke<void>("browser_protection_set", { enabled }),
  browserProtectionLevel: (level: "standard" | "intensive") =>
    invoke<void>("browser_protection_level", { level }),
  browserUpdateLists: () => invoke<BrowserProtectionStatus>("browser_update_lists"),
  browserSiteSet: (profile: string, host: string, enabled: boolean) =>
    invoke<void>("browser_site_set", { profile, host, enabled }),
  browserSiteRemove: (profile: string, host: string) =>
    invoke<void>("browser_site_remove", { profile, host }),
  browserOpenExternal: (url: string) => invoke<void>("browser_open_external", { url }),
  browserPurgeSessions: () => invoke<void>("browser_purge_sessions"),
  browserOpenDownloads: () => invoke<string>("browser_open_downloads"),

  assistedSnapshot: () => invoke<AssistedSnapshot>("assisted_snapshot"),
  assistedEnqueue: (request: AssistedRequest) => invoke<number>("assisted_enqueue", { request }),
  assistedStart: () => invoke<AssistedSnapshot>("assisted_start"),
  assistedPause: () => invoke<AssistedSnapshot>("assisted_pause"),
  assistedCancelRemaining: () => invoke<number>("assisted_cancel_remaining"),
  assistedConfirm: (itemId: string, remoteUrl?: string | null) =>
    invoke<AssistedSnapshot>("assisted_confirm", { itemId, remoteUrl: remoteUrl ?? null }),
  assistedFail: (itemId: string, message: string) =>
    invoke<AssistedSnapshot>("assisted_fail", { itemId, message }),
  assistedRetry: (itemId: string) => invoke<AssistedSnapshot>("assisted_retry", { itemId }),
  assistedReload: () => invoke<AssistedSnapshot>("assisted_reload"),
};

export function errorMessage(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  if (error && typeof error === "object") {
    const maybe = error as { message?: unknown };
    if (typeof maybe.message === "string") return maybe.message;
  }
  return "Ocurrió un error inesperado.";
}
