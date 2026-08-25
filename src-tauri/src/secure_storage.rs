use super::*;

fn credential_path(product: &str) -> Result<PathBuf, String> {
    let mut directory = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .ok_or_else(|| "无法定位 Windows 应用数据目录".to_string())?;
    directory.push("BanmaCollector");
    directory.push("credentials");
    std::fs::create_dir_all(&directory).map_err(|error| format!("无法创建凭据目录：{error}"))?;
    directory.push(format!("{product}.bin"));
    Ok(directory)
}

#[cfg(windows)]
fn protect(data: &[u8]) -> Result<Vec<u8>, String> {
    use std::ptr::{null, null_mut};
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{
        CryptProtectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };

    let input = CRYPT_INTEGER_BLOB {
        cbData: u32::try_from(data.len()).map_err(|_| "会话数据过大".to_string())?,
        pbData: data.as_ptr().cast_mut(),
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: null_mut(),
    };
    let succeeded = unsafe {
        CryptProtectData(
            &input,
            null(),
            null(),
            null(),
            null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if succeeded == 0 {
        return Err(format!(
            "Windows 会话加密失败：{}",
            std::io::Error::last_os_error()
        ));
    }
    let encrypted = unsafe {
        let bytes = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        LocalFree(output.pbData.cast());
        bytes
    };
    Ok(encrypted)
}

#[cfg(windows)]
fn unprotect(data: &[u8]) -> Result<Vec<u8>, String> {
    use std::ptr::{null, null_mut};
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{
        CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };

    let input = CRYPT_INTEGER_BLOB {
        cbData: u32::try_from(data.len()).map_err(|_| "会话数据过大".to_string())?,
        pbData: data.as_ptr().cast_mut(),
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: null_mut(),
    };
    let succeeded = unsafe {
        CryptUnprotectData(
            &input,
            null_mut(),
            null(),
            null(),
            null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if succeeded == 0 {
        return Err(format!(
            "Windows 会话解密失败：{}",
            std::io::Error::last_os_error()
        ));
    }
    let decrypted = unsafe {
        let bytes = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        LocalFree(output.pbData.cast());
        bytes
    };
    Ok(decrypted)
}

#[cfg(not(windows))]
fn protect(_data: &[u8]) -> Result<Vec<u8>, String> {
    Err("安全会话存储目前仅支持 Windows".into())
}

#[cfg(not(windows))]
fn unprotect(_data: &[u8]) -> Result<Vec<u8>, String> {
    Err("安全会话存储目前仅支持 Windows".into())
}

pub(crate) fn save_cookie(product: &str, cookie: &str) -> Result<(), String> {
    let encrypted = protect(cookie.as_bytes())?;
    std::fs::write(credential_path(product)?, encrypted)
        .map_err(|error| format!("无法保存加密会话：{error}"))
}

pub(crate) fn load_cookie(product: &str) -> Result<Option<String>, String> {
    let path = credential_path(product)?;
    if !path.is_file() {
        return Ok(None);
    }
    let encrypted = std::fs::read(path).map_err(|error| format!("无法读取加密会话：{error}"))?;
    let decrypted = unprotect(&encrypted)?;
    String::from_utf8(decrypted)
        .map(Some)
        .map_err(|_| "加密会话内容无效".to_string())
}

pub(crate) fn delete_cookie(product: &str) -> Result<(), String> {
    let path = credential_path(product)?;
    if path.exists() {
        std::fs::remove_file(path).map_err(|error| format!("无法删除加密会话：{error}"))?;
    }
    Ok(())
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn dpapi_round_trip_is_bound_to_current_windows_user() {
        let original = b"session-cookie-with-sensitive-value";
        let encrypted = protect(original).expect("protect session");
        assert_ne!(encrypted, original);
        assert_eq!(unprotect(&encrypted).expect("unprotect session"), original);
    }
}
