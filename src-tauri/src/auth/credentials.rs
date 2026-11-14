use crate::error::{AppError, AppResult};
use base64::Engine;
use serde::de::DeserializeOwned;
use serde::Serialize;

#[cfg(windows)]
fn protect_bytes(data: &[u8]) -> AppResult<Vec<u8>> {
    use windows::Win32::Foundation::{LocalFree, HLOCAL};
    use windows::Win32::Security::Cryptography::{
        CryptProtectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };

    let input = CRYPT_INTEGER_BLOB {
        cbData: data.len() as u32,
        pbData: data.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    unsafe {
        CryptProtectData(
            &input,
            windows::core::PCWSTR::null(),
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
        .map_err(|e| AppError(format!("No se pudo cifrar el secreto con Windows DPAPI: {e}")))?;
        let slice = std::slice::from_raw_parts(output.pbData, output.cbData as usize);
        let result = slice.to_vec();
        let _ = LocalFree(Some(HLOCAL(output.pbData as *mut core::ffi::c_void)));
        Ok(result)
    }
}

#[cfg(windows)]
fn unprotect_bytes(data: &[u8]) -> AppResult<Vec<u8>> {
    use windows::Win32::Foundation::{LocalFree, HLOCAL};
    use windows::Win32::Security::Cryptography::{
        CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };

    let input = CRYPT_INTEGER_BLOB {
        cbData: data.len() as u32,
        pbData: data.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    unsafe {
        CryptUnprotectData(
            &input,
            None,
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
        .map_err(|e| {
            AppError(format!(
                "No se pudo descifrar el secreto. Es posible que se guardara con otra cuenta de Windows: {e}"
            ))
        })?;
        let slice = std::slice::from_raw_parts(output.pbData, output.cbData as usize);
        let result = slice.to_vec();
        let _ = LocalFree(Some(HLOCAL(output.pbData as *mut core::ffi::c_void)));
        Ok(result)
    }
}

#[cfg(not(windows))]
fn protect_bytes(data: &[u8]) -> AppResult<Vec<u8>> {
    Ok(data.to_vec())
}

#[cfg(not(windows))]
fn unprotect_bytes(data: &[u8]) -> AppResult<Vec<u8>> {
    Ok(data.to_vec())
}

/// Cifra un valor serializable con DPAPI (ámbito del usuario actual).
pub fn encrypt_json<T: Serialize>(value: &T) -> AppResult<Vec<u8>> {
    let json = serde_json::to_vec(value)?;
    protect_bytes(&json)
}

/// Descifra un blob DPAPI y lo deserializa.
pub fn decrypt_json<T: DeserializeOwned>(blob: &[u8]) -> AppResult<T> {
    let json = unprotect_bytes(blob)?;
    serde_json::from_slice(&json).map_err(|e| {
        AppError(format!(
            "El contenido cifrado no tiene el formato esperado: {e}"
        ))
    })
}

/// Codifica en base64url sin relleno (para PKCE).
pub fn b64url(bytes: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct Payload {
        token: String,
        expires: i64,
    }

    #[cfg(windows)]
    #[test]
    fn dpapi_roundtrip() {
        let payload = Payload {
            token: "secreto-de-prueba".into(),
            expires: 123,
        };
        let blob = encrypt_json(&payload).unwrap();
        assert_ne!(blob, b"secreto-de-prueba");
        let restored: Payload = decrypt_json(&blob).unwrap();
        assert_eq!(payload, restored);
    }

    #[test]
    fn base64url_has_no_padding() {
        let encoded = b64url(&[1, 2, 3, 4, 5]);
        assert!(!encoded.contains('='));
        assert!(!encoded.contains('+'));
        assert!(!encoded.contains('/'));
    }
}
