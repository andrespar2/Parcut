use crate::db;
use crate::error::{fail, AppResult};
use crate::state::AppState;
use crate::utils::now_iso;
use adblock::lists::{FilterSet, ParseOptions, RuleTypes};
use adblock::request::Request;
use adblock::Engine;
use serde::Serialize;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, Webview, Wry};
use webview2_com::Microsoft::Web::WebView2::Win32::{
    ICoreWebView2Environment, ICoreWebView2WebResourceRequestedEventArgs,
    COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
};
use webview2_com::{CoTaskMemPWSTR, WebResourceRequestedEventHandler};
use windows::core::{HSTRING, PWSTR};

const BASELINE: &str = include_str!("filters/baseline.txt");

struct ListDef {
    id: &'static str,
    name: &'static str,
    url: &'static str,
    license: &'static str,
    intensive_only: bool,
}

const LISTS: [ListDef; 3] = [
    ListDef {
        id: "easylist",
        name: "EasyList",
        url: "https://easylist.to/easylist/easylist.txt",
        license: "CC BY-SA 3.0 / GPLv3",
        intensive_only: false,
    },
    ListDef {
        id: "easyprivacy",
        name: "EasyPrivacy",
        url: "https://easylist.to/easylist/easyprivacy.txt",
        license: "CC BY-SA 3.0 / GPLv3",
        intensive_only: false,
    },
    ListDef {
        id: "peter-lowe",
        name: "Peter Lowe's Ad and tracking server list",
        url: "https://pgl.yoyo.org/adservers/serverlist.php?hostformat=adblockplus&showintro=0&mimetype=plaintext",
        license: "Uso libre con atribución",
        intensive_only: true,
    },
];

const LIST_MAX_AGE_SECS: u64 = 7 * 24 * 3600;

thread_local! {
    static ENGINE: RefCell<Option<Engine>> = RefCell::new(None);
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FilterListInfo {
    pub id: String,
    pub name: String,
    pub url: String,
    pub license: String,
    pub bytes: i64,
    pub updated_at: Option<String>,
    pub intensive_only: bool,
    pub installed: bool,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SiteRule {
    pub profile: String,
    pub host: String,
    pub enabled: bool,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ProtectionStatus {
    pub enabled: bool,
    pub level: String,
    pub engine_ready: bool,
    pub rules: u64,
    pub lists: Vec<FilterListInfo>,
    pub sites: Vec<SiteRule>,
    pub blocked_total: u64,
    pub blocked_by_profile: HashMap<String, u64>,
    pub update_running: bool,
    pub last_update: Option<String>,
    pub last_error: Option<String>,
}

pub struct ProtectionState {
    pub enabled: AtomicBool,
    pub level: Mutex<String>,
    pub blocked: Mutex<HashMap<String, u64>>,
    pub sites: Mutex<HashMap<String, HashMap<String, bool>>>,
    pub current_hosts: Mutex<HashMap<String, String>>,
    pub engine_ready: AtomicBool,
    pub rules: AtomicU64,
    pub building: AtomicBool,
    pub update_running: AtomicBool,
    pub last_update: Mutex<Option<String>>,
    pub last_error: Mutex<Option<String>>,
}

impl ProtectionState {
    pub fn new(enabled: bool, level: String) -> Self {
        Self {
            enabled: AtomicBool::new(enabled),
            level: Mutex::new(level),
            blocked: Mutex::new(HashMap::new()),
            sites: Mutex::new(HashMap::new()),
            current_hosts: Mutex::new(HashMap::new()),
            engine_ready: AtomicBool::new(false),
            rules: AtomicU64::new(0),
            building: AtomicBool::new(false),
            update_running: AtomicBool::new(false),
            last_update: Mutex::new(None),
            last_error: Mutex::new(None),
        }
    }
}

fn lock<'a, T>(mutex: &'a Mutex<T>) -> std::sync::MutexGuard<'a, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn list_dir(app: &AppHandle) -> PathBuf {
    let state = app.state::<AppState>();
    state.paths.root.join("web-protection").join("lists")
}

fn list_path(app: &AppHandle, id: &str) -> PathBuf {
    list_dir(app).join(format!("{id}.txt"))
}

/// Carga la configuración y arranca el motor en segundo plano.
pub fn init(app: &AppHandle) {
    let state = app.state::<AppState>();
    let settings = state.settings_snapshot();
    *lock(&state.protection.level) = settings.web_protection_level.clone();
    state
        .protection
        .enabled
        .store(settings.web_protection_enabled, Ordering::SeqCst);

    if let Ok(sites) = {
        let db = state.db();
        db::list_browser_sites(&db)
    } {
        let mut map = lock(&state.protection.sites);
        for (profile, host, enabled) in sites {
            map.entry(profile).or_default().insert(host, enabled);
        }
    }
    let _ = std::fs::create_dir_all(list_dir(app));
    rebuild_engine(app);
    ensure_lists_fresh(app);
}

pub fn set_global_enabled(app: &AppHandle, enabled: bool) -> AppResult<()> {
    let state = app.state::<AppState>();
    state.protection.enabled.store(enabled, Ordering::SeqCst);
    let mut settings = state.settings_snapshot();
    settings.web_protection_enabled = enabled;
    settings.save(&state.paths.settings_file)?;
    *lock(&state.settings) = settings;
    let _ = app.emit("browser:protection", ());
    Ok(())
}

pub fn set_level(app: &AppHandle, level: &str) -> AppResult<()> {
    if !matches!(level, "standard" | "intensive") {
        return fail("Nivel de protección no válido.");
    }
    let state = app.state::<AppState>();
    *lock(&state.protection.level) = level.to_string();
    let mut settings = state.settings_snapshot();
    settings.web_protection_level = level.to_string();
    settings.save(&state.paths.settings_file)?;
    *lock(&state.settings) = settings;
    rebuild_engine(app);
    let _ = app.emit("browser:protection", ());
    Ok(())
}

pub fn normalize_host(input: &str) -> AppResult<String> {
    let raw = input.trim().to_ascii_lowercase();
    if raw.contains("://") || raw.contains('/') || raw.contains('\\') || raw.contains(' ') {
        return fail("Dominio no válido.");
    }
    let host = raw
        .split(':')
        .next()
        .unwrap_or("")
        .trim_start_matches('.')
        .trim_end_matches('.')
        .to_string();
    let labels: Vec<&str> = host.split('.').collect();
    let is_ipv4 = labels.len() == 4 && labels.iter().all(|label| label.parse::<u8>().is_ok());
    let valid_labels = labels.iter().all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-')
    });
    let valid_tld = labels
        .last()
        .map(|tld| tld.len() >= 2 && tld.chars().all(|c| c.is_ascii_alphabetic()))
        .unwrap_or(false);
    if host.is_empty()
        || host.len() > 255
        || labels.len() < 2
        || !valid_labels
        || (!valid_tld && !is_ipv4)
    {
        return fail("Dominio no válido.");
    }
    Ok(host)
}

pub fn set_site(app: &AppHandle, profile: &str, host: &str, enabled: bool) -> AppResult<()> {
    crate::browser::profiles::validate_profile(profile)?;
    let host = normalize_host(host)?;
    let state = app.state::<AppState>();
    {
        let db = state.db();
        db::set_browser_site(&db, profile, &host, enabled)?;
    }
    lock(&state.protection.sites)
        .entry(profile.to_string())
        .or_default()
        .insert(host, enabled);
    let _ = app.emit("browser:protection", ());
    Ok(())
}

pub fn remove_site(app: &AppHandle, profile: &str, host: &str) -> AppResult<()> {
    crate::browser::profiles::validate_profile(profile)?;
    let host = normalize_host(host)?;
    let state = app.state::<AppState>();
    {
        let db = state.db();
        db::delete_browser_site(&db, profile, &host)?;
    }
    if let Some(map) = lock(&state.protection.sites).get_mut(profile) {
        map.remove(&host);
    }
    let _ = app.emit("browser:protection", ());
    Ok(())
}

pub fn status(app: &AppHandle) -> ProtectionStatus {
    let state = app.state::<AppState>();
    let mut sites = Vec::new();
    for (profile, hosts) in lock(&state.protection.sites).iter() {
        for (host, enabled) in hosts {
            sites.push(SiteRule {
                profile: profile.clone(),
                host: host.clone(),
                enabled: *enabled,
            });
        }
    }
    sites.sort_by(|a, b| a.profile.cmp(&b.profile).then(a.host.cmp(&b.host)));
    let blocked = lock(&state.protection.blocked).clone();
    let blocked_total = blocked.values().sum();
    let mut lists = list_infos(app);
    lists.insert(
        0,
        FilterListInfo {
            id: "baseline".into(),
            name: "ParCut Baseline".into(),
            url: String::new(),
            license: "Propia".into(),
            bytes: BASELINE.len() as i64,
            updated_at: None,
            intensive_only: false,
            installed: true,
        },
    );
    let enabled = state.protection.enabled.load(Ordering::SeqCst);
    let level = lock(&state.protection.level).clone();
    let engine_ready = state.protection.engine_ready.load(Ordering::SeqCst);
    let rules = state.protection.rules.load(Ordering::SeqCst);
    let update_running = state.protection.update_running.load(Ordering::SeqCst);
    let last_update = lock(&state.protection.last_update).clone();
    let last_error = lock(&state.protection.last_error).clone();
    ProtectionStatus {
        enabled,
        level,
        engine_ready,
        rules,
        lists,
        sites,
        blocked_total,
        blocked_by_profile: blocked,
        update_running,
        last_update,
        last_error,
    }
}

fn list_infos(app: &AppHandle) -> Vec<FilterListInfo> {
    LISTS
        .iter()
        .map(|def| {
            let path = list_path(app, def.id);
            let metadata = std::fs::metadata(&path).ok();
            let updated_at = metadata
                .as_ref()
                .and_then(|m| m.modified().ok())
                .and_then(|time| {
                    let datetime: chrono::DateTime<chrono::Utc> = time.into();
                    Some(datetime.to_rfc3339())
                });
            FilterListInfo {
                id: def.id.to_string(),
                name: def.name.to_string(),
                url: def.url.to_string(),
                license: def.license.to_string(),
                bytes: metadata.map(|m| m.len() as i64).unwrap_or(0),
                updated_at,
                intensive_only: def.intensive_only,
                installed: path.is_file(),
            }
        })
        .collect()
}

fn list_stale(path: &Path) -> bool {
    match std::fs::metadata(path).and_then(|m| m.modified()) {
        Ok(time) => time
            .elapsed()
            .map(|elapsed| elapsed.as_secs() > LIST_MAX_AGE_SECS)
            .unwrap_or(true),
        Err(_) => true,
    }
}

pub fn ensure_lists_fresh(app: &AppHandle) {
    let needs_update = LISTS.iter().any(|def| list_stale(&list_path(app, def.id)));
    if needs_update {
        update_lists(app, false);
    }
}

pub fn update_lists(app: &AppHandle, force: bool) {
    let state = app.state::<AppState>();
    if state.protection.update_running.swap(true, Ordering::SeqCst) {
        return;
    }
    let client = state.http.clone();
    let dir = list_dir(app);
    let _ = std::fs::create_dir_all(&dir);
    let app2 = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut errors: Vec<String> = Vec::new();
        for def in LISTS.iter() {
            let path = dir.join(format!("{}.txt", def.id));
            if !force && !list_stale(&path) {
                continue;
            }
            match download_list(&client, def.url).await {
                Ok(text) if text.len() > 500 => {
                    let tmp = path.with_extension("txt.tmp");
                    if let Err(err) = tokio::fs::write(&tmp, text.as_bytes()).await {
                        errors.push(format!("{}: {err}", def.name));
                    } else {
                        let _ = tokio::fs::rename(&tmp, &path).await;
                    }
                }
                Ok(_) => errors.push(format!("{}: respuesta demasiado corta", def.name)),
                Err(err) => errors.push(format!("{}: {err}", def.name)),
            }
        }
        let state = app2.state::<AppState>();
        state.protection.update_running.store(false, Ordering::SeqCst);
        if errors.is_empty() {
            *lock(&state.protection.last_update) = Some(now_iso());
            *lock(&state.protection.last_error) = None;
        } else {
            *lock(&state.protection.last_error) = Some(errors.join(" · "));
        }
        rebuild_engine(&app2);
        let _ = app2.emit("browser:protection", ());
    });
}

async fn download_list(client: &reqwest::Client, url: &str) -> AppResult<String> {
    let response = client
        .get(url)
        .timeout(std::time::Duration::from_secs(120))
        .send()
        .await?;
    if !response.status().is_success() {
        return fail(format!("HTTP {}", response.status().as_u16()));
    }
    Ok(response.text().await?)
}

fn build_engine_bytes(level: &str, dir: &Path) -> Result<(Vec<u8>, u64), String> {
    let mut filter_set = FilterSet::new(false);
    let mut rules: u64 = 0;
    let mut sources: Vec<(String, String)> = vec![("ParCut Baseline".into(), BASELINE.to_string())];
    for def in LISTS.iter() {
        if def.intensive_only && level != "intensive" {
            continue;
        }
        let path = dir.join(format!("{}.txt", def.id));
        if let Ok(text) = std::fs::read_to_string(&path) {
            sources.push((def.name.to_string(), text));
        }
    }
    for (_, text) in sources {
        rules += text
            .lines()
            .filter(|line| {
                let line = line.trim();
                !line.is_empty() && !line.starts_with('!') && !line.starts_with('[')
            })
            .count() as u64;
        filter_set.add_filter_list(
            text,
            ParseOptions {
                rule_types: RuleTypes::NetworkOnly,
                ..Default::default()
            },
        );
    }
    let engine = Engine::new_with_filter_set(filter_set);
    Ok((engine.serialize(), rules))
}

pub fn rebuild_engine(app: &AppHandle) {
    let state = app.state::<AppState>();
    if state.protection.building.swap(true, Ordering::SeqCst) {
        return;
    }
    let level = lock(&state.protection.level).clone();
    let dir = list_dir(app);
    let app2 = app.clone();
    std::thread::spawn(move || {
        let built = build_engine_bytes(&level, &dir);
        let app3 = app2.clone();
        let _ = app2.run_on_main_thread(move || {
            let state = app3.state::<AppState>();
            match built {
                Ok((bytes, rules)) => {
                    let mut engine = Engine::default();
                    match engine.deserialize(&bytes) {
                        Ok(()) => {
                            ENGINE.with(|cell| {
                                *cell.borrow_mut() = Some(engine);
                            });
                            state.protection.engine_ready.store(true, Ordering::SeqCst);
                            state.protection.rules.store(rules, Ordering::SeqCst);
                            *lock(&state.protection.last_error) = None;
                        }
                        Err(err) => {
                            state.protection.engine_ready.store(false, Ordering::SeqCst);
                            *lock(&state.protection.last_error) =
                                Some(format!("No se pudo cargar el motor de protección: {err}"));
                        }
                    }
                }
                Err(err) => {
                    state.protection.engine_ready.store(false, Ordering::SeqCst);
                    *lock(&state.protection.last_error) = Some(err);
                }
            }
            state.protection.building.store(false, Ordering::SeqCst);
            let _ = app3.emit("browser:protection", ());
        });
    });
}

/// Registra el filtro de red y el manejador de WebView2 para un perfil.
pub fn attach(app: &AppHandle, profile: &str, webview: &Webview<Wry>) -> AppResult<()> {
    let app = app.clone();
    let profile = profile.to_string();
    webview.with_webview(move |platform| {
        unsafe {
            let environment = platform.environment();
            let core = match platform.controller().CoreWebView2() {
                Ok(core) => core,
                Err(_) => return,
            };
            if core
                .AddWebResourceRequestedFilter(
                    &HSTRING::from("*"),
                    COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
                )
                .is_err()
            {
                return;
            }
            let handler = WebResourceRequestedEventHandler::create(Box::new(move |_sender, args| {
                if let Some(args) = args {
                    let _ = inspect_request(&app, &profile, &environment, &args);
                }
                Ok(())
            }));
            let mut token = 0i64;
            let _ = core.add_WebResourceRequested(&handler, &mut token);
        }
    })?;
    Ok(())
}

fn inspect_request(
    app: &AppHandle,
    profile: &str,
    environment: &ICoreWebView2Environment,
    args: &ICoreWebView2WebResourceRequestedEventArgs,
) -> windows::core::Result<()> {
    unsafe {
        let state = app.state::<AppState>();
        if !state.protection.enabled.load(Ordering::SeqCst) {
            return Ok(());
        }
        let request = args.Request()?;
        let mut uri_ptr = PWSTR::null();
        request.Uri(&mut uri_ptr)?;
        let uri = CoTaskMemPWSTR::from(uri_ptr).to_string();
        if uri.is_empty() {
            return Ok(());
        }
        let mut method_ptr = PWSTR::null();
        request.Method(&mut method_ptr)?;
        let method = CoTaskMemPWSTR::from(method_ptr).to_string();
        let headers = request.Headers()?;
        let destination = read_header(&headers, "Sec-Fetch-Dest");
        let fetch_mode = read_header(&headers, "Sec-Fetch-Mode");
        let referer = read_header(&headers, "Referer");
        let current = lock(&state.protection.current_hosts)
            .get(profile)
            .cloned()
            .unwrap_or_default();

        if !should_block(
            &state,
            profile,
            &uri,
            if referer.is_empty() {
                &current
            } else {
                &referer
            },
            &destination,
            &fetch_mode,
            if method.is_empty() { "GET" } else { &method },
        ) {
            return Ok(());
        }

        let response = environment.CreateWebResourceResponse(
            None,
            403,
            &HSTRING::from("Blocked by ParCut"),
            &HSTRING::from("Content-Type: text/plain; charset=UTF-8"),
        )?;
        let _ = args.SetResponse(&response);
        let mut blocked = lock(&state.protection.blocked);
        *blocked.entry(profile.to_string()).or_insert(0) += 1;
        Ok(())
    }
}

fn read_header(
    headers: &webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2HttpRequestHeaders,
    name: &str,
) -> String {
    let mut value = PWSTR::null();
    if unsafe { headers.GetHeader(&HSTRING::from(name), &mut value) }.is_ok() {
        CoTaskMemPWSTR::from(value).to_string()
    } else {
        String::new()
    }
}

pub fn should_block(
    state: &AppState,
    profile: &str,
    url: &str,
    source_url: &str,
    destination: &str,
    fetch_mode: &str,
    method: &str,
) -> bool {
    if !state.protection.enabled.load(Ordering::SeqCst) {
        return false;
    }
    let dest = destination.to_ascii_lowercase();
    if dest == "document" || dest == "iframe" || fetch_mode.eq_ignore_ascii_case("navigate") {
        return false;
    }
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return false;
    }
    if is_auth_url(url) {
        return false;
    }
    let current_host = lock(&state.protection.current_hosts)
        .get(profile)
        .cloned()
        .unwrap_or_default();
    if !site_allows(&state.protection.sites, profile, &current_host) {
        return false;
    }
    let request_type = match dest.as_str() {
        "script" => "script",
        "stylesheet" | "style" => "stylesheet",
        "image" | "imageset" => "image",
        "font" => "font",
        "media" => "media",
        "iframe" | "frame" => "subdocument",
        "worker" | "sharedworker" => "script",
        "empty" => "xhr",
        _ => "other",
    };
    let request = match Request::new(url, source_url, request_type, method) {
        Ok(request) => request,
        Err(_) => return false,
    };
    ENGINE.with(|cell| {
        cell.borrow()
            .as_ref()
            .map(|engine| engine.check_network_request(&request).should_block())
            .unwrap_or(false)
    })
}

pub fn site_allows(
    sites: &Mutex<HashMap<String, HashMap<String, bool>>>,
    profile: &str,
    host: &str,
) -> bool {
    if host.is_empty() {
        return true;
    }
    let host = host.to_ascii_lowercase();
    let sites = lock(sites);
    let Some(profile_sites) = sites.get(profile) else {
        return true;
    };
    let mut candidate = host.as_str();
    loop {
        if let Some(enabled) = profile_sites.get(candidate) {
            return *enabled;
        }
        match candidate.split_once('.') {
            Some((_, rest)) if !rest.is_empty() => candidate = rest,
            _ => break,
        }
    }
    true
}

fn is_auth_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    let host = lower
        .split("://")
        .nth(1)
        .and_then(|rest| rest.split('/').next())
        .unwrap_or("");
    if host.starts_with("accounts.google.")
        || host.starts_with("login.")
        || host == "accounts.youtube.com"
    {
        return true;
    }
    let path = lower.split("://").nth(1).unwrap_or("");
    path.contains("/login")
        || path.contains("/signin")
        || path.contains("/sign-in")
        || path.contains("/oauth")
        || path.contains("/checkpoint")
        || path.contains("/auth")
        || path.contains("/captcha")
        || path.contains("/challenge")
}

pub fn set_current_url(app: &AppHandle, profile: &str, url: &str) {
    let state = app.state::<AppState>();
    let host = url::Url::parse(url)
        .ok()
        .and_then(|parsed| parsed.host_str().map(|host| host.to_string()))
        .unwrap_or_default();
    lock(&state.protection.current_hosts).insert(profile.to_string(), host);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine() -> Engine {
        let mut set = FilterSet::new(false);
        set.add_filter_list(
            format!("{BASELINE}\n||tracker.test^\n@@||allowed.test^$document\n"),
            ParseOptions {
                rule_types: RuleTypes::NetworkOnly,
                ..Default::default()
            },
        );
        Engine::new_with_filter_set(set)
    }

    #[test]
    fn baseline_blocks_known_ad_hosts() {
        let engine = engine();
        let request = Request::new(
            "https://doubleclick.net/pixel.js",
            "https://www.youtube.com/",
            "script",
            "GET",
        )
        .unwrap();
        assert!(engine.check_network_request(&request).should_block());
    }

    #[test]
    fn baseline_allows_platform_documents() {
        let engine = engine();
        let request = Request::new(
            "https://www.youtube.com/watch?v=1",
            "https://www.youtube.com/",
            "document",
            "GET",
        )
        .unwrap();
        assert!(!engine.check_network_request(&request).should_block());
    }

    #[test]
    fn site_exceptions_match_subdomains() {
        let mut map: HashMap<String, HashMap<String, bool>> = HashMap::new();
        map.entry("youtube".into()).or_default().insert("youtube.com".into(), false);
        let sites = Mutex::new(map);
        assert!(!site_allows(&sites, "youtube", "www.youtube.com"));
        assert!(!site_allows(&sites, "youtube", "studio.youtube.com"));
        assert!(site_allows(&sites, "facebook", "www.youtube.com"));
        assert!(site_allows(&sites, "youtube", "example.com"));
    }

    #[test]
    fn auth_urls_are_bypassed() {
        assert!(is_auth_url("https://accounts.google.com/o/oauth2/v2/auth?x=1"));
        assert!(is_auth_url("https://www.facebook.com/login/?next=/"));
        assert!(!is_auth_url("https://www.youtube.com/watch?v=1"));
    }

    #[test]
    fn host_normalization_is_strict() {
        assert_eq!(normalize_host("  WWW.Example.COM:443 ").unwrap(), "www.example.com");
        assert_eq!(normalize_host("127.0.0.1").unwrap(), "127.0.0.1");
        assert!(normalize_host("https://example.com").is_err());
        assert!(normalize_host("example").is_err());
        assert!(normalize_host("").is_err());
    }
}
