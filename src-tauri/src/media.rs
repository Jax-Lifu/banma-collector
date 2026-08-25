use super::*;
use aes::cipher::{BlockDecrypt, KeyInit};
use aes::Aes128;

pub(crate) const PLAYABLE_MARKER_VERSION: &[u8] = b"playable-v2";

pub(crate) fn safe_folder_name(name: &str) -> String {
    let invalid = Regex::new(r#"[<>:\"/\\|?*\x00-\x1f]"#).expect("valid regex");
    invalid
        .replace_all(name, "_")
        .trim_matches(['.', ' '])
        .to_string()
}

pub(crate) fn safe_subfolder_path(name: &str) -> PathBuf {
    name.split('/')
        .map(safe_folder_name)
        .filter(|part| !part.is_empty() && part != "." && part != "..")
        .fold(PathBuf::new(), |path, part| path.join(part))
}

pub(crate) fn safe_filename(item: &ResourceItem) -> String {
    let extension = if item.extension.eq_ignore_ascii_case("m3u8")
        || item.extension.eq_ignore_ascii_case("mpd")
    {
        if item.kind == "audio" {
            "mp3"
        } else {
            "mp4"
        }
    } else if item.extension.is_empty()
        || item.extension.len() > 6
        || !item.extension.chars().all(|c| c.is_ascii_alphanumeric())
    {
        match item.kind.as_str() {
            "audio" => "mp3",
            "video" => "mp4",
            "image" => "png",
            "document" => "pdf",
            "data" => "json",
            _ => "bin",
        }
    } else {
        &item.extension
    };
    let fallback = format!("{}.{}", item.id, extension);
    let invalid = Regex::new(r#"[<>:\"/\\|?*\x00-\x1f]"#).expect("valid regex");
    let cleaned = invalid
        .replace_all(&item.title, "_")
        .trim_matches(['.', ' '])
        .to_string();
    // 接口有时把 UUID.mp4 直接放在 title 中。先去掉已有扩展名，避免生成
    // `xxx.mp4_hash.mp4` 这类会误导系统播放器和用户的双扩展名。
    let cleaned = Path::new(&cleaned)
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|_| {
            Path::new(&cleaned)
                .extension()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.eq_ignore_ascii_case(extension))
        })
        .unwrap_or(&cleaned)
        .to_string();
    let prefix = item
        .sequence
        .map(|sequence| format!("{sequence:03}_"))
        .unwrap_or_default();
    let quality = item
        .quality
        .as_deref()
        .map(safe_folder_name)
        .filter(|value| !value.is_empty())
        .map(|value| format!("{value}_"))
        .unwrap_or_default();
    let language = item
        .language
        .as_deref()
        .map(safe_folder_name)
        .filter(|value| !value.is_empty())
        .map(|value| format!("{value}_"))
        .unwrap_or_default();
    if cleaned.is_empty() {
        format!("{prefix}{language}{quality}{fallback}")
    } else {
        format!(
            "{prefix}{cleaned}_{language}{quality}{}.{}",
            item.id, extension
        )
    }
}

pub(crate) fn legacy_filename(item: &ResourceItem) -> String {
    let mut legacy = item.clone();
    legacy.sequence = None;
    safe_filename(&legacy)
}

const MEDIA_KEY_MASK: &str = "5c77002799f2cfcc78897c3b34ce2e85";

fn unwrap_media_key(kid: &str, encrypted_key: &str) -> Result<String, String> {
    let kid: String = kid
        .chars()
        .filter(|value| value.is_ascii_hexdigit())
        .collect();
    let kid = hex::decode(&kid).map_err(|_| "视频 KID 格式无效".to_string())?;
    let encrypted_key = hex::decode(encrypted_key).map_err(|_| "视频密钥格式无效".to_string())?;
    let mask = hex::decode(MEDIA_KEY_MASK).expect("valid media key mask");
    if kid.len() != 16 || encrypted_key.len() != 16 {
        return Err("视频密钥长度无效".into());
    }

    // 斑马密钥接口返回的是包装后的 16 字节密文。原生播放器先用
    // KID 与客户端掩码异或得到包装密钥，再做一次 AES-128-ECB 解密。
    let wrapping_key = kid
        .iter()
        .zip(mask.iter())
        .map(|(kid_byte, mask_byte)| kid_byte ^ mask_byte)
        .collect::<Vec<_>>();
    let cipher = Aes128::new_from_slice(&wrapping_key)
        .map_err(|_| "无法初始化视频密钥解密器".to_string())?;
    let mut block = aes::Block::clone_from_slice(&encrypted_key);
    cipher.decrypt_block(&mut block);
    Ok(hex::encode(block))
}

struct MediaKeyCandidates {
    native: String,
    endpoint: String,
}

async fn fetch_media_key_candidates(
    client: &reqwest::Client,
    kid: &str,
) -> Result<MediaKeyCandidates, String> {
    debug_log!("media key request kid={}", kid);
    let value: serde_json::Value = client
        .get(format!(
            "{MEDIA_KEY_HOST}/live-maple-openapi/api/resources/key"
        ))
        .query(&[("kid", kid)])
        .send()
        .await
        .map_err(|error| format!("获取视频解密密钥失败：{error}"))?
        .error_for_status()
        .map_err(|error| format!("视频密钥接口失败：{error}"))?
        .json()
        .await
        .map_err(|error| format!("视频密钥响应无效：{error}"))?;
    let key = value
        .get("key")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    if key.len() != 32 || !key.chars().all(|value| value.is_ascii_hexdigit()) {
        return Err(format!("视频资源 {kid} 没有可用的解密密钥"));
    }
    let media_key = unwrap_media_key(kid, key)?;
    debug_log!("media key unwrapped kid={} status=success", kid);
    Ok(MediaKeyCandidates {
        native: media_key,
        endpoint: key.to_ascii_lowercase(),
    })
}

async fn fetch_media_key(client: &reqwest::Client, kid: &str) -> Result<String, String> {
    Ok(fetch_media_key_candidates(client, kid).await?.native)
}

fn bento4_tool(name: &str) -> PathBuf {
    let bundled = crate::runtime_tools::command(name);
    if bundled.is_absolute() {
        return bundled;
    }
    let suffix = if cfg!(windows) { ".exe" } else { "" };
    let tool_name = format!("{name}{suffix}");
    let dump_name = format!("mp4dump{suffix}");
    let decrypt_name = format!("mp4decrypt{suffix}");
    let mut directories = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .unwrap_or_default();

    if let Some(root) = std::env::var_os("SCOOP") {
        directories.push(PathBuf::from(root).join("apps/bento4/current/bin"));
    }
    if let Some(global) = std::env::var_os("SCOOP_GLOBAL") {
        let global = PathBuf::from(global);
        if let Some(root) = global.parent() {
            directories.push(root.join("apps/bento4/current/bin"));
        }
        directories.push(global.join("apps/bento4/current/bin"));
    }
    if let Some(profile) = std::env::var_os("USERPROFILE") {
        directories.push(PathBuf::from(profile).join("scoop/apps/bento4/current/bin"));
    }

    // 必须同时存在 mp4dump 和 mp4decrypt，防止命中同名的 Python mp4dump 包。
    directories
        .into_iter()
        .find(|directory| {
            directory.join(&dump_name).is_file() && directory.join(&decrypt_name).is_file()
        })
        .map(|directory| directory.join(tool_name))
        .unwrap_or_else(|| PathBuf::from(name))
}

async fn inspect_cenc_kids(path: &Path) -> Result<Vec<String>, String> {
    let mut command = Command::new(bento4_tool("mp4dump"));
    command
        .args(["--verbosity", "1"])
        .arg(path)
        .kill_on_drop(true);
    let output = command.output().await.map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            "斑马正式课程视频需要 Bento4，请安装 Bento4 并确保 mp4dump、mp4decrypt 已加入 PATH"
                .to_string()
        } else {
            format!("检查视频加密信息失败：{error}")
        }
    })?;
    if !output.status.success() {
        let detail = command_error_detail(&output.stderr);
        return Err(format!("无法读取 MP4 加密信息，文件可能下载不完整{detail}"));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut kids = Vec::new();
    for line in text
        .lines()
        .filter(|line| line.to_ascii_lowercase().contains("default_kid"))
    {
        let value = line.split_once('=').map(|(_, value)| value).unwrap_or(line);
        let kid: String = value
            .chars()
            .filter(|value| value.is_ascii_hexdigit())
            .collect();
        if kid.len() >= 32 {
            let kid = kid[..32].to_ascii_lowercase();
            if !kids.contains(&kid) {
                kids.push(kid);
            }
        }
    }
    Ok(kids)
}

async fn bento_compatible_input(path: &Path) -> Result<(PathBuf, bool), String> {
    if path.to_string_lossy().is_ascii() {
        return Ok((path.to_path_buf(), false));
    }

    // Bento4 的 Windows 工具无法稳定打开中文路径。使用同盘 ASCII 路径的
    // 硬链接进行检测和解密，不复制视频内容，也不会占用双倍磁盘空间。
    let staging_base = path
        .ancestors()
        .skip(1)
        .find(|ancestor| !ancestor.as_os_str().is_empty() && ancestor.to_string_lossy().is_ascii())
        .ok_or_else(|| format!("无法找到兼容的视频暂存目录：{}", path.display()))?;
    let staging_dir = staging_base.join(".banma-media-staging");
    fs::create_dir_all(&staging_dir)
        .await
        .map_err(|error| format!("创建视频解密暂存目录失败：{error}"))?;
    let digest = hex::encode(Sha256::digest(path.to_string_lossy().as_bytes()));
    let staging_path = staging_dir.join(format!("{}.input.mp4", &digest[..20]));
    let _ = fs::remove_file(&staging_path).await;
    if let Err(_link_error) = fs::hard_link(path, &staging_path).await {
        debug_log!(
            "bento staging hard-link failed input={} error={}, fallback=copy",
            path.display(),
            _link_error
        );
        fs::copy(path, &staging_path)
            .await
            .map_err(|error| format!("暂存视频以兼容解密工具失败：{error}"))?;
    }
    debug_log!(
        "bento staging input={} staged={}",
        path.display(),
        staging_path.display()
    );
    Ok((staging_path, true))
}

pub(crate) fn command_error_detail(stderr: &[u8]) -> String {
    let text = String::from_utf8_lossy(stderr);
    let compact = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .rev()
        .take(8)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join(" | ");
    if compact.is_empty() {
        String::new()
    } else {
        format!("：{compact}")
    }
}

async fn decrypt_mp4_with_keys(
    input_path: &Path,
    output_path: &Path,
    keys: &[(String, String)],
) -> Result<(), String> {
    let _ = fs::remove_file(output_path).await;
    let mut command = Command::new(bento4_tool("mp4decrypt"));
    command.kill_on_drop(true);
    for (kid, key) in keys {
        command.args(["--key", &format!("{kid}:{key}")]);
    }
    let output = command
        .arg(input_path)
        .arg(output_path)
        .output()
        .await
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                "斑马正式课程视频需要 Bento4，请安装 Bento4 并确保 mp4decrypt 已加入 PATH"
                    .to_string()
            } else {
                format!("启动视频解密工具失败：{error}")
            }
        })?;
    if !output.status.success() {
        let _ = fs::remove_file(output_path).await;
        return Err(format!(
            "斑马视频 CENC 解密失败{}",
            command_error_detail(&output.stderr)
        ));
    }
    let size = fs::metadata(output_path)
        .await
        .map(|value| value.len())
        .unwrap_or(0);
    if size < 32 * 1024 {
        let _ = fs::remove_file(output_path).await;
        return Err("斑马视频解密结果不完整".into());
    }
    Ok(())
}

async fn decrypt_mp4_with_ffmpeg(
    input_path: &Path,
    output_path: &Path,
    key: &str,
) -> Result<(), String> {
    let _ = fs::remove_file(output_path).await;
    let mut command = Command::new(crate::runtime_tools::command("ffmpeg"));
    command
        .args([
            "-nostdin",
            "-y",
            "-v",
            "error",
            "-decryption_key",
            key,
            "-i",
        ])
        .arg(input_path)
        .args([
            "-map",
            "0:v:0",
            "-map",
            "0:a?",
            "-c",
            "copy",
            "-movflags",
            "+faststart",
        ])
        .arg(output_path)
        .stdout(Stdio::null())
        .kill_on_drop(true);
    let output = tokio::time::timeout(std::time::Duration::from_secs(20 * 60), command.output())
        .await
        .map_err(|_| "FFmpeg 解密直链 MP4 超过 20 分钟".to_string())?
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                "解密视频需要安装 ffmpeg，并确保 ffmpeg 已加入 PATH".to_string()
            } else {
                format!("启动 FFmpeg 视频解密失败：{error}")
            }
        })?;
    if !output.status.success() {
        let _ = fs::remove_file(output_path).await;
        return Err(format!(
            "FFmpeg 视频解密失败{}",
            command_error_detail(&output.stderr)
        ));
    }
    verify_playable_video(output_path).await
}

async fn decrypt_mp4_candidate(
    input_path: &Path,
    output_path: &Path,
    keys: &[(String, String)],
) -> Result<(), String> {
    if keys.len() == 1 {
        // Bento4 对部分带大量 subsample encryption 表的直链 MP4 会生成损坏码流，
        // FFmpeg 的 MOV demuxer 可正确处理。多 KID 文件才回退 Bento4。
        return decrypt_mp4_with_ffmpeg(input_path, output_path, &keys[0].1).await;
    }
    decrypt_mp4_with_keys(input_path, output_path, keys).await?;
    verify_playable_video(output_path).await
}

pub(crate) async fn decrypt_mp4_if_needed(
    client: &reqwest::Client,
    path: &Path,
) -> Result<Option<PathBuf>, String> {
    let (inspection_path, staged) = bento_compatible_input(path).await?;
    let kids = match inspect_cenc_kids(&inspection_path).await {
        Ok(kids) => kids,
        Err(_error) => {
            if staged {
                let _ = fs::remove_file(&inspection_path).await;
            }
            // Bento4 不支持的普通 MP4 继续交给 ffmpeg；真实损坏会在标准化或
            // 30 帧解码校验中得到更准确的错误，不能在加密探测阶段全部误杀。
            debug_log!(
                "cenc inspect unavailable path={} reason={}, fallback=ffmpeg",
                path.display(),
                _error
            );
            return Ok(None);
        }
    };
    if kids.is_empty() {
        if staged {
            let _ = fs::remove_file(&inspection_path).await;
        }
        debug_log!("cenc inspect path={} encrypted=false", path.display());
        return Ok(None);
    }
    debug_log!(
        "cenc inspect path={} encrypted=true kids={:?}",
        path.display(),
        kids
    );

    let output_path = inspection_path.with_extension("decrypting.mp4");
    let mut native_keys = Vec::new();
    let mut endpoint_keys = Vec::new();
    for kid in &kids {
        let candidates = match fetch_media_key_candidates(client, kid).await {
            Ok(candidates) => candidates,
            Err(error) => {
                if staged {
                    let _ = fs::remove_file(&inspection_path).await;
                }
                return Err(error);
            }
        };
        native_keys.push((kid.clone(), candidates.native));
        endpoint_keys.push((kid.clone(), candidates.endpoint));
    }

    let native_validation =
        decrypt_mp4_candidate(&inspection_path, &output_path, &native_keys).await;
    let result = match native_validation {
        Ok(()) => {
            debug_log!(
                "cenc key strategy=native path={} status=success",
                path.display()
            );
            Ok(())
        }
        Err(_native_error) if endpoint_keys != native_keys => {
            debug_log!(
                "cenc key strategy=native path={} status=failed reason={}, fallback=endpoint",
                path.display(),
                _native_error
            );
            decrypt_mp4_candidate(&inspection_path, &output_path, &endpoint_keys)
                .await
                .map(|()| {
                    debug_log!(
                        "cenc key strategy=endpoint path={} status=success",
                        path.display()
                    );
                })
                .map_err(|error| format!("视频使用两种密钥策略解密后仍无法播放：{error}"))
        }
        Err(error) => Err(error),
    };
    if staged {
        let _ = fs::remove_file(&inspection_path).await;
    }
    if let Err(error) = result {
        let _ = fs::remove_file(&output_path).await;
        return Err(error);
    }
    let _size = fs::metadata(&output_path)
        .await
        .map(|value| value.len())
        .unwrap_or(0);
    debug_log!(
        "cenc decrypt complete input={} output={} bytes={}",
        path.display(),
        output_path.display(),
        _size
    );
    Ok(Some(output_path))
}

pub(crate) async fn verify_playable_video(path: &Path) -> Result<(), String> {
    debug_log!("decode verify start path={}", path.display());
    let mut command = Command::new(crate::runtime_tools::command("ffmpeg"));
    command
        .args(["-nostdin", "-v", "error", "-xerror", "-i"])
        .arg(path)
        .args(["-map", "0:v:0", "-frames:v", "30", "-f", "null", "-"])
        .stdout(Stdio::null())
        .kill_on_drop(true);
    let output = tokio::time::timeout(std::time::Duration::from_secs(45), command.output())
        .await
        .map_err(|_| "视频解码校验超时，文件可能不完整".to_string())?
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                "校验下载视频需要安装 ffmpeg，并确保 ffmpeg 已加入 PATH".to_string()
            } else {
                format!("启动视频校验失败：{error}")
            }
        })?;
    if !output.status.success() {
        return Err(format!(
            "视频解码校验失败，未将损坏文件标记为下载成功{}",
            command_error_detail(&output.stderr)
        ));
    }
    debug_log!("decode verify complete path={}", path.display());
    Ok(())
}

async fn normalize_mp4(path: &Path) -> Result<(), String> {
    debug_log!("mp4 normalize start path={}", path.display());
    let output_path = path.with_extension("normalizing.mp4");
    let _ = fs::remove_file(&output_path).await;
    let mut command = Command::new(crate::runtime_tools::command("ffmpeg"));
    command
        .args(["-nostdin", "-y", "-v", "error", "-i"])
        .arg(path)
        .args([
            "-map",
            "0:v:0",
            "-map",
            "0:a?",
            "-c",
            "copy",
            "-movflags",
            "+faststart",
            "-avoid_negative_ts",
            "make_zero",
        ])
        .arg(&output_path)
        .stdout(Stdio::null())
        .kill_on_drop(true);
    let output = tokio::time::timeout(std::time::Duration::from_secs(120), command.output())
        .await
        .map_err(|_| "MP4 标准化封装超时".to_string())?
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                "处理下载视频需要安装 ffmpeg，并确保 ffmpeg 已加入 PATH".to_string()
            } else {
                format!("启动 MP4 标准化失败：{error}")
            }
        })?;
    if !output.status.success() {
        let _ = fs::remove_file(&output_path).await;
        return Err(format!(
            "MP4 标准化封装失败，源文件可能不完整{}",
            command_error_detail(&output.stderr)
        ));
    }
    let size = fs::metadata(&output_path)
        .await
        .map(|value| value.len())
        .unwrap_or(0);
    if size < 32 * 1024 {
        let _ = fs::remove_file(&output_path).await;
        return Err("MP4 标准化结果不完整".into());
    }
    verify_playable_video(&output_path).await.inspect_err(|_| {
        let _ = std::fs::remove_file(&output_path);
    })?;
    fs::remove_file(path)
        .await
        .map_err(|error| format!("替换旧视频失败：{error}"))?;
    fs::rename(&output_path, path)
        .await
        .map_err(|error| format!("保存标准 MP4 失败：{error}"))?;
    debug_log!(
        "mp4 normalize complete path={} bytes={}",
        path.display(),
        size
    );
    Ok(())
}

pub(crate) async fn prepare_playable_video(
    client: &reqwest::Client,
    path: &Path,
) -> Result<(), String> {
    if let Some(decrypted_path) = decrypt_mp4_if_needed(client, path).await? {
        if let Err(error) = normalize_mp4(&decrypted_path).await {
            let _ = fs::remove_file(&decrypted_path).await;
            return Err(error);
        }
        fs::remove_file(path)
            .await
            .map_err(|error| format!("替换加密视频失败：{error}"))?;
        fs::rename(&decrypted_path, path)
            .await
            .map_err(|error| format!("保存解密视频失败：{error}"))?;
        Ok(())
    } else {
        normalize_mp4(path).await
    }
}

pub(crate) async fn has_current_playable_marker(path: &Path) -> bool {
    fs::read(path)
        .await
        .is_ok_and(|value| value == PLAYABLE_MARKER_VERSION)
}

pub(crate) async fn write_playable_marker(path: &Path) -> Result<(), String> {
    fs::write(path, PLAYABLE_MARKER_VERSION)
        .await
        .map_err(|error| format!("写入视频校验标记失败：{error}"))
}

pub(crate) async fn manifest_decryption_context(
    client: &reqwest::Client,
    url: &str,
) -> Result<(Option<String>, Option<usize>), String> {
    debug_log!("dash manifest request url={}", url);
    let response = client
        .get(url)
        .header("Referer", "https://conan.yuanfudao.com/")
        .send()
        .await
        .map_err(|error| format!("读取 DASH 清单失败：{error}"))?
        .error_for_status()
        .map_err(|error| format!("DASH 清单请求失败：{error}"))?;
    let manifest = response
        .text()
        .await
        .map_err(|error| format!("读取 DASH 清单失败：{error}"))?;
    let regex = Regex::new(r#"(?i)default_KID\s*=\s*["']([0-9a-f-]{32,36})["']"#)
        .map_err(|error| error.to_string())?;
    let key = if let Some(found) = regex
        .captures(&manifest)
        .and_then(|captures| captures.get(1))
    {
        let kid: String = found
            .as_str()
            .chars()
            .filter(|value| value.is_ascii_hexdigit())
            .collect();
        Some(fetch_media_key(client, &kid).await?)
    } else {
        None
    };

    // ffmpeg 会把同一 MPD 里的 HEVC 轨道排在 AVC 前面。Windows 默认播放器
    // 往往未安装 HEVC 扩展，因此记录 AVC 在视频轨道中的序号，下载时显式映射。
    let representation =
        Regex::new(r#"(?i)<Representation\b([^>]*)>"#).map_err(|error| error.to_string())?;
    let codecs =
        Regex::new(r#"(?i)\bcodecs\s*=\s*["']([^"']+)["']"#).map_err(|error| error.to_string())?;
    let mime_type = Regex::new(r#"(?i)\bmimeType\s*=\s*["']([^"']+)["']"#)
        .map_err(|error| error.to_string())?;
    let mut video_stream_index = 0usize;
    let mut avc_stream_index = None;
    for attributes in representation
        .captures_iter(&manifest)
        .filter_map(|captures| captures.get(1).map(|value| value.as_str()))
    {
        let codec = codecs
            .captures(attributes)
            .and_then(|captures| captures.get(1))
            .map(|value| value.as_str().to_ascii_lowercase())
            .unwrap_or_default();
        let mime = mime_type
            .captures(attributes)
            .and_then(|captures| captures.get(1))
            .map(|value| value.as_str().to_ascii_lowercase())
            .unwrap_or_default();
        let is_video = mime.starts_with("video/")
            || ["avc1", "avc3", "hev1", "hvc1", "vp9", "av01"]
                .iter()
                .any(|prefix| codec.starts_with(prefix));
        if !is_video {
            continue;
        }
        if avc_stream_index.is_none() && (codec.starts_with("avc1") || codec.starts_with("avc3")) {
            avc_stream_index = Some(video_stream_index);
        }
        video_stream_index += 1;
    }

    debug_log!(
        "dash manifest parsed encrypted={} avc_stream={:?}",
        key.is_some(),
        avc_stream_index
    );
    Ok((key, avc_stream_index))
}

#[cfg(test)]
mod tests;
