use crate::utils::normalize_hashtags;
use serde::Serialize;

/// Campos finales que se enviarán a la plataforma, ya adaptados a sus límites.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishFields {
    pub title: String,
    pub description: String,
    pub hashtags: Vec<String>,
    pub warnings: Vec<String>,
}

/// Título por defecto: "<NOMBRE DEL VIDEO> - PARTE N".
pub fn default_title(project_name: &str, part: i64) -> String {
    let name = project_name.trim();
    let name = if name.is_empty() { "Video" } else { name };
    format!("{name} - PARTE {part}")
}

fn truncate_chars(input: &str, max: usize) -> String {
    if input.chars().count() <= max {
        return input.to_string();
    }
    let mut out: String = input.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// Une descripción libre con los hashtags, sin duplicarlos.
pub fn compose_caption(description: &str, hashtags: &[String]) -> String {
    let mut caption = description.trim().to_string();
    for tag in hashtags {
        let marker = format!("#{tag}");
        if !caption.to_lowercase().contains(&marker.to_lowercase()) {
            if !caption.is_empty() {
                caption.push(' ');
            }
            caption.push_str(&marker);
        }
    }
    caption
}

/// Aplica los límites reales de cada plataforma.
///
/// - YouTube: título 100 caracteres, descripción 5000, tags sin '#'.
/// - Facebook: título 255, descripción 63 206.
/// - TikTok: pie de foto 2200 caracteres (título + descripción + hashtags).
pub fn apply_limits(
    platform: &str,
    raw_title: &str,
    raw_description: &str,
    raw_hashtags: &[String],
    part: i64,
    project_name: &str,
) -> PublishFields {
    let mut warnings = Vec::new();
    let hashtags = normalize_hashtags(raw_hashtags);
    let mut title = raw_title.trim().to_string();
    if title.is_empty() {
        title = default_title(project_name, part);
    }
    let description = raw_description.trim().to_string();

    match platform {
        "youtube" => {
            if title.chars().count() > 100 {
                warnings.push("El título de YouTube se recortó a 100 caracteres.".into());
            }
            let title = truncate_chars(&title, 100);
            let mut caption = compose_caption(&description, &hashtags);
            if caption.chars().count() > 5000 {
                warnings.push("La descripción de YouTube se recortó a 5000 caracteres.".into());
                caption = truncate_chars(&caption, 5000);
            }
            let weighted: Vec<String> = hashtags
                .into_iter()
                .filter(|t| {
                    if t.chars().count() > 100 {
                        warnings.push(format!("El hashtag #{t} supera el límite de 100 caracteres."));
                        false
                    } else {
                        true
                    }
                })
                .collect();
            PublishFields {
                title,
                description: caption,
                hashtags: weighted,
                warnings,
            }
        }
        "facebook" => {
            if title.chars().count() > 255 {
                warnings.push("El título de Facebook se recortó a 255 caracteres.".into());
            }
            let title = truncate_chars(&title, 255);
            let mut caption = compose_caption(&description, &hashtags);
            if caption.chars().count() > 63_206 {
                warnings.push("La descripción de Facebook se recortó a 63 206 caracteres.".into());
                caption = truncate_chars(&caption, 63_206);
            }
            PublishFields {
                title,
                description: caption,
                hashtags,
                warnings,
            }
        }
        _ => {
            let caption = compose_caption(&title, &[description.clone()]);
            let caption = if caption.is_empty() {
                compose_caption(&default_title(project_name, part), &hashtags)
            } else {
                compose_caption(&description, &hashtags)
            };
            let mut limited = truncate_chars(&caption, 2200);
            if limited != caption {
                warnings.push("El texto de TikTok se recortó a 2200 caracteres.".into());
            }
            // TikTok no tiene campo separado de descripción.
            let mut kept_tags = hashtags;
            while compose_caption("", &kept_tags).chars().count() > 2200 && !kept_tags.is_empty() {
                kept_tags.pop();
            }
            limited = truncate_chars(&compose_caption(&description, &kept_tags), 2200);
            PublishFields {
                title: limited,
                description: String::new(),
                hashtags: kept_tags,
                warnings,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_title_matches_required_format() {
        assert_eq!(default_title("Mi viaje por Colombia", 1), "Mi viaje por Colombia - PARTE 1");
        assert_eq!(default_title("  ", 3), "Video - PARTE 3");
    }

    #[test]
    fn hashtags_are_appended_once() {
        let caption = compose_caption("Hola #viaje", &["viaje".into(), "colombia".into()]);
        assert_eq!(caption, "Hola #viaje #colombia");
    }

    #[test]
    fn youtube_title_is_limited() {
        let title = "x".repeat(150);
        let fields = apply_limits("youtube", &title, "", &[], 1, "Demo");
        assert_eq!(fields.title.chars().count(), 100);
        assert!(!fields.warnings.is_empty());
    }

    #[test]
    fn tiktok_merges_title_and_hashtags() {
        let fields = apply_limits("tiktok", "Mi clip", "Hola", &["viaje".into()], 2, "Demo");
        assert!(fields.title.starts_with("Hola"));
        assert!(fields.title.contains("#viaje"));
        assert!(fields.description.is_empty());
    }

    #[test]
    fn empty_title_falls_back_to_part_number() {
        let fields = apply_limits("facebook", "   ", "", &[], 4, "Serie");
        assert_eq!(fields.title, "Serie - PARTE 4");
    }
}
