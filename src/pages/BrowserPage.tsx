import { useCallback, useEffect, useRef, useState } from "react";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import {
  AlertCircle,
  AlertTriangle,
  ArrowLeft,
  ArrowRight,
  Check,
  ClipboardCopy,
  Download,
  ExternalLink,
  FolderOpen,
  Globe,
  Home,
  Info,
  Loader2,
  Lock,
  Pause,
  Play,
  Plus,
  RefreshCw,
  RotateCw,
  Send,
  Settings2,
  Shield,
  ShieldOff,
  Trash2,
  UploadCloud,
  X,
} from "lucide-react";
import {
  api,
  AssistedSnapshot,
  BrowserProfileInfo,
  BrowserProtectionStatus,
  BrowserSnapshot,
  BrowserTabInfo,
  errorMessage,
} from "../lib/api";
import { formatBytes, PLATFORM_LABELS } from "../lib/format";
import { useAppStore } from "../store/app";
import { Modal, Pill, Segmented, SwitchRow } from "../components/ui";
import { PlatformLogo } from "../components/PlatformLogo";

const HOME_URLS: Record<string, string> = {
  youtube: "https://www.youtube.com/",
  facebook: "https://www.facebook.com/",
  tiktok: "https://www.tiktok.com/",
};

const STUDIO_URLS: Record<string, string> = {
  youtube: "https://studio.youtube.com/",
  facebook: "https://www.facebook.com/",
  tiktok: "https://www.tiktok.com/tiktokstudio",
};

const UPLOAD_URLS: Record<string, string> = {
  youtube: "https://www.youtube.com/upload",
  facebook: "https://www.facebook.com/",
  tiktok: "https://www.tiktok.com/tiktokstudio/upload",
};

const PROFILE_ORDER = ["youtube", "facebook", "tiktok"] as const;

// Oculta el WebView cuando se abandona la página, con margen para los remontajes
// de React en desarrollo (StrictMode) que no deben ocultar el navegador.
let browserHideTimer: number | null = null;

function scheduleBrowserHide() {
  if (browserHideTimer) window.clearTimeout(browserHideTimer);
  browserHideTimer = window.setTimeout(() => {
    browserHideTimer = null;
    void api.browserHide();
  }, 250);
}

function cancelBrowserHide() {
  if (browserHideTimer) {
    window.clearTimeout(browserHideTimer);
    browserHideTimer = null;
  }
}

type Panel = null | "protection" | "settings";

interface Confirmation {
  title: string;
  message: string;
  confirmLabel: string;
  danger?: boolean;
  run: () => Promise<void>;
}

function profileLabel(profile: string): string {
  return PLATFORM_LABELS[profile] ?? profile;
}

function hostOf(url: string): string {
  try {
    return new URL(url).hostname.toLowerCase();
  } catch {
    return "";
  }
}

function matchingException(
  protection: BrowserProtectionStatus | undefined,
  profile: string,
  host: string,
) {
  if (!protection || !host) return undefined;
  const parts = host.split(".");
  const suffixes = parts.map((_, index) => parts.slice(index).join("."));
  return protection.sites.find(
    (site) => site.profile === profile && suffixes.includes(site.host),
  );
}

export function BrowserPage() {
  const settings = useAppStore((state) => state.settings);
  const saveSettings = useAppStore((state) => state.saveSettings);
  const toast = useAppStore((state) => state.toast);
  const browserStartAssisted = useAppStore((state) => state.browserStartAssisted);
  const setBrowserStartAssisted = useAppStore((state) => state.setBrowserStartAssisted);

  const [snapshot, setSnapshot] = useState<BrowserSnapshot | null>(null);
  const [assisted, setAssisted] = useState<AssistedSnapshot | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [address, setAddress] = useState("");
  const [addressFocused, setAddressFocused] = useState(false);
  const [panel, setPanel] = useState<Panel>(null);
  const [tabMenuOpen, setTabMenuOpen] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [confirm, setConfirm] = useState<Confirmation | null>(null);
  const [failOpen, setFailOpen] = useState(false);
  const [failMessage, setFailMessage] = useState("");

  const surfaceRef = useRef<HTMLDivElement | null>(null);
  const noticeTimer = useRef<number | null>(null);
  const panelRef = useRef<Panel>(null);

  const refresh = useCallback(async () => {
    try {
      const next = await api.browserSnapshot();
      setSnapshot(next);
      return next;
    } catch (err) {
      setError(errorMessage(err));
      return null;
    }
  }, []);

  const refreshAssisted = useCallback(async () => {
    try {
      setAssisted(await api.assistedSnapshot());
    } catch {
      /* el estado del navegador sigue siendo independiente */
    }
  }, []);

  const showNotice = useCallback((message: string) => {
    setNotice(message);
    if (noticeTimer.current) window.clearTimeout(noticeTimer.current);
    noticeTimer.current = window.setTimeout(() => setNotice(null), 6500);
  }, []);

  const syncBounds = useCallback(() => {
    const element = surfaceRef.current;
    if (!element) return;
    const rect = element.getBoundingClientRect();
    if (rect.width < 2 || rect.height < 2) return;
    void api
      .browserSetBounds(rect.left, rect.top, rect.width, rect.height)
      .catch(() => undefined);
  }, []);

  useEffect(() => {
    let disposed = false;
    cancelBrowserHide();
    void (async () => {
      try {
        setLoading(true);
        const initial = await api.browserOpen();
        if (disposed) return;
        setSnapshot(initial);
        setError(null);
        void refreshAssisted();
      } catch (err) {
        if (!disposed) setError(errorMessage(err));
      } finally {
        if (!disposed) setLoading(false);
      }
    })();
    return () => {
      disposed = true;
      scheduleBrowserHide();
    };
  }, []);

  useEffect(() => {
    if (!browserStartAssisted || loading || !snapshot) return;
    setBrowserStartAssisted(false);
    void (async () => {
      try {
        const next = await api.assistedStart();
        setAssisted(next);
        showNotice("Publicación asistida iniciada. La página de subida está abierta.");
        await refresh();
      } catch (err) {
        setError(errorMessage(err));
      }
    })();
  }, [browserStartAssisted, loading, snapshot, setBrowserStartAssisted, refresh, showNotice]);

  useEffect(() => {
    const unlisteners: Promise<UnlistenFn>[] = [
      listen("browser:changed", () => void refresh()),
      listen<{ message?: string }>("browser:notice", (event) => {
        if (event.payload?.message) showNotice(event.payload.message);
      }),
      listen("browser:protection", () => void refresh()),
      listen("browser:download", () => void refresh()),
      listen("assisted:changed", (event) => {
        setAssisted(event.payload as AssistedSnapshot);
      }),
    ];
    return () => {
      void Promise.all(unlisteners).then((items) => items.forEach((unlisten) => unlisten()));
    };
  }, [refresh, showNotice]);

  useEffect(() => {
    const timer = window.setInterval(() => {
      if (!document.hidden) {
        void refresh();
        void refreshAssisted();
      }
    }, 1600);
    return () => window.clearInterval(timer);
  }, [refresh, refreshAssisted]);

  const activeTab = snapshot?.tabs.find((tab) => tab.id === snapshot.activeId) ?? null;

  useEffect(() => {
    if (!addressFocused) setAddress(activeTab?.url ?? "");
  }, [activeTab?.url, activeTab?.id, addressFocused]);

  useEffect(() => {
    if (loading || panel) return;
    const element = surfaceRef.current;
    if (!element) return;
    const observer = new ResizeObserver(() => syncBounds());
    observer.observe(element);
    window.addEventListener("resize", syncBounds);
    const timer = window.setTimeout(syncBounds, 140);
    return () => {
      observer.disconnect();
      window.removeEventListener("resize", syncBounds);
      window.clearTimeout(timer);
    };
  }, [panel, loading, activeTab?.id, syncBounds]);

  useEffect(() => {
    const timer = window.setTimeout(syncBounds, 220);
    return () => window.clearTimeout(timer);
  }, [snapshot?.activeId, syncBounds]);

  useEffect(() => {
    panelRef.current = panel;
  }, [panel]);

  const runAction = async (key: string, action: () => Promise<void>) => {
    setBusy(key);
    setError(null);
    try {
      await action();
    } catch (err) {
      setError(errorMessage(err));
      toast("error", errorMessage(err));
    } finally {
      setBusy(null);
    }
  };

  const navigate = async (input: string) => {
    if (!input.trim()) return;
    await runAction("navigate", async () => {
      await api.browserNavigate(input, activeTab?.id ?? null);
      await refresh();
    });
  };

  const action = async (kind: "back" | "forward" | "reload" | "stop") => {
    await runAction(kind, async () => {
      await api.browserAction(kind, activeTab?.id ?? null);
      await refresh();
    });
  };

  const nudgeRepaint = () => {
    const element = document.querySelector(".browser-page") as HTMLElement | null;
    if (!element) return;
    element.style.transform = "translateZ(0)";
    window.requestAnimationFrame(() => {
      element.style.transform = "";
    });
  };

  const openPanel = async (next: Exclude<Panel, null>) => {
    setPanel(next);
    setTabMenuOpen(false);
    await api.browserHide();
    window.setTimeout(nudgeRepaint, 40);
  };

  const closePanel = async () => {
    setPanel(null);
    const next = await api.browserOpen();
    setSnapshot(next);
    window.setTimeout(syncBounds, 160);
    window.setTimeout(nudgeRepaint, 60);
  };

  const activateTab = async (tab: BrowserTabInfo) => {
    if (panel) {
      await closePanel();
    }
    await runAction("tab", async () => {
      await api.browserActivate(tab.id);
      await refresh();
      window.setTimeout(syncBounds, 120);
    });
  };

  const openPlatform = async (profile: string, url: string) => {
    if (panel) {
      await closePanel();
    }
    await runAction("platform", async () => {
      await api.browserOpenPlatform(profile, url);
      const next = await refresh();
      const tab = next?.tabs.find((item) => item.profile === profile);
      if (tab) setAddress(tab.url);
      window.setTimeout(syncBounds, 160);
    });
  };

  const newTab = async (profile: string) => {
    setTabMenuOpen(false);
    if (panel) await closePanel();
    await runAction("new-tab", async () => {
      await api.browserNewTab(profile);
      await refresh();
      window.setTimeout(syncBounds, 160);
    });
  };

  const closeTab = async (tabId: string) => {
    await runAction("close-tab", async () => {
      await api.browserCloseTab(tabId);
      await refresh();
    });
  };

  const toggleProtection = async (enabled: boolean) => {
    await runAction("protection", async () => {
      await api.browserProtectionSet(enabled);
      await refresh();
    });
  };

  const changeLevel = async (level: "standard" | "intensive") => {
    await runAction("level", async () => {
      await api.browserProtectionLevel(level);
      await refresh();
    });
  };

  const toggleSiteProtection = async () => {
    if (!activeTab) return;
    const host = hostOf(activeTab.url);
    if (!host) return;
    const exception = matchingException(snapshot?.protection, activeTab.profile, host);
    await runAction("site", async () => {
      if (exception && !exception.enabled) {
        await api.browserSiteRemove(activeTab.profile, exception.host);
      } else {
        await api.browserSiteSet(activeTab.profile, host, false);
      }
      await refresh();
    });
  };

  const updateLists = async () => {
    await runAction("lists", async () => {
      await api.browserUpdateLists();
      await refresh();
    });
  };

  const clearProfile = (profile: string, mode: "session" | "cache" | "profile") => {
    const label = profileLabel(profile);
    const titles = {
      session: `Borrar sesión de ${label}`,
      cache: `Limpiar caché de ${label}`,
      profile: `Eliminar perfil de ${label}`,
    };
    const messages = {
      session:
        "Se cerrarán las sesiones iniciadas en esta plataforma. Las demás plataformas no se modifican.",
      cache:
        "Se eliminarán archivos temporales y caché. La sesión iniciada se conserva.",
      profile:
        "Se eliminará todo el perfil de navegación: cookies, almacenamiento local, caché e historial. Las demás plataformas no se modifican.",
    };
    setConfirm({
      title: titles[mode],
      message: messages[mode],
      confirmLabel: mode === "profile" ? "Eliminar perfil" : "Borrar datos",
      danger: true,
      run: async () => {
        setBusy(`clear-${profile}`);
        try {
          await api.browserClearData(profile, mode);
          const next = await refresh();
          if (panelRef.current) await api.browserHide();
          if (mode === "profile" && next) {
            showNotice(`Perfil de ${label} eliminado.`);
          } else {
            showNotice(`Datos de ${label} actualizados.`);
          }
        } finally {
          setBusy(null);
        }
      },
    });
  };

  const setKeepSession = async (value: boolean) => {
    if (!settings) return;
    if (value) {
      await saveSettings({ ...settings, browserKeepSession: true });
      await refresh();
      return;
    }
    setConfirm({
      title: "Desactivar «Mantener sesión iniciada»",
      message:
        "Las sesiones actuales se borrarán y no se conservarán al cerrar ParCut. Podrás volver a iniciar sesión cuando quieras.",
      confirmLabel: "Desactivar y borrar sesiones",
      danger: true,
      run: async () => {
        await saveSettings({ ...settings, browserKeepSession: false });
        await api.browserPurgeSessions();
        await refresh();
        showNotice("Las sesiones guardadas se eliminaron.");
      },
    });
  };

  const chooseDownloadDir = async () => {
    if (!settings) return;
    const selected = await open({
      directory: true,
      multiple: false,
      title: "Carpeta de descargas del navegador",
      defaultPath: settings.browserDownloadDir || undefined,
    });
    if (typeof selected === "string") {
      await saveSettings({ ...settings, browserDownloadDir: selected });
      await refresh();
    }
  };

  const protection = snapshot?.protection;
  const currentHost = activeTab ? hostOf(activeTab.url) : "";
  const currentException = activeTab
    ? matchingException(protection, activeTab.profile, currentHost)
    : undefined;
  const siteDisabled = currentException ? !currentException.enabled : false;
  const secure = activeTab?.url.startsWith("https://") ?? false;
  const activeProfile = activeTab?.profile ?? "youtube";
  const blocked = protection?.blockedByProfile[activeProfile] ?? 0;

  const orderedAssisted = [...(assisted?.items ?? [])].sort(
    (a, b) => a.partIndex - b.partIndex || a.createdAt.localeCompare(b.createdAt),
  );
  const assistedActive =
    orderedAssisted.find((item) => item.id === assisted?.activeId) ?? null;
  const assistedFailed = orderedAssisted.find((item) => item.status === "error") ?? null;

  const copyAssisted = async () => {
    if (!assistedActive) return;
    const parts = [assistedActive.title];
    if (assistedActive.description.trim()) parts.push(assistedActive.description.trim());
    const tagText = assistedActive.hashtags.map((tag) => `#${tag}`).join(" ");
    const lower = assistedActive.description.toLowerCase();
    const missing = assistedActive.hashtags.filter(
      (tag) => !lower.includes(`#${tag.toLowerCase()}`),
    );
    if (missing.length > 0) {
      parts.push(missing.map((tag) => `#${tag}`).join(" "));
    } else if (tagText && !lower.includes("#")) {
      parts.push(tagText);
    }
    await navigator.clipboard.writeText(parts.join("\n\n"));
    showNotice("Título, descripción y hashtags copiados al portapapeles.");
  };

  const confirmAssisted = async () => {
    if (!assistedActive) return;
    setBusy("assisted-confirm");
    try {
      const next = await api.assistedConfirm(assistedActive.id);
      setAssisted(next);
      const following = next.items.find((item) => item.id === next.activeId);
      showNotice(
        following
          ? `Parte ${assistedActive.partIndex} confirmada. Siguiente: parte ${following.partIndex}.`
          : "Todas las partes de la cola están confirmadas.",
      );
      await refresh();
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setBusy(null);
    }
  };

  const retryAssisted = async () => {
    if (!assistedFailed) return;
    setBusy("assisted-retry");
    try {
      const next = await api.assistedRetry(assistedFailed.id);
      setAssisted(next);
      showNotice(`Reintentando la parte ${assistedFailed.partIndex}.`);
      await refresh();
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setBusy(null);
    }
  };

  const pauseAssisted = async () => {
    const next = await api.assistedPause();
    setAssisted(next);
    showNotice("Cola asistida en pausa. La publicación actual sigue esperando tu confirmación.");
  };

  const startAssisted = async () => {
    setBusy("assisted-start");
    try {
      const next = await api.assistedStart();
      setAssisted(next);
      await refresh();
      showNotice("Página de subida abierta. Confirma la parte cuando la plataforma termine.");
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setBusy(null);
    }
  };

  const cancelAssisted = () => {
    setConfirm({
      title: "Cancelar publicaciones restantes",
      message:
        "Se cancelarán las partes pendientes de la cola asistida. Las publicaciones ya confirmadas no se modifican.",
      confirmLabel: "Cancelar restantes",
      danger: true,
      run: async () => {
        await api.assistedCancelRemaining();
        await refreshAssisted();
        showNotice("Publicaciones restantes canceladas.");
      },
    });
  };

  const submitAssistedFail = async () => {
    if (!assistedActive) return;
    setBusy("assisted-fail");
    try {
      const next = await api.assistedFail(
        assistedActive.id,
        failMessage.trim() || "La publicación no se completó en la plataforma.",
      );
      setAssisted(next);
      setFailOpen(false);
      setFailMessage("");
      showNotice("Parte marcada con error. La cola queda en pausa.");
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setBusy(null);
    }
  };

  return (
    <div className="browser-page">
      <div className="browser-chrome">
        <div className="browser-tabs">
          <div className="browser-tab-scroll">
            {snapshot?.tabs.map((tab) => (
              <div
                key={tab.id}
                className={`browser-tab${tab.id === snapshot.activeId ? " active" : ""}`}
                onClick={() => void activateTab(tab)}
                role="tab"
                tabIndex={0}
                onKeyDown={(event) => {
                  if (event.key === "Enter") void activateTab(tab);
                }}
              >
                <span className={`tab-platform ${tab.profile}`}>
                  <PlatformLogo platform={tab.profile} size={13} />
                </span>
                <span className="tab-title">{tab.title || profileLabel(tab.profile)}</span>
                {tab.loading ? <span className="tab-loading" /> : null}
                <button
                  className="tab-close"
                  onClick={(event) => {
                    event.stopPropagation();
                    void closeTab(tab.id);
                  }}
                  aria-label="Cerrar pestaña"
                >
                  <X size={11} />
                </button>
              </div>
            ))}
          </div>
          <button
            className="icon-btn browser-new-tab"
            onClick={() => setTabMenuOpen((value) => !value)}
            title="Nueva pestaña"
          >
            <Plus size={16} />
          </button>
        </div>

        {tabMenuOpen ? (
          <div className="browser-tab-menu">
            <span className="small faint">Nueva pestaña en:</span>
            {PROFILE_ORDER.map((profile) => (
              <button key={profile} className="btn btn-sm" onClick={() => void newTab(profile)}>
                <PlatformLogo platform={profile} size={14} />
                {profileLabel(profile)}
              </button>
            ))}
          </div>
        ) : null}

        <div className="browser-toolbar">
          <div className="browser-nav-buttons">
            <button
              className="icon-btn"
              title="Atrás"
              onClick={() => void action("back")}
              disabled={!activeTab}
            >
              <ArrowLeft />
            </button>
            <button
              className="icon-btn"
              title="Adelante"
              onClick={() => void action("forward")}
              disabled={!activeTab}
            >
              <ArrowRight />
            </button>
            <button
              className="icon-btn"
              title="Actualizar"
              onClick={() => void action("reload")}
              disabled={!activeTab}
            >
              {activeTab?.loading ? <Loader2 className="spin" /> : <RotateCw />}
            </button>
            <button
              className="icon-btn"
              title="Inicio"
              onClick={() => void openPlatform(activeProfile, HOME_URLS[activeProfile])}
            >
              <Home />
            </button>
          </div>

          <form
            className="browser-address-row"
            onSubmit={(event) => {
              event.preventDefault();
              (event.currentTarget.querySelector("input") as HTMLInputElement | null)?.blur();
              setAddressFocused(false);
              void navigate(address);
            }}
          >
            <span className={`browser-security ${secure ? "secure" : "insecure"}`}>
              {secure ? <Lock size={13} /> : <AlertTriangle size={13} />}
            </span>
            <input
              className="browser-address"
              value={address}
              onChange={(event) => setAddress(event.target.value)}
              onFocus={() => setAddressFocused(true)}
              onBlur={() => setAddressFocused(false)}
              placeholder="Escribe una dirección web o busca en Google"
              spellCheck={false}
              aria-label="Dirección web"
            />
            {activeTab && blocked > 0 ? (
              <span className="browser-blocked" title="Elementos bloqueados en esta plataforma">
                <Shield size={12} />
                {blocked}
              </span>
            ) : null}
          </form>

          <button
            className={`btn btn-sm ${protection?.enabled ? "protection-on" : "protection-off"}`}
            onClick={() => void openPanel("protection")}
            title="Protección web"
          >
            {protection?.enabled ? <Shield size={14} /> : <ShieldOff size={14} />}
            <span className="protection-label">
              {protection?.enabled ? "Protección" : "Sin protección"}
            </span>
          </button>
          <button
            className="icon-btn"
            title="Abrir en el navegador del sistema"
            disabled={!activeTab}
            onClick={() => activeTab && void api.browserOpenExternal(activeTab.url)}
          >
            <ExternalLink />
          </button>
          <button
            className="icon-btn"
            title="Configuración del navegador"
            onClick={() => void openPanel("settings")}
          >
            <Settings2 />
          </button>
        </div>

        <div className="browser-shortcuts">
          <button
            className="browser-shortcut"
            onClick={() => void openPlatform("youtube", STUDIO_URLS.youtube)}
          >
            <PlatformLogo platform="youtube" size={14} /> YouTube Studio
          </button>
          <button
            className="browser-shortcut"
            onClick={() => void openPlatform("facebook", STUDIO_URLS.facebook)}
          >
            <PlatformLogo platform="facebook" size={14} /> Facebook
          </button>
          <button
            className="browser-shortcut"
            onClick={() => void openPlatform("tiktok", STUDIO_URLS.tiktok)}
          >
            <PlatformLogo platform="tiktok" size={14} /> TikTok Studio
          </button>
          <span className="browser-shortcut-hint">
            <Lock size={11} /> WebView2 con validación de certificados
          </span>
        </div>

        {assisted && assisted.items.length > 0 ? (
          <div className="assisted-bar">
            {assistedActive ? (
              <>
                <div className="assisted-main">
                  <div className="row wrap" style={{ gap: 8 }}>
                    <Pill tone="accent">
                      Parte {assistedActive.partIndex} de {assisted.items.length}
                    </Pill>
                    <span className="assisted-platform">
                      <PlatformLogo platform={assistedActive.platform} size={14} />
                      {profileLabel(assistedActive.platform)}
                    </span>
                    <span className="assisted-title" title={assistedActive.title}>
                      {assistedActive.title}
                    </span>
                  </div>
                  <span className="small faint">
                    Esperando tu confirmación. La siguiente parte no se abrirá hasta que confirmes
                    esta publicación en la plataforma.
                  </span>
                </div>
                <div className="assisted-actions">
                  <button className="btn btn-sm" onClick={() => void copyAssisted()}>
                    <ClipboardCopy /> Copiar metadatos
                  </button>
                  <button
                    className="btn btn-sm"
                    onClick={() =>
                      assistedActive.clipPath && void api.revealPath(assistedActive.clipPath)
                    }
                    disabled={!assistedActive.clipPath}
                  >
                    <FolderOpen /> Archivo
                  </button>
                  <button
                    className="btn btn-sm btn-primary"
                    onClick={() => void confirmAssisted()}
                    disabled={busy !== null}
                  >
                    {busy === "assisted-confirm" ? <Loader2 className="spin" /> : <Check />}
                    Confirmar publicado
                  </button>
                  <button
                    className="icon-btn"
                    title="Marcar problema"
                    onClick={() => setFailOpen(true)}
                  >
                    <AlertCircle />
                  </button>
                  <button
                    className="icon-btn"
                    title={assisted.paused ? "Reanudar cola" : "Pausar cola"}
                    onClick={() =>
                      assisted.paused ? void startAssisted() : void pauseAssisted()
                    }
                  >
                    {assisted.paused ? <Play /> : <Pause />}
                  </button>
                  <button
                    className="icon-btn"
                    title="Abrir página de subida"
                    onClick={() =>
                      assistedActive &&
                      void openPlatform(
                        assistedActive.platform,
                        UPLOAD_URLS[assistedActive.platform] ?? HOME_URLS[assistedActive.platform],
                      )
                    }
                  >
                    <UploadCloud />
                  </button>
                  <button className="icon-btn" title="Cancelar restantes" onClick={cancelAssisted}>
                    <X />
                  </button>
                </div>
              </>
            ) : assistedFailed ? (
              <>
                <div className="assisted-main">
                  <div className="row wrap" style={{ gap: 8 }}>
                    <Pill tone="danger">Parte {assistedFailed.partIndex} con error</Pill>
                    <span className="assisted-title">{assistedFailed.title}</span>
                  </div>
                  <span className="small faint">
                    {assistedFailed.error ?? "La publicación no se completó."}
                  </span>
                </div>
                <div className="assisted-actions">
                  <button
                    className="btn btn-sm btn-primary"
                    onClick={() => void retryAssisted()}
                    disabled={busy !== null}
                  >
                    <Play /> Reintentar
                  </button>
                  <button className="btn btn-sm" onClick={cancelAssisted}>
                    Cancelar restantes
                  </button>
                </div>
              </>
            ) : assisted.pending > 0 ? (
              <>
                <div className="assisted-main">
                  <div className="row wrap" style={{ gap: 8 }}>
                    <Pill tone={assisted.paused ? "warning" : "accent"}>
                      {assisted.paused ? "Cola en pausa" : "Cola lista"}
                    </Pill>
                    <span className="assisted-title">
                      {assisted.pending} parte(s) pendientes · {assisted.published} publicadas
                    </span>
                  </div>
                  <span className="small faint">
                    La cola se abre de una en una. Confirma cada parte antes de continuar.
                  </span>
                </div>
                <div className="assisted-actions">
                  <button
                    className="btn btn-sm btn-primary"
                    onClick={() => void startAssisted()}
                    disabled={busy !== null}
                  >
                    {assisted.paused ? <Play /> : <Send />}
                    {assisted.paused
                      ? "Reanudar cola"
                      : `Abrir parte ${
                          orderedAssisted.find((item) => item.status === "pending")?.partIndex ?? ""
                        }`}
                  </button>
                  <button className="btn btn-sm" onClick={cancelAssisted}>
                    Cancelar restantes
                  </button>
                </div>
              </>
            ) : (
              <>
                <div className="assisted-main">
                  <div className="row wrap" style={{ gap: 8 }}>
                    <Pill tone="success">
                      <Check /> Cola completada
                    </Pill>
                    <span className="assisted-title">
                      {assisted.published} parte(s) publicadas
                    </span>
                  </div>
                  <span className="small faint">
                    Todas las partes de esta cola fueron confirmadas.
                  </span>
                </div>
              </>
            )}
          </div>
        ) : null}

        {activeTab?.loading ? <div className="browser-loading-bar" /> : null}
        {notice ? (
          <div className="browser-notice">
            <Info size={13} />
            <span>{notice}</span>
            <button className="icon-btn" onClick={() => setNotice(null)} aria-label="Cerrar aviso">
              <X size={12} />
            </button>
          </div>
        ) : null}
        {error ? (
          <div className="browser-notice error">
            <AlertTriangle size={13} />
            <span>{error}</span>
            <button className="icon-btn" onClick={() => setError(null)} aria-label="Cerrar error">
              <X size={12} />
            </button>
          </div>
        ) : null}
      </div>

      <div className="browser-area" ref={surfaceRef}>
        {panel === null ? (
          <div className="browser-surface-placeholder">
            {loading ? (
              <>
                <Loader2 className="spin" size={26} />
                <span>Iniciando el navegador y los perfiles…</span>
              </>
            ) : activeTab ? (
              <>
                <Globe size={24} />
                <span>
                  {profileLabel(activeProfile)} · {currentHost || "página de inicio"}
                </span>
                <button
                  className="btn btn-sm"
                  onClick={() => void action("reload")}
                  disabled={!activeTab}
                >
                  <RefreshCw size={13} /> Recargar
                </button>
              </>
            ) : (
              <>
                <Globe size={24} />
                <span>No hay pestañas abiertas.</span>
                <button className="btn btn-sm" onClick={() => void newTab(activeProfile)}>
                  <Plus size={13} /> Abrir YouTube
                </button>
              </>
            )}
          </div>
        ) : null}

        {panel === "protection" && protection ? (
          <div className="browser-panel">
            <div className="browser-panel-head">
              <div>
                <h2>
                  <Shield size={18} /> Protección web de ParCut
                </h2>
                <p>
                  Bloqueo de anuncios y rastreadores conocidos. No es Brave Shields ni incluye
                  todas sus protecciones.
                </p>
              </div>
              <button className="btn btn-sm" onClick={() => void closePanel()}>
                Volver al navegador
              </button>
            </div>

            <div className="browser-panel-grid">
              <section className="card col" style={{ gap: 14 }}>
                <SwitchRow
                  label="Protección web activada"
                  hint="Se aplica a todas las pestañas y perfiles."
                  checked={protection.enabled}
                  onChange={(value) => void toggleProtection(value)}
                />
                <div className="field">
                  <label>Nivel de protección</label>
                  <Segmented
                    value={protection.level}
                    onChange={(value) => void changeLevel(value)}
                    options={[
                      { value: "standard", label: "Estándar" },
                      { value: "intensive", label: "Intensiva" },
                    ]}
                  />
                  <span className="field-hint">
                    Estándar usa EasyList y EasyPrivacy. Intensiva añade la lista de servidores de
                    Peter Lowe. La intensiva puede afectar a contenido incrustado.
                  </span>
                </div>
                <div className="row between">
                  <span className="small muted">Elementos bloqueados</span>
                  <strong className="mono-num">{protection.blockedTotal}</strong>
                </div>
                <div className="row between">
                  <span className="small muted">Reglas cargadas</span>
                  <strong className="mono-num">
                    {protection.engineReady ? protection.rules.toLocaleString("es-ES") : "Preparando…"}
                  </strong>
                </div>
                <div className="row between">
                  <span className="small muted">Perfil activo</span>
                  <strong>
                    {profileLabel(activeProfile)} · {blocked} bloqueados
                  </strong>
                </div>
                <div className="row wrap" style={{ gap: 8 }}>
                  <button
                    className="btn btn-sm"
                    onClick={() => void toggleSiteProtection()}
                    disabled={!currentHost || busy !== null}
                  >
                    {siteDisabled ? <Shield size={13} /> : <ShieldOff size={13} />}
                    {siteDisabled ? "Activar en este sitio" : "Desactivar en este sitio"}
                  </button>
                  {currentHost ? <span className="mono">{currentHost}</span> : null}
                </div>
                {protection.lastError ? (
                  <div className="alert warning">{protection.lastError}</div>
                ) : null}
              </section>

              <section className="card col" style={{ gap: 12 }}>
                <div className="row between">
                  <div>
                    <h3 className="card-title">
                      <Download size={16} /> Listas de filtros
                    </h3>
                    <p className="card-sub">Se actualizan desde sus fuentes oficiales.</p>
                  </div>
                  <button
                    className="btn btn-sm"
                    onClick={() => void updateLists()}
                    disabled={protection.updateRunning || busy !== null}
                  >
                    {protection.updateRunning ? <Loader2 className="spin" /> : <RefreshCw />}
                    {protection.updateRunning ? "Actualizando…" : "Actualizar"}
                  </button>
                </div>
                <div className="col" style={{ gap: 8 }}>
                  {protection.lists.map((list) => (
                    <div key={list.id} className="browser-list-row">
                      <div className="grow" style={{ minWidth: 0 }}>
                        <div className="row" style={{ gap: 6 }}>
                          <strong style={{ fontSize: 12.5 }}>{list.name}</strong>
                          {list.installed ? (
                            <Pill tone="success">Instalada</Pill>
                          ) : (
                            <Pill tone="muted">No descargada</Pill>
                          )}
                        </div>
                        <div className="small faint">
                          {list.license} · {formatBytes(list.bytes)}
                          {list.updatedAt ? ` · ${new Date(list.updatedAt).toLocaleDateString("es-ES")}` : ""}
                        </div>
                        {list.url ? (
                          <button
                            className="link-like"
                            onClick={() => void api.browserOpenExternal(list.url)}
                          >
                            {list.url}
                          </button>
                        ) : null}
                      </div>
                    </div>
                  ))}
                </div>
                <p className="field-hint">
                  EasyList y EasyPrivacy se distribuyen con licencia CC BY-SA 3.0 / GPLv3. ParCut
                  las descarga en tu equipo y no las redistribuye dentro del instalador.
                </p>
              </section>

              <section className="card col" style={{ gap: 10 }}>
                <h3 className="card-title">
                  <ShieldOff size={16} /> Excepciones por sitio
                </h3>
                {protection.sites.filter((site) => !site.enabled).length === 0 ? (
                  <p className="small faint">
                    No hay sitios con la protección desactivada. Usa «Desactivar en este sitio»
                    cuando una página no funcione correctamente.
                  </p>
                ) : (
                  protection.sites
                    .filter((site) => !site.enabled)
                    .map((site) => (
                      <div key={`${site.profile}-${site.host}`} className="account-row">
                        <div className="grow" style={{ minWidth: 0 }}>
                          <div className="account-name">{site.host}</div>
                          <div className="account-platform">{profileLabel(site.profile)}</div>
                        </div>
                        <button
                          className="btn btn-sm"
                          onClick={() =>
                            void runAction("site-remove", async () => {
                              await api.browserSiteRemove(site.profile, site.host);
                              await refresh();
                            })
                          }
                        >
                          Reactivar
                        </button>
                      </div>
                    ))
                )}
              </section>
            </div>
          </div>
        ) : null}

        {panel === "settings" ? (
          <div className="browser-panel">
            <div className="browser-panel-head">
              <div>
                <h2>
                  <Settings2 size={18} /> Configuración del navegador
                </h2>
                <p>Sesiones, perfiles, descargas y almacenamiento local.</p>
              </div>
              <button className="btn btn-sm" onClick={() => void closePanel()}>
                Volver al navegador
              </button>
            </div>

            {settings ? (
              <div className="browser-panel-grid">
                <section className="card col" style={{ gap: 14 }}>
                  <SwitchRow
                    label="Mantener sesión iniciada"
                    hint="Las cookies y el almacenamiento local se conservan entre ejecuciones."
                    checked={settings.browserKeepSession}
                    onChange={(value) => void setKeepSession(value)}
                  />
                  <div className="field">
                    <label>Carpeta de descargas</label>
                    <div className="row" style={{ gap: 6 }}>
                      <input
                        className="input"
                        value={settings.browserDownloadDir}
                        placeholder="Carpeta predeterminada de ParCut"
                        readOnly
                      />
                      <button className="btn btn-sm" onClick={() => void chooseDownloadDir()}>
                        <FolderOpen /> Elegir
                      </button>
                    </div>
                    <button
                      className="link-like"
                      onClick={() =>
                        void runAction("downloads", async () => {
                          await api.browserOpenDownloads();
                        })
                      }
                    >
                      Abrir carpeta de descargas
                    </button>
                  </div>
                  <SwitchRow
                    label="Abrir la carpeta al terminar una descarga"
                    hint="Se aplica a las descargas hechas desde el navegador integrado."
                    checked={settings.browserAutoOpenDownloads}
                    onChange={(value) =>
                      void saveSettings({ ...settings, browserAutoOpenDownloads: value })
                    }
                  />
                  <div className="alert">
                    <Info size={15} />
                    <div>
                      Google puede rechazar el inicio de sesión dentro de navegadores integrados.
                      Si ocurre, usa <strong>Abrir en el navegador del sistema</strong>. ParCut no
                      captura contraseñas ni cookies.
                    </div>
                  </div>
                </section>

                <section className="card col" style={{ gap: 12 }}>
                  <h3 className="card-title">
                    <Globe size={16} /> Perfiles y sesiones
                  </h3>
                  {(snapshot?.profiles ?? []).map((profile: BrowserProfileInfo) => (
                    <div key={profile.id} className="browser-profile-row">
                      <div className="browser-profile-head">
                        <span className={`platform-logo ${profile.id}`}>
                          <PlatformLogo platform={profile.id} size={18} />
                        </span>
                        <div className="grow" style={{ minWidth: 0 }}>
                          <strong style={{ fontSize: 12.5 }}>{profile.label}</strong>
                          <div className="small faint">
                            {profile.hasData
                              ? `${formatBytes(profile.sizeBytes)} en disco`
                              : "Sin datos guardados"}
                          </div>
                        </div>
                        <Pill tone={profile.hasData ? "success" : "muted"}>
                          {profile.hasData ? "Con datos" : "Vacío"}
                        </Pill>
                      </div>
                      <div className="row wrap" style={{ gap: 6 }}>
                        <button
                          className="btn btn-sm"
                          onClick={() => void openPlatform(profile.id, HOME_URLS[profile.id])}
                        >
                          Abrir
                        </button>
                        <button
                          className="btn btn-sm"
                          onClick={() => clearProfile(profile.id, "cache")}
                          disabled={busy !== null}
                        >
                          Limpiar caché
                        </button>
                        <button
                          className="btn btn-sm"
                          onClick={() => clearProfile(profile.id, "session")}
                          disabled={busy !== null}
                        >
                          Borrar sesión
                        </button>
                        <button
                          className="btn btn-sm btn-danger"
                          onClick={() => clearProfile(profile.id, "profile")}
                          disabled={busy !== null}
                        >
                          <Trash2 /> Eliminar perfil
                        </button>
                      </div>
                    </div>
                  ))}
                </section>

                <section className="card col" style={{ gap: 10 }}>
                  <div className="row between">
                    <h3 className="card-title">
                      <UploadCloud size={16} /> Descargas recientes
                    </h3>
                    <button
                      className="btn btn-sm btn-ghost"
                      onClick={() => void api.browserOpenDownloads()}
                    >
                      <FolderOpen /> Carpeta
                    </button>
                  </div>
                  {(snapshot?.downloads ?? []).length === 0 ? (
                    <p className="small faint">No hay descargas registradas.</p>
                  ) : (
                    (snapshot?.downloads ?? []).slice(0, 8).map((download) => (
                      <div key={download.id} className="account-row">
                        <div className="grow" style={{ minWidth: 0 }}>
                          <div className="account-name">
                            {download.path.split(/[\\/]/).pop()}
                          </div>
                          <div className="account-platform">
                            {profileLabel(download.profile)} · {download.status}
                          </div>
                        </div>
                        <button
                          className="icon-btn"
                          onClick={() => void api.revealPath(download.path)}
                          title="Abrir ubicación"
                        >
                          <FolderOpen />
                        </button>
                      </div>
                    ))
                  )}
                </section>
              </div>
            ) : (
              <p className="muted">Cargando configuración…</p>
            )}
          </div>
        ) : null}
      </div>

      <Modal
        open={confirm !== null}
        title={confirm?.title ?? ""}
        subtitle={confirm?.message}
        onClose={() => setConfirm(null)}
        busy={busy !== null}
        actions={
          <>
            <button className="btn btn-ghost" onClick={() => setConfirm(null)} disabled={busy !== null}>
              Cancelar
            </button>
            <button
              className={`btn ${confirm?.danger ? "btn-danger" : "btn-primary"}`}
              disabled={busy !== null}
              onClick={async () => {
                if (!confirm) return;
                const run = confirm.run;
                setConfirm(null);
                await run();
              }}
            >
              {busy !== null ? <Loader2 className="spin" /> : <Trash2 />}
              {confirm?.confirmLabel ?? "Confirmar"}
            </button>
          </>
        }
      >
        <p className="small muted">
          Esta acción solo afecta al perfil seleccionado. Las sesiones de las demás plataformas y
          los proyectos de video no se modifican.
        </p>
      </Modal>

      <Modal
        open={failOpen}
        title={`Marcar problema en la parte ${assistedActive?.partIndex ?? ""}`}
        subtitle="La cola se pausará para que no se publique la siguiente parte."
        onClose={() => setFailOpen(false)}
        busy={busy === "assisted-fail"}
        actions={
          <>
            <button
              className="btn btn-ghost"
              onClick={() => setFailOpen(false)}
              disabled={busy === "assisted-fail"}
            >
              Cancelar
            </button>
            <button
              className="btn btn-danger"
              onClick={() => void submitAssistedFail()}
              disabled={busy === "assisted-fail"}
            >
              {busy === "assisted-fail" ? <Loader2 className="spin" /> : <AlertCircle />}
              Marcar error y pausar
            </button>
          </>
        }
      >
        <div className="field">
          <label htmlFor="assisted-fail">Motivo (opcional)</label>
          <textarea
            id="assisted-fail"
            className="textarea"
            value={failMessage}
            onChange={(event) => setFailMessage(event.target.value)}
            placeholder="Ejemplo: la plataforma rechazó el archivo o la subida no terminó."
            maxLength={500}
          />
          <span className="field-hint">
            No se marcará como publicada. Podrás reintentar esta parte desde la cola.
          </span>
        </div>
      </Modal>
    </div>
  );
}
