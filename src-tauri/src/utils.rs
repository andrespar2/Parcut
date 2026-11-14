use crate::error::{fail, AppResult};
use chrono::{SecondsFormat, Utc};
use std::path::{Path, PathBuf};

/// Marca de tiempo ISO-8601 en UTC.
pub fn now_iso() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

/// Identificador único (UUID v4).
pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Limpia un nombre para usarlo como archivo en Windows.
pub fn sanitize_filename(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        let forbidden = matches!(ch, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') || ch.is_control();
        if forbidden {
            out.push('_');
        } else {
            out.push(ch);
        }
    }
    let trimmed = out.trim().trim_matches('.').trim().to_string();
    let mut result: String = trimmed.chars().take(120).collect();
    if result.is_empty() {
        result = "video".into();
    }
    // Nombres reservados de Windows.
    let upper = result.to_uppercase();
    let reserved = ["CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "LPT1", "LPT2", "LPT3"];
    if reserved.contains(&upper.as_str()) {
        result = format!("_{result}");
    }
    result
}

/// Valida que una URL sea http(s) y no apunte a la red local.
pub fn validate_remote_url(raw: &str) -> AppResult<url::Url> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return fail("Introduce una URL de video.");
    }
    let parsed = url::Url::parse(trimmed).map_err(|_| {
        crate::error::AppError("La URL no es válida. Debe comenzar por http:// o https://".into())
    })?;
    match parsed.scheme() {
        "http" | "https" => {}
        _ => return fail("Solo se admiten enlaces http o https."),
    }
    let host = parsed
        .host_str()
        .ok_or_else(|| crate::error::AppError("La URL no tiene un dominio válido.".into()))?
        .to_ascii_lowercase();
    let private = host == "localhost"
        || host.ends_with(".localhost")
        || host.ends_with(".local")
        || host == "0.0.0.0"
        || host == "[::1]"
        || host.starts_with("127.")
        || host.starts_with("10.")
        || host.starts_with("192.168.")
        || (host.starts_with("169.254.") )
        || host
            .strip_prefix("172.")
            .and_then(|rest| rest.split('.').next())
            .and_then(|octet| octet.parse::<u8>().ok())
            .map(|o| (16..=31).contains(&o))
            .unwrap_or(false);
    if private {
        return fail("No se permiten direcciones de red local en las descargas.");
    }
    Ok(parsed)
}

/// Identifica la plataforma a partir del dominio, sin prometer compatibilidad.
pub fn detect_platform(url: &str) -> Option<&'static str> {
    let host = url::Url::parse(url).ok()?.host_str()?.to_ascii_lowercase();
    if host.ends_with("youtube.com") || host == "youtu.be" {
        Some("YouTube")
    } else if host.ends_with("facebook.com") || host == "fb.watch" {
        Some("Facebook")
    } else if host.ends_with("instagram.com") {
        Some("Instagram")
    } else if host.ends_with("tiktok.com") {
        Some("TikTok")
    } else if host.ends_with("vimeo.com") {
        Some("Vimeo")
    } else if host.ends_with("twitter.com") || host == "x.com" {
        Some("X")
    } else {
        None
    }
}

/// Normaliza una lista de hashtags: sin '#', sin duplicados, sin espacios.
pub fn normalize_hashtags(tags: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for tag in tags {
        for piece in tag.split(['#', ',', ';', ' ']) {
            let clean: String = piece
                .chars()
                .filter(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if clean.is_empty() {
                continue;
            }
            if clean.chars().count() > 60 {
                continue;
            }
            if !out.iter().any(|t| t.eq_ignore_ascii_case(&clean)) {
                out.push(clean);
            }
            if out.len() >= 30 {
                return out;
            }
        }
    }
    out
}

/// Comprueba (sin tocar el disco) que `target` queda dentro de `base`.
pub fn is_within(base: &Path, target: &Path) -> bool {
    let base_norm = normalize_path(base);
    let target_norm = normalize_path(target);
    target_norm.starts_with(&base_norm)
}

fn normalize_path(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    let text = out.to_string_lossy().replace('/', "\\");
    PathBuf::from(text.to_lowercase())
}

/// Canonicaliza y garantiza que la ruta esté dentro del directorio administrado.
pub fn ensure_within(base: &Path, target: &Path) -> AppResult<PathBuf> {
    let base_canon = base
        .canonicalize()
        .map_err(|_| crate::error::AppError("El directorio de trabajo de ParCut no existe.".into()))?;
    let resolved = if target.exists() {
        target.canonicalize()?
    } else {
        let parent = target
            .parent()
            .ok_or_else(|| crate::error::AppError("Ruta de archivo no válida.".into()))?;
        let parent_canon = if parent.exists() {
            parent.canonicalize()?
        } else {
            return fail("La carpeta destino no existe dentro del espacio de ParCut.");
        };
        match target.file_name() {
            Some(name) => parent_canon.join(name),
            None => parent_canon,
        }
    };
    if !is_within(&base_canon, &resolved) && resolved != base_canon {
        return fail("Operación bloqueada: la ruta está fuera del espacio de trabajo de ParCut.");
    }
    Ok(resolved)
}

/// Tamaño total de una carpeta, sin seguir enlaces simbólicos.
pub fn dir_size(path: &Path) -> u64 {
    let mut total = 0u64;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let p = entry.path();
            match entry.file_type() {
                Ok(ft) if ft.is_dir() => total += dir_size(&p),
                Ok(ft) if ft.is_file() => {
                    total += entry.metadata().map(|m| m.len()).unwrap_or(0);
                }
                _ => {}
            }
        }
    }
    total
}

/// Borra una carpeta con reintentos y limpieza de atributos de solo lectura.
pub fn remove_dir_all_robust(path: &Path) -> AppResult<(u64, u64)> {
    if !path.exists() {
        return Ok((0, 0));
    }
    let bytes = dir_size(path);
    let mut files = 0u64;
    if let Ok(entries) = walk_files(path) {
        files = entries.len() as u64;
    }
    let mut last_err: Option<std::io::Error> = None;
    for attempt in 0..3 {
        match std::fs::remove_dir_all(path) {
            Ok(()) => {
                last_err = None;
                break;
            }
            Err(err) => {
                last_err = Some(err);
                for file in walk_files(path).unwrap_or_default() {
                    if let Ok(metadata) = std::fs::metadata(&file) {
                        let mut perms = metadata.permissions();
                        if perms.readonly() {
                            perms.set_readonly(false);
                            let _ = std::fs::set_permissions(&file, perms);
                        }
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(150 * (attempt + 1)));
            }
        }
    }
    if let Some(err) = last_err {
        return Err(crate::error::AppError(format!(
            "No se pudieron eliminar todos los archivos: {err}"
        )));
    }
    Ok((bytes, files))
}

fn walk_files(root: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    stack.push(p);
                } else {
                    out.push(p);
                }
            }
        }
    }
    Ok(out)
}

/// Extensión de video aceptada en importaciones locales.
pub const VIDEO_EXTENSIONS: &[&str] = &[
    "mp4", "mov", "mkv", "webm", "avi", "m4v", "mpg", "mpeg", "wmv", "flv", "ts", "mts", "m2ts",
];

pub fn is_supported_video(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| VIDEO_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_removes_windows_forbidden_chars() {
        assert_eq!(sanitize_filename("mi:video?/parte*1"), "mi_video__parte_1");
        assert_eq!(sanitize_filename("  con  "), "_con");
        assert_eq!(sanitize_filename(""), "video");
    }

    #[test]
    fn hashtags_are_deduped_and_cleaned() {
        let tags = normalize_hashtags(&["#Viaje, #colombia #viaje #Colombia  #2026!".to_string()]);
        assert_eq!(tags, vec!["Viaje", "colombia", "2026"]);
    }

    #[test]
    fn private_hosts_are_rejected() {
        assert!(validate_remote_url("http://192.168.1.5/video").is_err());
        assert!(validate_remote_url("http://localhost:8080/a").is_err());
        assert!(validate_remote_url("file:///C:/video.mp4").is_err());
        assert!(validate_remote_url("https://www.youtube.com/watch?v=abc").is_ok());
    }

    #[test]
    fn platform_detection_is_conservative() {
        assert_eq!(
            detect_platform("https://youtu.be/abc"),
            Some("YouTube")
        );
        assert_eq!(detect_platform("https://example.com/x"), None);
    }

    #[test]
    fn path_containment_blocks_traversal() {
        let base = std::env::temp_dir().join("parcut-test-base");
        std::fs::create_dir_all(&base).unwrap();
        assert!(is_within(&base, &base.join("projects").join("x")));
        assert!(!is_within(&base, &base.join("..").join("otro")));
        let _ = std::fs::remove_dir_all(&base);
    }
}
