use super::*;

#[tauri::command]
pub(crate) fn reveal_path(app: tauri::AppHandle, path: String) -> Result<(), String> {
    app.opener()
        .open_path(path, None::<&str>)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) async fn resource_preview_path(
    app: tauri::AppHandle,
    request: PreviewPathRequest,
) -> Result<Option<String>, String> {
    let output = std::fs::canonicalize(PathBuf::from(request.output_dir))
        .map_err(|error| format!("无法访问下载目录：{error}"))?;
    let target_dir = request
        .item
        .subfolder
        .as_deref()
        .map(safe_subfolder_path)
        .filter(|path| !path.as_os_str().is_empty())
        .map(|path| output.join(path))
        .unwrap_or_else(|| output.clone());
    let mut path = target_dir.join(safe_filename(&request.item));
    if request.item.extension.eq_ignore_ascii_case("m3u8")
        || request.item.extension.eq_ignore_ascii_case("mpd")
    {
        path = path.with_extension(if request.item.kind == "audio" {
            "mp3"
        } else {
            "mp4"
        });
    }
    if !path.is_file() {
        return Ok(None);
    }
    if request.item.kind == "video" {
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("mp4");
        let verified = path.with_extension(format!("{extension}.playable"));
        if !verified.is_file() {
            return Ok(None);
        }
    }
    let path =
        std::fs::canonicalize(&path).map_err(|error| format!("无法访问预览文件：{error}"))?;
    if !path.starts_with(&output) {
        return Err("预览文件不在当前下载目录中".into());
    }
    app.asset_protocol_scope()
        .allow_file(&path)
        .map_err(|error| format!("无法授权本地预览文件：{error}"))?;
    Ok(Some(path.to_string_lossy().into_owned()))
}
