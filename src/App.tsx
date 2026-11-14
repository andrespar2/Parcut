import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { QueueSnapshot } from "./lib/api";
import { applyTheme, useAppStore } from "./store/app";
import { Sidebar } from "./components/Sidebar";
import { TopBar } from "./components/TopBar";
import { Toasts } from "./components/Toasts";
import { HomePage } from "./pages/HomePage";
import { NewProjectPage } from "./pages/NewProjectPage";
import { ProjectPage } from "./pages/ProjectPage";
import { ClipsPage } from "./pages/ClipsPage";
import { PublishPage } from "./pages/PublishPage";
import { AccountsPage } from "./pages/AccountsPage";
import { SettingsPage } from "./pages/SettingsPage";

function CurrentPage() {
  const view = useAppStore((state) => state.view);
  switch (view) {
    case "new":
      return <NewProjectPage />;
    case "project":
      return <ProjectPage />;
    case "clips":
      return <ClipsPage />;
    case "publish":
      return <PublishPage />;
    case "accounts":
      return <AccountsPage />;
    case "settings":
      return <SettingsPage />;
    default:
      return <HomePage />;
  }
}

export default function App() {
  const loadSettings = useAppStore((state) => state.loadSettings);
  const refreshAll = useAppStore((state) => state.refreshAll);
  const loadProjects = useAppStore((state) => state.loadProjects);
  const loadAccounts = useAppStore((state) => state.loadAccounts);
  const loadQueue = useAppStore((state) => state.loadQueue);
  const setQueue = useAppStore((state) => state.setQueue);
  const clearProjectTasks = useAppStore((state) => state.clearProjectTasks);
  const settings = useAppStore((state) => state.settings);

  useEffect(() => {
    void (async () => {
      await loadSettings();
      await refreshAll();
    })();
  }, [loadSettings, refreshAll]);

  useEffect(() => {
    if (settings?.theme !== "system") return;
    const media = window.matchMedia("(prefers-color-scheme: light)");
    const handler = () => applyTheme("system");
    media.addEventListener("change", handler);
    return () => media.removeEventListener("change", handler);
  }, [settings?.theme]);

  useEffect(() => {
    const unlisteners = [
      listen<{ level: "success" | "error" | "info"; message: string }>("toast", (event) => {
        useAppStore.getState().toast(event.payload.level ?? "info", event.payload.message);
      }),
      listen("project:updated", () => void loadProjects()),
      listen("accounts:changed", () => void loadAccounts()),
      listen<QueueSnapshot>("queue:changed", (event) => setQueue(event.payload)),
      listen("queue:finished", () => void loadQueue()),
      listen<{ projectId?: string }>("download:done", (event) => {
        if (event.payload?.projectId) clearProjectTasks(event.payload.projectId);
        void loadProjects();
      }),
      listen<{ projectId?: string }>("split:done", (event) => {
        if (event.payload?.projectId) clearProjectTasks(event.payload.projectId);
        void loadProjects();
      }),
      listen<{ projectId?: string; message: string }>("download:error", (event) => {
        if (event.payload?.projectId) clearProjectTasks(event.payload.projectId);
        void loadProjects();
      }),
      listen<{ projectId?: string; message: string }>("split:error", (event) => {
        if (event.payload?.projectId) clearProjectTasks(event.payload.projectId);
        void loadProjects();
      }),
      listen<{ projectId?: string }>("project:media-ready", (event) => {
        if (event.payload?.projectId) clearProjectTasks(event.payload.projectId);
        void loadProjects();
      }),
    ];
    return () => {
      void Promise.all(unlisteners).then((fns) => fns.forEach((fn) => fn()));
    };
  }, [loadProjects, loadAccounts, loadQueue, setQueue, clearProjectTasks]);

  return (
    <div className="app">
      <Sidebar />
      <main className="main">
        <TopBar />
        <div className="content">
          <CurrentPage />
        </div>
      </main>
      <Toasts />
    </div>
  );
}
