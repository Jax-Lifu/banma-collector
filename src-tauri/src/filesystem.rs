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
    let target_dir = resource_target_dir(&output, &request.item, request.separate_languages);
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
        let alternate_dir =
            resource_target_dir(&output, &request.item, !request.separate_languages);
        let mut alternate = alternate_dir.join(safe_filename(&request.item));
        if request.item.extension.eq_ignore_ascii_case("m3u8")
            || request.item.extension.eq_ignore_ascii_case("mpd")
        {
            alternate = alternate.with_extension(if request.item.kind == "audio" {
                "mp3"
            } else {
                "mp4"
            });
        }
        if !alternate.is_file() {
            return Ok(None);
        }
        path = alternate;
    }
    if request.item.kind == "video" && !has_current_playable_marker(&output, &path).await {
        return Ok(None);
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
