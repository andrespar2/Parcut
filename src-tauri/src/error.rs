use serde::{Serialize, Serializer};

/// Error de dominio de ParCut. Se serializa como string legible para la UI.
#[derive(Debug, Clone)]
pub struct AppError(pub String);

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for AppError {}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl From<String> for AppError {
    fn from(value: String) -> Self {
        AppError(value)
    }
}

impl From<&str> for AppError {
    fn from(value: &str) -> Self {
        AppError(value.to_string())
    }
}

impl From<std::io::Error> for AppError {
    fn from(value: std::io::Error) -> Self {
        AppError(format!("Error de archivo: {value}"))
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(value: rusqlite::Error) -> Self {
        AppError(format!("Error de base de datos: {value}"))
    }
}

impl From<serde_json::Error> for AppError {
    fn from(value: serde_json::Error) -> Self {
        AppError(format!("Error de datos: {value}"))
    }
}

impl From<reqwest::Error> for AppError {
    fn from(value: reqwest::Error) -> Self {
        if value.is_timeout() {
            AppError("La conexión con el servidor agotó el tiempo de espera.".into())
        } else if value.is_connect() {
            AppError("No se pudo conectar con el servidor. Revisa tu conexión a internet.".into())
        } else {
            AppError(format!("Error de red: {value}"))
        }
    }
}

impl From<tauri::Error> for AppError {
    fn from(value: tauri::Error) -> Self {
        AppError(format!("Error de la aplicación: {value}"))
    }
}

impl From<url::ParseError> for AppError {
    fn from(value: url::ParseError) -> Self {
        AppError(format!("URL no válida: {value}"))
    }
}

impl From<tokio::task::JoinError> for AppError {
    fn from(value: tokio::task::JoinError) -> Self {
        AppError(format!("Tarea interrumpida: {value}"))
    }
}

pub type AppResult<T> = Result<T, AppError>;

/// Azúcar para construir errores rápidamente.
pub fn fail<T>(message: impl Into<String>) -> AppResult<T> {
    Err(AppError(message.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_serializes_as_string() {
        let e = AppError("algo falló".into());
        let json = serde_json::to_string(&e).unwrap();
        assert_eq!(json, "\"algo falló\"");
    }
}
