import { useEffect, useState } from "react";
import {
  AlertCircle,
  CheckCircle2,
  Copy,
  ExternalLink,
  Eye,
  EyeOff,
  KeyRound,
  Loader2,
  Link2,
  ShieldCheck,
  Trash2,
  Unplug,
  Users,
} from "lucide-react";
import {
  api,
  Account,
  errorMessage,
  ManualAuth,
  PlatformStatus,
} from "../lib/api";
import { useAppStore } from "../store/app";
import { Alert, Modal, Pill } from "../components/ui";
import { PlatformLogo } from "../components/PlatformLogo";

type CredentialForm = { clientId: string; clientSecret: string; showSecret: boolean };

export function AccountsPage() {
  const platforms = useAppStore((state) => state.platforms);
  const accounts = useAppStore((state) => state.accounts);
  const settings = useAppStore((state) => state.settings);
  const loadAccounts = useAppStore((state) => state.loadAccounts);
  const loadSettings = useAppStore((state) => state.loadSettings);
  const toast = useAppStore((state) => state.toast);

  const [forms, setForms] = useState<Record<string, CredentialForm>>({});
  const [connecting, setConnecting] = useState<string | null>(null);
  const [saving, setSaving] = useState<string | null>(null);
  const [error, setError] = useState<{ platform: string; message: string } | null>(null);
  const [manual, setManual] = useState<{ platform: string; auth: ManualAuth } | null>(null);
  const [manualUrl, setManualUrl] = useState("");
  const [manualBusy, setManualBusy] = useState(false);
  const [disconnectTarget, setDisconnectTarget] = useState<Account | null>(null);

  useEffect(() => {
    if (!settings) return;
    setForms((current) => {
      const next = { ...current };
      const ids: Record<string, string> = {
        youtube: settings.youtubeClientId,
        facebook: settings.facebookAppId,
        tiktok: settings.tiktokClientKey,
      };
      for (const [platform, clientId] of Object.entries(ids)) {
        next[platform] = {
          clientId: next[platform]?.clientId || clientId,
          clientSecret: "",
          showSecret: next[platform]?.showSecret ?? false,
        };
      }
      return next;
    });
  }, [settings]);

  const updateForm = (platform: string, patch: Partial<CredentialForm>) => {
    setForms((current) => ({
      ...current,
      [platform]: {
        ...(current[platform] ?? { clientId: "", clientSecret: "", showSecret: false }),
        ...patch,
      },
    }));
  };

  const saveCredentials = async (platform: string) => {
    const form = forms[platform];
    if (!form) return;
    setSaving(platform);
    setError(null);
    try {
      await api.platformSaveCredentials(platform, form.clientId, form.clientSecret);
      updateForm(platform, { clientSecret: "" });
      toast("success", "Credenciales guardadas. El secreto queda cifrado con DPAPI.");
      await Promise.all([loadAccounts(), loadSettings()]);
    } catch (err) {
      setError({ platform, message: errorMessage(err) });
    } finally {
      setSaving(null);
    }
  };

  const clearCredentials = async (platform: string) => {
    setSaving(platform);
    try {
      await api.platformClearCredentials(platform);
      updateForm(platform, { clientId: "", clientSecret: "" });
      toast("info", "Credenciales eliminadas de este equipo.");
      await Promise.all([loadAccounts(), loadSettings()]);
    } catch (err) {
      setError({ platform, message: errorMessage(err) });
    } finally {
      setSaving(null);
    }
  };

  const connect = async (platform: string) => {
    setConnecting(platform);
    setError(null);
    try {
      const account = await api.oauthConnect(platform);
      toast("success", `Cuenta conectada: ${account.displayName}`);
      await loadAccounts();
    } catch (err) {
      setError({ platform, message: errorMessage(err) });
    } finally {
      setConnecting(null);
    }
  };

  const startManual = async (platform: string) => {
    setError(null);
    try {
      const auth = await api.oauthStartManual(platform);
      setManual({ platform, auth });
      setManualUrl("");
    } catch (err) {
      setError({ platform, message: errorMessage(err) });
    }
  };

  const completeManual = async () => {
    if (!manual) return;
    setManualBusy(true);
    setError(null);
    try {
      const account = await api.oauthCompleteManual(manual.platform, manualUrl.trim());
      toast("success", `Cuenta conectada: ${account.displayName}`);
      setManual(null);
      await loadAccounts();
    } catch (err) {
      setError({ platform: manual.platform, message: errorMessage(err) });
    } finally {
      setManualBusy(false);
    }
  };

  const disconnect = async () => {
    if (!disconnectTarget) return;
    try {
      await api.accountDisconnect(disconnectTarget.id);
      toast("info", "Cuenta desconectada y tokens eliminados.");
      setDisconnectTarget(null);
      await loadAccounts();
    } catch (err) {
      toast("error", errorMessage(err));
    }
  };

  const copy = async (text: string, message: string) => {
    try {
      await navigator.clipboard.writeText(text);
      toast("success", message);
    } catch {
      toast("error", "No se pudo copiar al portapapeles.");
    }
  };

  return (
    <div className="page">
      <div className="section-head">
        <div>
          <h2>Cuentas conectadas</h2>
          <p>
            Autorización oficial mediante OAuth. ParCut nunca solicita ni almacena contraseñas.
          </p>
        </div>
        <div className="spacer" />
        <button className="btn btn-sm" onClick={() => void loadAccounts()}>
          <Users /> Actualizar
        </button>
      </div>

      <Alert tone="info" icon={<ShieldCheck size={16} />}>
        Los tokens se guardan cifrados con Windows DPAPI, ligados a tu usuario. Cada plataforma
        necesita credenciales de desarrollador propias y, en algunos casos, revisión de permisos.
        Sin ellas, la interfaz lo indica y no simula una conexión.
      </Alert>

      <div className="platform-grid">
        {platforms.map((platform) => {
          const platformAccounts = accounts.filter(
            (account) => account.platform === platform.platform,
          );
          const form = forms[platform.platform] ?? {
            clientId: "",
            clientSecret: "",
            showSecret: false,
          };
          const isBusy = connecting === platform.platform;
          return (
            <PlatformCard
              key={platform.platform}
              platform={platform}
              accounts={platformAccounts}
              form={form}
              busy={isBusy || saving === platform.platform}
              connecting={isBusy}
              onFormChange={(patch) => updateForm(platform.platform, patch)}
              onSave={() => void saveCredentials(platform.platform)}
              onClear={() => void clearCredentials(platform.platform)}
              onConnect={() => void connect(platform.platform)}
              onManual={() => void startManual(platform.platform)}
              onDisconnect={setDisconnectTarget}
              onCopy={copy}
              error={
                error?.platform === platform.platform ? error.message : null
              }
            />
          );
        })}
      </div>

      <section className="card flat">
        <div className="row wrap" style={{ gap: 14 }}>
          <KeyRound size={18} style={{ color: "var(--accent)" }} />
          <div className="grow" style={{ minWidth: 260 }}>
            <strong style={{ fontSize: 12.5 }}>¿Por qué necesito credenciales propias?</strong>
            <p className="field-hint" style={{ margin: "3px 0 0", lineHeight: 1.6 }}>
              Cada plataforma exige que la aplicación esté registrada por su propietario. ParCut
              incluye todo el flujo técnico (PKCE, callback local, cifrado, adaptadores de subida),
              pero no distribuye secretos de terceros. Configura las credenciales en cada tarjeta y
              registra la URI de redirección indicada en el panel del proveedor.
            </p>
          </div>
        </div>
      </section>

      <Modal
        open={manual !== null}
        title={`Autorización manual · ${manual ? manual.platform : ""}`}
        subtitle="Úsala si el navegador no puede volver a ParCut automáticamente."
        onClose={() => setManual(null)}
        wide
        busy={manualBusy}
        actions={
          <>
            <button className="btn btn-ghost" onClick={() => setManual(null)} disabled={manualBusy}>
              Cancelar
            </button>
            <button
              className="btn btn-primary"
              onClick={completeManual}
              disabled={manualBusy || manualUrl.trim().length === 0}
            >
              {manualBusy ? <Loader2 className="spin" /> : <Link2 />}
              Completar conexión
            </button>
          </>
        }
      >
        {manual ? (
          <div className="col" style={{ gap: 12 }}>
            <p className="small muted" style={{ margin: 0 }}>
              1. Autoriza la aplicación en el navegador que se acaba de abrir.
            </p>
            <p className="small muted" style={{ margin: 0 }}>
              2. Al terminar, la página redirigirá a{" "}
              <span className="mono">{manual.auth.redirectUri}</span>. Si esa página no carga,
              copia su dirección completa desde la barra del navegador.
            </p>
            <div className="row" style={{ gap: 8 }}>
              <button
                className="btn btn-sm"
                onClick={() => void api.openExternal(manual.auth.authUrl)}
              >
                <ExternalLink /> Abrir de nuevo
              </button>
              <button
                className="btn btn-sm btn-ghost"
                onClick={() => void copy(manual.auth.authUrl, "Enlace de autorización copiado.")}
              >
                <Copy /> Copiar enlace
              </button>
            </div>
            <div className="field">
              <label htmlFor="manual-url">URL de redirección</label>
              <input
                id="manual-url"
                className="input"
                placeholder="http://127.0.0.1:17421/callback?code=…"
                value={manualUrl}
                onChange={(event) => setManualUrl(event.target.value)}
                spellCheck={false}
              />
              <span className="field-hint">
                La URL contiene un código de un solo uso; ParCut lo canjea al instante y no lo
                guarda.
              </span>
            </div>
          </div>
        ) : null}
      </Modal>

      <Modal
        open={disconnectTarget !== null}
        title="Desconectar cuenta"
        subtitle="Se eliminarán los tokens cifrados de este equipo."
        onClose={() => setDisconnectTarget(null)}
        actions={
          <>
            <button className="btn btn-ghost" onClick={() => setDisconnectTarget(null)}>
              Cancelar
            </button>
            <button className="btn btn-danger" onClick={disconnect}>
              <Unplug /> Desconectar
            </button>
          </>
        }
      >
        <p className="small muted">
          {disconnectTarget
            ? `${disconnectTarget.displayName} dejará de estar disponible para publicar. Las publicaciones ya realizadas no se modifican.`
            : ""}
        </p>
      </Modal>
    </div>
  );
}

function PlatformCard({
  platform,
  accounts,
  form,
  busy,
  connecting,
  onFormChange,
  onSave,
  onClear,
  onConnect,
  onManual,
  onDisconnect,
  onCopy,
  error,
}: {
  platform: PlatformStatus;
  accounts: Account[];
  form: CredentialForm;
  busy: boolean;
  connecting: boolean;
  onFormChange: (patch: Partial<CredentialForm>) => void;
  onSave: () => void;
  onClear: () => void;
  onConnect: () => void;
  onManual: () => void;
  onDisconnect: (account: Account) => void;
  onCopy: (text: string, message: string) => void;
  error: string | null;
}) {
  return (
    <div className="platform-card">
      <div className="platform-head">
        <span className={`platform-logo ${platform.platform}`}>
          <PlatformLogo platform={platform.platform} size={24} />
        </span>
        <div className="grow">
          <h3>{platform.displayName}</h3>
          <p>
            {platform.connectedAccounts > 0
              ? `${platform.connectedAccounts} cuenta(s) conectada(s)`
              : "Sin cuentas conectadas"}
          </p>
        </div>
        <Pill tone={platform.configured ? "success" : "warning"}>
          {platform.configured ? "App configurada" : "Falta configuración"}
        </Pill>
      </div>

      {accounts.length > 0 ? (
        <div className="col" style={{ gap: 8 }}>
          {accounts.map((account) => (
            <div key={account.id} className="account-row">
              <CheckCircle2 size={15} style={{ color: "var(--success)", flex: "none" }} />
              <div className="grow" style={{ minWidth: 0 }}>
                <div className="account-name">{account.displayName}</div>
                <div className="account-platform">{account.externalId}</div>
              </div>
              <button
                className="icon-btn danger"
                title="Desconectar cuenta"
                onClick={() => onDisconnect(account)}
              >
                <Unplug />
              </button>
            </div>
          ))}
        </div>
      ) : null}

      {error ? (
        <Alert tone="danger" icon={<AlertCircle size={16} />}>
          <span style={{ display: "block", wordBreak: "break-word" }}>{error}</span>
          <button className="btn btn-sm" style={{ marginTop: 8 }} onClick={onManual}>
            <Link2 /> Probar autorización manual
          </button>
        </Alert>
      ) : null}

      <div className="row" style={{ gap: 8 }}>
        <button className="btn btn-primary grow" onClick={onConnect} disabled={busy}>
          {connecting ? <Loader2 className="spin" /> : <Link2 />}
          {connecting ? "Esperando autorización…" : "Conectar cuenta"}
        </button>
        <button className="btn" onClick={onManual} title="Autorización manual" disabled={busy}>
          <ExternalLink />
        </button>
      </div>

      <details className="credentials">
        <summary>
          <KeyRound size={14} /> Credenciales de desarrollador
        </summary>
        <div className="field" style={{ marginTop: 10 }}>
          <label htmlFor={`client-${platform.platform}`}>
            {platform.platform === "tiktok" ? "Client key" : "Client ID / App ID"}
          </label>
          <input
            id={`client-${platform.platform}`}
            className="input"
            value={form.clientId}
            onChange={(event) => onFormChange({ clientId: event.target.value })}
            placeholder="Identificador público de tu app"
            spellCheck={false}
          />
        </div>
        <div className="field">
          <label htmlFor={`secret-${platform.platform}`}>Client secret</label>
          <div className="row" style={{ gap: 6 }}>
            <input
              id={`secret-${platform.platform}`}
              className="input"
              type={form.showSecret ? "text" : "password"}
              value={form.clientSecret}
              onChange={(event) => onFormChange({ clientSecret: event.target.value })}
              placeholder={
                platform.hasClientSecret ? "Guardado · introduce uno nuevo para cambiarlo" : "Secreto de la app"
              }
              spellCheck={false}
            />
            <button
              className="icon-btn"
              onClick={() => onFormChange({ showSecret: !form.showSecret })}
              title={form.showSecret ? "Ocultar" : "Mostrar"}
            >
              {form.showSecret ? <EyeOff /> : <Eye />}
            </button>
          </div>
          <span className="field-hint">
            Se cifra con DPAPI y nunca se envía al frontend ni al repositorio.
          </span>
        </div>
        <div className="row wrap" style={{ gap: 8 }}>
          <button
            className="btn btn-sm"
            onClick={onSave}
            disabled={busy || !form.clientId.trim() || !form.clientSecret.trim()}
          >
            {busy ? <Loader2 className="spin" /> : <ShieldCheck />} Guardar
          </button>
          <button className="btn btn-sm btn-ghost" onClick={onClear} disabled={busy}>
            <Trash2 /> Quitar
          </button>
        </div>
        <div className="field">
          <span className="field-label">URI de redirección</span>
          <div className="row" style={{ gap: 6 }}>
            <span className="mono grow" style={{ overflow: "hidden", textOverflow: "ellipsis" }}>
              {platform.redirectUri}
            </span>
            <button
              className="icon-btn"
              onClick={() => onCopy(platform.redirectUri, "URI de redirección copiada.")}
              title="Copiar URI"
            >
              <Copy />
            </button>
          </div>
          <span className="field-hint">
            Regístrala exactamente así en el panel de {platform.displayName}. Los puertos fijos
            evitan errores de coincidencia.
          </span>
        </div>
        <p className="field-hint" style={{ margin: 0 }}>
          {platform.requirements}
        </p>
        <div className="row wrap" style={{ gap: 6 }}>
          <button
            className="btn btn-sm btn-ghost"
            onClick={() => void api.openExternal(platform.docsUrl)}
          >
            <ExternalLink /> Documentación oficial
          </button>
        </div>
        <div className="row wrap" style={{ gap: 4 }}>
          {platform.scopes.map((scope) => (
            <span key={scope} className="pill muted" style={{ fontSize: 10 }}>
              {scope.split("/").pop()}
            </span>
          ))}
        </div>
      </details>
    </div>
  );
}
