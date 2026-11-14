import { create } from "zustand";
import {
  Account,
  api,
  AppInfo,
  errorMessage,
  PlatformStatus,
  Project,
  QueueSnapshot,
  Settings,
} from "../lib/api";

export type View =
  | "home"
  | "new"
  | "browser"
  | "project"
  | "clips"
  | "publish"
  | "accounts"
  | "settings";

export type ToastLevel = "success" | "error" | "info";

export interface Toast {
  id: number;
  level: ToastLevel;
  message: string;
}

export interface TaskInfo {
  key: string;
  kind: "download" | "split" | "import";
  projectId: string;
  percent: number;
  message: string;
}

interface AppStore {
  view: View;
  projectId: string | null;
  publishProjectId: string | null;
  publishClipIds: string[] | null;
  browserStartAssisted: boolean;
  settings: Settings | null;
  appInfo: AppInfo | null;
  accounts: Account[];
  platforms: PlatformStatus[];
  queue: QueueSnapshot | null;
  projects: Project[];
  tasks: Record<string, TaskInfo>;
  toasts: Toast[];
  navigate: (view: View, options?: { projectId?: string; publishProjectId?: string; clipIds?: string[] }) => void;
  setBrowserStartAssisted: (value: boolean) => void;
  loadSettings: () => Promise<void>;
  saveSettings: (settings: Settings) => Promise<void>;
  loadProjects: () => Promise<void>;
  loadAccounts: () => Promise<void>;
  loadQueue: () => Promise<void>;
  setQueue: (queue: QueueSnapshot) => void;
  loadAppInfo: () => Promise<void>;
  refreshAll: () => Promise<void>;
  toast: (level: ToastLevel, message: string) => void;
  dismissToast: (id: number) => void;
  setTask: (task: TaskInfo) => void;
  clearTask: (key: string) => void;
  clearProjectTasks: (projectId: string) => void;
}

export function applyTheme(theme: string | undefined) {
  const resolved =
    theme === "system"
      ? window.matchMedia("(prefers-color-scheme: light)").matches
        ? "light"
        : "dark"
      : theme === "light"
        ? "light"
        : "dark";
  document.documentElement.dataset.theme = resolved;
}

let toastSeq = 1;

export const useAppStore = create<AppStore>((set, get) => ({
  view: "home",
  projectId: null,
  publishProjectId: null,
  publishClipIds: null,
  browserStartAssisted: false,
  settings: null,
  appInfo: null,
  accounts: [],
  platforms: [],
  queue: null,
  projects: [],
  tasks: {},
  toasts: [],

  navigate: (view, options) => {
    set({
      view,
      projectId: options?.projectId ?? (view === "project" ? get().projectId : null),
      publishProjectId:
        options?.publishProjectId ??
        (view === "publish" ? get().publishProjectId : null),
      publishClipIds:
        view === "publish" ? options?.clipIds ?? get().publishClipIds : null,
    });
  },

  setBrowserStartAssisted: (value) => set({ browserStartAssisted: value }),

  loadSettings: async () => {
    try {
      const settings = await api.settingsGet();
      set({ settings });
      applyTheme(settings.theme);
    } catch (error) {
      get().toast("error", errorMessage(error));
    }
  },

  saveSettings: async (settings) => {
    const saved = await api.settingsSave(settings);
    set({ settings: saved });
    applyTheme(saved.theme);
  },

  loadProjects: async () => {
    try {
      const projects = await api.projectsList();
      set({ projects });
    } catch (error) {
      get().toast("error", errorMessage(error));
    }
  },

  loadAccounts: async () => {
    try {
      const [accounts, platforms] = await Promise.all([
        api.accountsList(),
        api.platformStatuses(),
      ]);
      set({ accounts, platforms });
    } catch (error) {
      get().toast("error", errorMessage(error));
    }
  },

  loadQueue: async () => {
    try {
      const queue = await api.queueSnapshot();
      set({ queue });
    } catch (error) {
      get().toast("error", errorMessage(error));
    }
  },

  setQueue: (queue) => set({ queue }),

  loadAppInfo: async () => {    try {
      const appInfo = await api.appInfo();
      set({ appInfo });
    } catch (error) {
      get().toast("error", errorMessage(error));
    }
  },

  refreshAll: async () => {
    await Promise.all([
      get().loadProjects(),
      get().loadAccounts(),
      get().loadQueue(),
      get().loadAppInfo(),
    ]);
  },

  toast: (level, message) => {
    const id = toastSeq++;
    set((state) => ({ toasts: [...state.toasts, { id, level, message }].slice(-4) }));
    window.setTimeout(() => get().dismissToast(id), level === "error" ? 8000 : 4500);
  },

  dismissToast: (id) => {
    set((state) => ({ toasts: state.toasts.filter((toast) => toast.id !== id) }));
  },

  setTask: (task) => {
    set((state) => ({ tasks: { ...state.tasks, [task.key]: task } }));
  },

  clearTask: (key) => {
    set((state) => {
      const tasks = { ...state.tasks };
      delete tasks[key];
      return { tasks };
    });
  },

  clearProjectTasks: (projectId) => {
    set((state) => {
      const tasks = Object.fromEntries(
        Object.entries(state.tasks).filter(([, task]) => task.projectId !== projectId),
      );
      return { tasks };
    });
  },
}));

/** Tareas activas visibles en la barra superior. */
export function activeTaskCount(state: {
  tasks: Record<string, TaskInfo>;
  queue: QueueSnapshot | null;
}): number {
  const local = Object.keys(state.tasks).length;
  const queued = state.queue?.running ? 1 : 0;
  return local + queued;
}
