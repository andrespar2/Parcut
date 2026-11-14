use super::profiles;
use super::{protection, BrowserBounds, BrowserDownload, BrowserTabInfo};
use crate::error::{fail, AppError, AppResult};
use crate::state::AppState;
use crate::utils::now_iso;
use serde_json::json;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::Ordering;
use tauri::webview::{DownloadEvent, NewWindowResponse, PageLoadEvent, WebviewBuilder};
use tauri::{
    AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, Webview, WebviewUrl, Wry,
};
use tauri_plugin_opener::OpenerExt;
use webview2_com::Microsoft::Web::WebView2::Win32::{
    CreateCoreWebView2EnvironmentWithOptions, ICoreWebView2Environment,
    ICoreWebView2EnvironmentOptions, ICoreWebView2Profile2, ICoreWebView2_13,
    COREWEBVIEW2_BROWSING_DATA_KINDS_CACHE_STORAGE, COREWEBVIEW2_BROWSING_DATA_KINDS_COOKIES,
    COREWEBVIEW2_BROWSING_DATA_KINDS_DISK_CACHE, COREWEBVIEW2_BROWSING_DATA_KINDS_INDEXED_DB,
    COREWEBVIEW2_BROWSING_DATA_KINDS_LOCAL_STORAGE, COREWEBVIEW2_BROWSING_DATA_KINDS_SERVICE_WORKERS,
};
use webview2_com::{
    ClearBrowsingDataCompletedHandler, CreateCoreWebView2EnvironmentCompletedHandler,
};
use windows::core::{Interface, HSTRING, PCWSTR};

thread_local! {
    static TAB_VIEWS: RefCell<HashMap<String, Webview<Wry>>> = RefCell::new(HashMap::new());
    static ENVIRONMENTS: RefCell<HashMap<String, ICoreWebView2Environment>> = RefCell::new(HashMap::new());
}

fn is_safe_navigation(url: &url::Url) -> bool {
    matches!(url.scheme(), "http" | "https") || (url.scheme() == "about" && url.path() == "blank")
}

fn tab_title(profile: &str, url: &str) -> String {
    let host = url::Url::parse(url)
        .ok()
        .and_then(|parsed| parsed.host_str().map(|host| host.to_string()))
        .unwrap_or_default();
    if host.is_empty() || host == "about" {
        profiles::label(profile).to_string()
    } else {
        host.trim_start_matches("www.").to_string()
    }
}

fn update_tab(app: &AppHandle, id: &str, update: impl FnOnce(&mut BrowserTabInfo)) {
    let state = app.state::<AppState>();
    let mut tabs = state.browser.tabs.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(tab) = tabs.iter_mut().find(|tab| tab.id == id) {
        update(tab);
    }
}

fn is_active(app: &AppHandle, id: &str) -> bool {
    let state = app.state::<AppState>();
    let active = state
        .browser
        .active
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    active.as_deref() == Some(id)
}

fn record_download(app: &AppHandle, profile: &str, url: &str, path: &std::path::Path, status: &str) {
    let state = app.state::<AppState>();
    let id = format!("dl-{}", state.browser.seq.fetch_add(1, Ordering::SeqCst));
    let mut downloads = state
        .browser
        .downloads
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    downloads.push(BrowserDownload {
        id: id.clone(),
        profile: profile.to_string(),
        url: url.to_string(),
        path: path.display().to_string(),
        status: status.to_string(),
        created_at: now_iso(),
    });
    if downloads.len() > 50 {
        let excess = downloads.len() - 50;
        downloads.drain(0..excess);
    }
    let _ = app.emit("browser:download", json!({ "id": id, "status": status }));
}

fn ensure_environment(app: &AppHandle, profile: &str) -> AppResult<ICoreWebView2Environment> {
    if let Some(environment) = ENVIRONMENTS.with(|map| map.borrow().get(profile).cloned()) {
        return Ok(environment);
    }
    let dir = profiles::ensure_dir(app, profile)?;
    let slot: Rc<RefCell<Option<ICoreWebView2Environment>>> = Rc::new(RefCell::new(None));
    let slot_ref = slot.clone();
    let dir_string = dir.display().to_string();
    CreateCoreWebView2EnvironmentCompletedHandler::wait_for_async_operation(
        Box::new(move |handler| unsafe {
            CreateCoreWebView2EnvironmentWithOptions(
                PCWSTR::null(),
                &HSTRING::from(dir_string),
                None::<&ICoreWebView2EnvironmentOptions>,
                &handler,
            )
            .map_err(webview2_com::Error::WindowsError)
        }),
        Box::new(move |code, environment| {
            code?;
            *slot_ref.borrow_mut() = environment;
            Ok(())
        }),
    )
    .map_err(|err| AppError(format!("No se pudo iniciar el navegador integrado: {err}")))?;
    let environment = slot
        .borrow()
        .clone()
        .ok_or_else(|| AppError("WebView2 no devolviÃ³ el entorno del perfil.".into()))?;
    ENVIRONMENTS.with(|map| {
        map.borrow_mut()
            .insert(profile.to_string(), environment.clone());
    });
    Ok(environment)
}

fn bounds(app: &AppHandle) -> BrowserBounds {
    let state = app.state::<AppState>();
    let value = *state.browser.bounds.lock().unwrap_or_else(|e| e.into_inner());
    value
}

fn create_tab(app: &AppHandle, profile: &str, url: &str) -> AppResult<BrowserTabInfo> {
    profiles::validate_profile(profile)?;
    let state = app.state::<AppState>();
    let sequence = state.browser.seq.fetch_add(1, Ordering::SeqCst);
    let id = format!("browser-{profile}-{sequence}");
    let environment = ensure_environment(app, profile)?;
    let initial = url::Url::parse("about:blank")?;
    let current_bounds = bounds(app);
    let width = current_bounds.width.max(2.0);
    let height = current_bounds.height.max(2.0);
    let window = app
        .get_window("main")
        .ok_or_else(|| AppError("La ventana principal de ParCut no estÃ¡ disponible.".into()))?;

    let app_nav = app.clone();
    let id_nav = id.clone();
    let profile_nav = profile.to_string();
    let app_load = app.clone();
    let id_load = id.clone();
    let profile_load = profile.to_string();
    let app_new = app.clone();
    let app_dl = app.clone();
    let profile_dl = profile.to_string();
    let app_title = app.clone();
    let id_title = id.clone();

    let builder = WebviewBuilder::new(&id, WebviewUrl::External(initial))
        .with_environment(environment)
        .on_document_title_changed(move |_webview, title| {
            let clean: String = title.trim().chars().take(90).collect();
            if clean.is_empty() {
                return;
            }
            update_tab(&app_title, &id_title, |tab| {
                tab.title = clean;
            });
            let _ = app_title.emit("browser:changed", ());
        })
        .on_navigation(move |navigation| {
            if !is_safe_navigation(navigation) {
                let _ = app_nav.emit(
                    "browser:notice",
                    json!({ "message": "Se bloqueÃ³ una navegaciÃ³n con un esquema no permitido." }),
                );
                return false;
            }
            if matches!(navigation.scheme(), "http" | "https") {
                update_tab(&app_nav, &id_nav, |tab| {
                    tab.url = navigation.to_string();
                    tab.title = tab_title(&profile_nav, navigation.as_str());
                });
                if is_active(&app_nav, &id_nav) {
                    protection::set_current_url(&app_nav, &profile_nav, navigation.as_str());
                }
            }
            true
        })
        .on_page_load(move |_webview, payload| {
            let started = matches!(payload.event(), PageLoadEvent::Started);
            update_tab(&app_load, &id_load, |tab| {
                tab.loading = started;
                tab.url = payload.url().to_string();
                if started {
                    tab.title = tab_title(&profile_load, payload.url().as_str());
                }
            });
            if !started && is_active(&app_load, &id_load) {
                protection::set_current_url(&app_load, &profile_load, payload.url().as_str());
            }
            let _ = app_load.emit("browser:changed", ());
        })
        .on_new_window(move |popup_url, _features| {
            let host = popup_url.host_str().unwrap_or("");
            if popup_url.scheme() == "https" && super::is_popup_host_allowed(host) {
                NewWindowResponse::Allow
            } else {
                let _ = app_new.emit(
                    "browser:notice",
                    json!({ "message": format!("Se bloqueÃ³ una ventana emergente: {host}") }),
                );
                NewWindowResponse::Deny
            }
        })
        .on_download(move |_webview, event| match event {
            DownloadEvent::Requested { url, destination } => {
                match profiles::download_destination(&app_dl, &profile_dl, &url) {
                    Ok(path) if !profiles::is_blocked_download(&path) => {
                        *destination = path.clone();
                        record_download(&app_dl, &profile_dl, url.as_str(), &path, "downloading");
                        true
                    }
                    Ok(path) => {
                        let _ = app_dl.emit(
                            "browser:notice",
                            json!({ "message": format!("Descarga bloqueada por seguridad: {}", path.display()) }),
                        );
                        false
                    }
                    Err(err) => {
                        let _ = app_dl.emit("browser:notice", json!({ "message": err.to_string() }));
                        false
                    }
                }
            }
            DownloadEvent::Finished { url, path, success } => {
                if let Some(path) = path {
                    record_download(
                        &app_dl,
                        &profile_dl,
                        url.as_str(),
                        &path,
                        if success { "completed" } else { "failed" },
                    );
                    if success {
                        let state = app_dl.state::<AppState>();
                        if state.settings_snapshot().browser_auto_open_downloads {
                            if let Some(parent) = path.parent() {
                                let _ = app_dl
                                    .opener()
                                    .open_path(parent.display().to_string(), None::<&str>);
                            }
                        }
                    }
                }
                true
            }
            _ => true,
        });

    let webview = window.add_child(
        builder,
        LogicalPosition::new(current_bounds.x, current_bounds.y),
        LogicalSize::new(width, height),
    )?;
    let _ = webview.hide();
    let _ = protection::attach(app, profile, &webview);

    let tab = BrowserTabInfo {
        id: id.clone(),
        profile: profile.to_string(),
        title: tab_title(profile, url),
        url: url.to_string(),
        loading: true,
        home_url: profiles::home_url(profile).to_string(),
    };
    {
        let mut tabs = state.browser.tabs.lock().unwrap_or_else(|e| e.into_inner());
        tabs.push(tab.clone());
    }
    TAB_VIEWS.with(|views| {
        views.borrow_mut().insert(id.clone(), webview.clone());
    });
    webview.navigate(url::Url::parse(url)?)?;
    let _ = app.emit("browser:changed", ());
    Ok(tab)
}

fn ensure_default_tabs(app: &AppHandle) -> AppResult<()> {
    let state = app.state::<AppState>();
    for profile in profiles::PROFILES {
        let exists = state
            .browser
            .tabs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .any(|tab| tab.profile == profile);
        if !exists {
            create_tab(app, profile, profiles::home_url(profile))?;
        }
    }
    let mut active = state.browser.active.lock().unwrap_or_else(|e| e.into_inner());
    if active.is_none() {
        *active = state
            .browser
            .tabs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .first()
            .map(|tab| tab.id.clone());
    }
    Ok(())
}

fn apply_layout(app: &AppHandle) -> AppResult<()> {
    let state = app.state::<AppState>();
    let active = state.browser.active.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let visible = state.browser.visible.load(Ordering::SeqCst);
    let current = bounds(app);
    let valid = current.width > 2.0 && current.height > 2.0;
    TAB_VIEWS.with(|views| {
        for (id, view) in views.borrow().iter() {
            if visible && valid && Some(id.clone()) == active {
                let _ = view.set_position(LogicalPosition::new(current.x, current.y));
                let _ = view.set_size(LogicalSize::new(current.width, current.height));
                let _ = view.show();
            } else {
                let _ = view.hide();
            }
        }
    });
    Ok(())
}

fn close_all_views() {
    let views: Vec<Webview<Wry>> = TAB_VIEWS.with(|map| map.borrow().values().cloned().collect());
    for view in views {
        let _ = view.close();
    }
    TAB_VIEWS.with(|map| map.borrow_mut().clear());
    ENVIRONMENTS.with(|map| map.borrow_mut().clear());
}

/// Prepara el navegador y las tres pestaÃ±as base (una por plataforma).
pub fn prepare(app: &AppHandle) -> AppResult<()> {
    let state = app.state::<AppState>();
    let settings = state.settings_snapshot();
    if !settings.browser_keep_session
        && !state.browser.privacy_purged.swap(true, Ordering::SeqCst)
    {
        close_all_views();
        state
            .browser
            .tabs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
        *state.browser.active.lock().unwrap_or_else(|e| e.into_inner()) = None;
        profiles::remove_all(app)?;
    }
    ensure_default_tabs(app)?;
    state.browser.visible.store(true, Ordering::SeqCst);
    state.browser.initialized.store(true, Ordering::SeqCst);
    apply_layout(app)?;
    protection::ensure_lists_fresh(app);
    let _ = app.emit("browser:changed", ());
    Ok(())
}

pub fn hide(app: &AppHandle) -> AppResult<()> {
    let state = app.state::<AppState>();
    state.browser.visible.store(false, Ordering::SeqCst);
    apply_layout(app)
}

pub fn set_bounds(app: &AppHandle, new_bounds: BrowserBounds) -> AppResult<()> {
    let state = app.state::<AppState>();
    *state.browser.bounds.lock().unwrap_or_else(|e| e.into_inner()) = new_bounds;
    // La página del navegador es quien envía los límites: si los envía, debe estar visible.
    state.browser.visible.store(true, Ordering::SeqCst);
    apply_layout(app)
}

pub fn activate(app: &AppHandle, tab_id: &str) -> AppResult<()> {
    let state = app.state::<AppState>();
    let tab = state
        .browser
        .tabs
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .find(|tab| tab.id == tab_id)
        .cloned()
        .ok_or_else(|| AppError("La pestaÃ±a no existe.".into()))?;
    *state.browser.active.lock().unwrap_or_else(|e| e.into_inner()) = Some(tab_id.to_string());
    protection::set_current_url(app, &tab.profile, &tab.url);
    apply_layout(app)?;
    let _ = app.emit("browser:changed", ());
    Ok(())
}

pub fn new_tab(app: &AppHandle, profile: &str) -> AppResult<BrowserTabInfo> {
    profiles::validate_profile(profile)?;
    let tab = create_tab(app, profile, profiles::home_url(profile))?;
    activate(app, &tab.id)?;
    Ok(tab)
}

pub fn close_tab(app: &AppHandle, tab_id: &str) -> AppResult<()> {
    let state = app.state::<AppState>();
    let removed = {
        let mut tabs = state.browser.tabs.lock().unwrap_or_else(|e| e.into_inner());
        let position = tabs.iter().position(|tab| tab.id == tab_id);
        position.map(|index| tabs.remove(index))
    };
    let Some(removed) = removed else {
        return fail("La pestaÃ±a ya no existe.");
    };
    if let Some(view) = TAB_VIEWS.with(|views| views.borrow_mut().remove(tab_id)) {
        let _ = view.close();
    }
    let mut active = state.browser.active.lock().unwrap_or_else(|e| e.into_inner());
    if active.as_deref() == Some(tab_id) {
        let tabs = state.browser.tabs.lock().unwrap_or_else(|e| e.into_inner());
        *active = tabs
            .iter()
            .find(|tab| tab.profile == removed.profile)
            .or_else(|| tabs.first())
            .map(|tab| tab.id.clone());
    }
    drop(active);
    apply_layout(app)?;
    let _ = app.emit("browser:changed", ());
    Ok(())
}

pub fn navigate(app: &AppHandle, tab_id: Option<String>, url: url::Url) -> AppResult<()> {
    let state = app.state::<AppState>();
    let target = match tab_id {
        Some(id) => Some(id),
        None => state.browser.active.lock().unwrap_or_else(|e| e.into_inner()).clone(),
    }
    .ok_or_else(|| AppError("No hay una pestaÃ±a activa.".into()))?;
    let tab = state
        .browser
        .tabs
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .find(|tab| tab.id == target)
        .cloned()
        .ok_or_else(|| AppError("La pestaÃ±a no existe.".into()))?;
    if !is_active(app, &target) {
        activate(app, &target)?;
    }
    let view = TAB_VIEWS
        .with(|views| views.borrow().get(&target).cloned())
        .ok_or_else(|| AppError("La pestaÃ±a no estÃ¡ disponible.".into()))?;
    view.navigate(url.clone())?;
    update_tab(app, &target, |tab| {
        tab.url = url.to_string();
        tab.loading = true;
        tab.title = tab_title(&tab.profile, url.as_str());
    });
    protection::set_current_url(app, &tab.profile, url.as_str());
    let _ = app.emit("browser:changed", ());
    Ok(())
}

pub fn open_platform(app: &AppHandle, profile: &str, url: &str) -> AppResult<BrowserTabInfo> {
    profiles::validate_profile(profile)?;
    let state = app.state::<AppState>();
    let tab = {
        let tabs = state.browser.tabs.lock().unwrap_or_else(|e| e.into_inner());
        tabs.iter().find(|tab| tab.profile == profile).cloned()
    };
    let tab = match tab {
        Some(tab) => tab,
        None => create_tab(app, profile, profiles::home_url(profile))?,
    };
    activate(app, &tab.id)?;
    if !url.is_empty() {
        let target = url::Url::parse(url)?;
        let view = TAB_VIEWS
            .with(|views| views.borrow().get(&tab.id).cloned())
            .ok_or_else(|| AppError("La pestaÃ±a no estÃ¡ disponible.".into()))?;
        view.navigate(target.clone())?;
        update_tab(app, &tab.id, |tab| {
            tab.url = target.to_string();
            tab.loading = true;
            tab.title = tab_title(profile, target.as_str());
        });
        protection::set_current_url(app, profile, target.as_str());
    }
    apply_layout(app)?;
    let _ = app.emit("browser:changed", ());
    let tabs = state.browser.tabs.lock().unwrap_or_else(|e| e.into_inner());
    tabs.iter()
        .find(|candidate| candidate.id == tab.id)
        .cloned()
        .ok_or_else(|| AppError("La pestaÃ±a se cerrÃ³ inesperadamente.".into()))
}

pub fn browser_action(app: &AppHandle, tab_id: Option<String>, action: &str) -> AppResult<()> {
    let state = app.state::<AppState>();
    let target = match tab_id {
        Some(id) => Some(id),
        None => state.browser.active.lock().unwrap_or_else(|e| e.into_inner()).clone(),
    }
    .ok_or_else(|| AppError("No hay una pestaÃ±a activa.".into()))?;
    let view = TAB_VIEWS
        .with(|views| views.borrow().get(&target).cloned())
        .ok_or_else(|| AppError("La pestaÃ±a no estÃ¡ disponible.".into()))?;
    let action = action.to_string();
    view.with_webview(move |platform| {
        if let Ok(core) = unsafe { platform.controller().CoreWebView2() } {
            match action.as_str() {
                "back" => {
                    let _ = unsafe { core.GoBack() };
                }
                "forward" => {
                    let _ = unsafe { core.GoForward() };
                }
                "reload" => {
                    let _ = unsafe { core.Reload() };
                }
                "stop" => {
                    let _ = unsafe { core.Stop() };
                }
                _ => {}
            }
        }
    })?;
    Ok(())
}

/// Elimina datos de navegaciÃ³n. `mode`: `session`, `cache` o `profile`.
pub async fn clear_profile_data(app: &AppHandle, profile: &str, mode: &str) -> AppResult<()> {
    profiles::validate_profile(profile)?;
    if mode == "profile" {
        let profile = profile.to_string();
        return super::on_main(app, move |handle| reset_profile(handle, &profile));
    }
    let kinds = match mode {
        "cache" => COREWEBVIEW2_BROWSING_DATA_KINDS_DISK_CACHE,
        "session" => {
            COREWEBVIEW2_BROWSING_DATA_KINDS_COOKIES
                | COREWEBVIEW2_BROWSING_DATA_KINDS_LOCAL_STORAGE
                | COREWEBVIEW2_BROWSING_DATA_KINDS_INDEXED_DB
                | COREWEBVIEW2_BROWSING_DATA_KINDS_CACHE_STORAGE
                | COREWEBVIEW2_BROWSING_DATA_KINDS_SERVICE_WORKERS
        }
        _ => return fail("Tipo de limpieza no vÃ¡lido."),
    };

    let (sender, receiver) = std::sync::mpsc::channel();
    let handle = app.clone();
    let profile = profile.to_string();
    let profile_key = profile.clone();
    handle
        .clone()
        .run_on_main_thread(move || {
            let state = handle.state::<AppState>();
            let tab_id = {
                let tabs = state.browser.tabs.lock().unwrap_or_else(|e| e.into_inner());
                tabs.iter()
                    .find(|tab| tab.profile == profile)
                    .map(|tab| tab.id.clone())
            };
            let Some(tab_id) = tab_id else {
                let _ = sender.send(Err(AppError("No hay pestaÃ±as para este perfil.".into())));
                return;
            };
            let Some(view) = TAB_VIEWS.with(|views| views.borrow().get(&tab_id).cloned()) else {
                let _ = sender.send(Err(AppError("La pestaÃ±a no estÃ¡ disponible.".into())));
                return;
            };
            let sender_cb = sender.clone();
            let queued = view.with_webview(move |platform| {
                let result = (|| -> AppResult<()> {
                    let core = unsafe { platform.controller().CoreWebView2() }
                        .map_err(|e| AppError(format!("WebView2 no estÃ¡ disponible: {e}")))?;                    let core13: ICoreWebView2_13 = core
                        .cast()
                        .map_err(|e| AppError(format!("Perfil de WebView2 no disponible: {e}")))?;
                    let web_profile = unsafe { core13.Profile() }
                        .map_err(|e| AppError(format!("No se pudo obtener el perfil: {e}")))?;
                    let web_profile: ICoreWebView2Profile2 = web_profile
                        .cast()
                        .map_err(|e| AppError(format!("Perfil de WebView2 no compatible: {e}")))?;
                    let done = sender_cb.clone();
                    let handler = ClearBrowsingDataCompletedHandler::create(Box::new(move |_code| {
                        let _ = done.send(Ok(()));
                        Ok(())
                    }));
                    unsafe {
                        web_profile
                            .ClearBrowsingData(kinds, &handler)
                            .map_err(|e| AppError(format!("No se pudo limpiar la sesiÃ³n: {e}")))?;
                    }
                    Ok(())
                })();
                if let Err(err) = result {
                    let _ = sender_cb.send(Err(err));
                }
            });
            if let Err(err) = queued {
                let _ = sender.send(Err(AppError(format!(
                    "No se pudo acceder a la pestaÃ±a: {err}"
                ))));
            }
        })
        .map_err(|e| AppError(format!("No se pudo programar la limpieza: {e}")))?;

    let result = receiver
        .recv_timeout(std::time::Duration::from_secs(45))
        .map_err(|_| AppError("La limpieza del navegador tardÃ³ demasiado.".into()))?;
    result?;

    let state = app.state::<AppState>();
    state
        .protection
        .blocked
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(profile_key, 0);
    let _ = app.emit("browser:changed", ());
    Ok(())
}

fn reset_profile(app: &AppHandle, profile: &str) -> AppResult<()> {
    let state = app.state::<AppState>();
    let profile = profile.to_string();
    // Cerrar pestaÃ±as del perfil y liberar el entorno antes de borrar archivos.
    let ids: Vec<String> = {
        let mut tabs = state.browser.tabs.lock().unwrap_or_else(|e| e.into_inner());
        let ids: Vec<String> = tabs
            .iter()
            .filter(|tab| tab.profile == profile)
            .map(|tab| tab.id.clone())
            .collect();
        tabs.retain(|tab| tab.profile != profile);
        ids
    };
    for id in &ids {
        if let Some(view) = TAB_VIEWS.with(|views| views.borrow_mut().remove(id)) {
            let _ = view.close();
        }
    }
    ENVIRONMENTS.with(|map| {
        map.borrow_mut().remove(&profile);
    });
    std::thread::sleep(std::time::Duration::from_millis(350));
    profiles::remove_profile_dir(app, &profile)?;
    {
        let mut tabs = state.browser.tabs.lock().unwrap_or_else(|e| e.into_inner());
        let mut active = state.browser.active.lock().unwrap_or_else(|e| e.into_inner());
        if tabs.is_empty() {
            *active = None;
        } else if !tabs
            .iter()
            .any(|tab| active.as_deref() == Some(tab.id.as_str()))
        {
            *active = tabs.first().map(|tab| tab.id.clone());
        }
        let _ = &mut tabs;
    }
    let new_tab = create_tab(app, &profile, profiles::home_url(&profile))?;
    activate(app, &new_tab.id)?;
    apply_layout(app)?;
    let _ = app.emit("browser:changed", ());
    Ok(())
}

/// Borra todas las sesiones (se usa al desactivar Â«Mantener sesiÃ³n iniciadaÂ»).
pub fn purge_all_sessions(app: &AppHandle) -> AppResult<()> {
    let state = app.state::<AppState>();
    close_all_views();
    {
        let mut tabs = state.browser.tabs.lock().unwrap_or_else(|e| e.into_inner());
        tabs.clear();
    }
    *state.browser.active.lock().unwrap_or_else(|e| e.into_inner()) = None;
    profiles::remove_all(app)?;
    state.browser.privacy_purged.store(true, Ordering::SeqCst);
    let _ = app.emit("browser:changed", ());
    Ok(())
}
