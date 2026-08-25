use super::*;
use crate::download_cancel::{wait_for_cancellation, DownloadCancellation};

async fn prepare_video_cancellable(
    client: &reqwest::Client,
    path: &Path,
    cancellation: &DownloadCancellation,
) -> Result<bool, String> {
    tokio::select! {
        result = prepare_playable_video(client, path) => result.map(|_| true),
        _ = wait_for_cancellation(cancellation) => Ok(false),
    }
}

async fn migrate_download_location(
    output_root: &Path,
    old_path: &Path,
    new_path: &Path,
) -> Result<(), String> {
    if new_path.is_file() || !old_path.is_file() || old_path == new_path {
        return Ok(());
    }
    if let Some(parent) = new_path.parent() {
        fs::create_dir_all(parent)
            .await
            .map_err(|error| format!("创建语言分类目录失败：{error}"))?;
    }
    fs::rename(old_path, new_path)
        .await
        .map_err(|error| format!("迁移已有下载到语言分类目录失败：{error}"))?;
    move_playable_marker(output_root, old_path, new_path).await;
    Ok(())
}

#[tauri::command]
pub(crate) async fn download_resources(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    request: DownloadRequest,
) -> Result<(), String> {
    let output = PathBuf::from(request.output_dir);
    debug_log!(
        "batch start product={} items={} concurrency={} output={}",
        request.product,
        request.items.len(),
        request.concurrency,
        output.display()
    );
    fs::create_dir_all(&output)
        .await
        .map_err(|e| format!("无法创建输出目录：{e}"))?;
    let semaphore = Arc::new(Semaphore::new(request.concurrency.clamp(1, 12)));
    let client = client_from(&state, &request.product)?;
    let generation = state.download_generation.clone();
    let batch_generation = generation.load(Ordering::Acquire);
    let separate_languages = request.separate_languages;
    let mut handles = Vec::new();

    let mut item_flags = HashMap::new();
    {
        if let Ok(mut cancellations) = state.download_cancellations.lock() {
            for item in &request.items {
                let flag = Arc::new(AtomicBool::new(false));
                cancellations.insert(item.id.clone(), flag.clone());
                item_flags.insert(item.id.clone(), flag);
            }
        }
    }

    for item in request.items {
        let semaphore = semaphore.clone();
        let client = client.clone();
        let app = app.clone();
        let output = output.clone();
        let cancellation = DownloadCancellation {
            item_flag: item_flags
                .get(&item.id)
                .cloned()
                .unwrap_or_else(|| Arc::new(AtomicBool::new(false))),
            generation: generation.clone(),
            batch_generation,
        };

        handles.push(tokio::spawn(async move {
            let emit = |status: &str, received, total, error| {
                let _ = app.emit("download-progress", DownloadProgress {
                    id: item.id.clone(),
                    status: status.into(),
                    received,
                    total,
                    error,
                });
            };
            let _permit = tokio::select! {
                permit = semaphore.acquire_owned() => match permit {
                    Ok(permit) => permit,
                    Err(error) => return Some(error.to_string()),
                },
                _ = wait_for_cancellation(&cancellation) => {
                    emit("cancelled", 0, None, None);
                    return None;
                }
            };
            debug_log!(
                "item start id={} title={:?} kind={} extension={} subfolder={:?}",
                item.id,
                item.title,
                item.kind,
                item.extension,
                item.subfolder
            );

            let result: Result<(), String> = async {
                if cancellation.is_cancelled() {
                    emit("cancelled", 0, None, None);
                    return Ok(());
                }

                let target_dir = resource_target_dir(
                    &output,
                    &item,
                    separate_languages,
                );
                fs::create_dir_all(&target_dir).await.map_err(|e| format!("创建专辑目录失败：{e}"))?;
                let final_path = target_dir.join(safe_filename(&item));
                let alternate_path = resource_target_dir(
                    &output,
                    &item,
                    !separate_languages,
                )
                .join(safe_filename(&item));
                migrate_download_location(&output, &alternate_path, &final_path).await?;
                debug_log!("item target id={} path={}", item.id, final_path.display());

                // 升级旧版无序号文件名，避免用户为了获得正确排序再次下载同一资源。
                if item.sequence.is_some() && !final_path.exists() {
                    let legacy_path = target_dir.join(legacy_filename(&item));
                    if legacy_path.is_file() {
                        fs::rename(&legacy_path, &final_path).await.map_err(|error| format!("升级剧集文件名失败：{error}"))?;
                        move_playable_marker(&output, &legacy_path, &final_path).await;
                    }
                }

                let is_stream_manifest = item.extension.eq_ignore_ascii_case("m3u8")
                    || item.extension.eq_ignore_ascii_case("mpd");

                // 1. 智能秒传/跳过。清单型资源必须排除历史上误存成 mp4 的几 KB 文本文件。
                if !is_stream_manifest && final_path.exists() {
                    if let Ok(metadata) = fs::metadata(&final_path).await {
                        if metadata.len() > 0 {
                            if item.kind == "video" && final_path.extension().and_then(|value| value.to_str()).is_some_and(|value| value.eq_ignore_ascii_case("mp4")) {
                                if !has_current_playable_marker(&output, &final_path).await {
                                    remove_playable_marker(&output, &final_path).await;
                                    if prepare_video_cancellable(&client, &final_path, &cancellation)
                                        .await
                                        .is_ok_and(|completed| completed)
                                    {
                                        write_playable_marker(&output, &final_path).await?;
                                    } else if cancellation.is_cancelled() {
                                        emit("cancelled", 0, None, None);
                                        return Ok(());
                                    } else {
                                        // 旧版本可能给损坏或未正确解密的视频写过成功标记。
                                        // 删除它并进入正常下载流程，避免每次重试都命中同一坏文件。
                                        let _ = fs::remove_file(&final_path).await;
                                    }
                                }
                                if !final_path.exists() {
                                    // 文件已被判定无效，继续从源地址重新下载。
                                } else {
                                    let size = fs::metadata(&final_path).await.map(|value| value.len()).unwrap_or(metadata.len());
                                    emit("completed", size, Some(size), None);
                                    return Ok(());
                                }
                            } else {
                                let size = metadata.len();
                                emit("completed", size, Some(size), None);
                                return Ok(());
                            }
                        }
                    }
                }

                // 2. 处理 HLS/DASH 流媒体清单，由 ffmpeg 拉取所有分片并封装为可播放文件。
                if is_stream_manifest {
                    let out_ext = if item.kind == "audio" { "mp3" } else { "mp4" };
                    let final_media_path = final_path.with_extension(out_ext);
                    let alternate_media_path = alternate_path.with_extension(out_ext);
                    migrate_download_location(
                        &output,
                        &alternate_media_path,
                        &final_media_path,
                    )
                    .await?;
                    if final_media_path.exists() {
                        if let Ok(metadata) = fs::metadata(&final_media_path).await {
                            // 旧版本曾把 MPD/M3U8 清单文本直接改名为 mp4；这类文件通常只有几 KB。
                            if metadata.len() >= 32 * 1024 && has_current_playable_marker(&output, &final_media_path).await {
                                let size = metadata.len();
                                debug_log!("stream cache hit id={} bytes={}", item.id, size);
                                emit("completed", size, Some(size), None);
                                return Ok(());
                            }
                        }
                        let _ = fs::remove_file(&final_media_path).await;
                    }

                    let partial = final_media_path.with_extension(format!("part.{}", out_ext));
                    let (dash_key, dash_avc_stream_index) = if item.extension.eq_ignore_ascii_case("mpd") {
                        tokio::select! {
                            result = manifest_decryption_context(&client, &item.url) => result?,
                            _ = wait_for_cancellation(&cancellation) => {
                                emit("cancelled", 0, None, None);
                                return Ok(());
                            }
                        }
                    } else {
                        (None, None)
                    };
                    debug_log!(
                        "manifest ready id={} type={} encrypted={} avc_stream={:?}",
                        item.id,
                        item.extension,
                        dash_key.is_some(),
                        dash_avc_stream_index
                    );
                    let mut last_stream_error = String::new();
                    let mut stream_completed = false;

                    for attempt in 1..=3 {
                        if cancellation.is_cancelled() {
                            let _ = fs::remove_file(&partial).await;
                            emit("cancelled", 0, None, None);
                            return Ok(());
                        }
                        let _ = fs::remove_file(&partial).await;
                        emit("downloading", 0, None, None);
                        debug_log!(
                            "stream attempt id={} attempt={}/3 partial={}",
                            item.id,
                            attempt,
                            partial.display()
                        );

                        let mut cmd = Command::new(crate::runtime_tools::command("ffmpeg"));
                        crate::runtime_tools::hide_window(&mut cmd);
                        cmd.args([
                            "-nostdin", "-y", "-hide_banner", "-loglevel", "error",
                            "-rw_timeout", "15000000",
                            "-reconnect", "1", "-reconnect_on_network_error", "1",
                            "-reconnect_streamed", "1", "-reconnect_delay_max", "5",
                            "-reconnect_max_retries", "10", "-reconnect_delay_total_max", "60",
                            "-user_agent", "ZebraAndroid/1.0",
                            "-referer", "https://conan.yuanfudao.com/",
                        ]);
                        if let Some(key) = dash_key.as_ref() {
                            // DASH demuxer uses cenc_decryption_key; decryption_key is a MOV
                            // demuxer option and makes ffmpeg reject an MPD before opening it.
                            cmd.args(["-cenc_decryption_key", key]);
                        }
                        cmd.arg("-i").arg(&item.url);
                        if item.kind == "audio" {
                            cmd.args(["-map", "0:a:0?", "-c:a", "copy"]);
                        } else {
                            if let Some(index) = dash_avc_stream_index {
                                cmd.args(["-map", &format!("0:v:{index}"), "-map", "0:a:0?"]);
                            }
                            cmd.args(["-c", "copy", "-movflags", "+faststart"]);
                        }
                        cmd.arg(&partial)
                            .stdout(Stdio::null())
                            .stderr(Stdio::piped());
                        cmd.kill_on_drop(true);

                        let mut child = cmd.spawn().map_err(|error| {
                            if error.kind() == std::io::ErrorKind::NotFound {
                                "下载 HLS/DASH 媒体需要安装 ffmpeg，并确保 ffmpeg 已加入 PATH".to_string()
                            } else {
                                format!("启动 ffmpeg 失败: {error}")
                            }
                        })?;
                        let mut stderr = child.stderr.take();
                        let stderr_task = tokio::spawn(async move {
                            let mut bytes = Vec::new();
                            if let Some(ref mut pipe) = stderr {
                                let _ = pipe.read_to_end(&mut bytes).await;
                            }
                            bytes
                        });
                        let started = std::time::Instant::now();
                        let mut attempt_ok = false;

                        loop {
                            if cancellation.is_cancelled() {
                                let _ = child.kill().await;
                                let _ = child.wait().await;
                                let _ = fs::remove_file(&partial).await;
                                emit("cancelled", 0, None, None);
                                return Ok(());
                            }
                            if started.elapsed() >= std::time::Duration::from_secs(20 * 60) {
                                let _ = child.kill().await;
                                let _ = child.wait().await;
                                last_stream_error = format!("第 {attempt} 次下载超过 20 分钟");
                                break;
                            }
                            tokio::select! {
                                res = child.wait() => {
                                    let status = res.map_err(|e| e.to_string())?;
                                    debug_log!("ffmpeg exit id={} attempt={} status={}", item.id, attempt, status);
                                    if status.success() {
                                        attempt_ok = true;
                                    } else {
                                        last_stream_error = format!("第 {attempt} 次下载时 CDN 连接中断");
                                    }
                                    break;
                                }
                                _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {
                                    let received = fs::metadata(&partial).await.ok().map(|value| value.len()).unwrap_or(0);
                                    emit("downloading", received, None, None);
                                }
                            }
                        }

                        let stderr_bytes = stderr_task.await.unwrap_or_default();
                        if !attempt_ok {
                            let detail = command_error_detail(&stderr_bytes);
                            debug_log!(
                                "stream attempt failed id={} attempt={} reason={}{}",
                                item.id,
                                attempt,
                                last_stream_error,
                                detail
                            );
                            if !detail.is_empty() {
                                last_stream_error.push_str(&detail);
                            }
                        }

                        if attempt_ok {
                            let size = fs::metadata(&partial).await.ok().map(|value| value.len()).unwrap_or(0);
                            if size >= 32 * 1024 {
                                debug_log!("stream attempt complete id={} attempt={} bytes={}", item.id, attempt, size);
                                stream_completed = true;
                                break;
                            }
                            last_stream_error = format!("第 {attempt} 次下载结果不完整（仅 {size} 字节）");
                        }

                        if attempt < 3 {
                            tokio::select! {
                                _ = tokio::time::sleep(std::time::Duration::from_secs(attempt * 2)) => {},
                                _ = wait_for_cancellation(&cancellation) => {
                                    emit("cancelled", 0, None, None);
                                    return Ok(());
                                }
                            }
                        }
                    }

                    if !stream_completed {
                        let _ = fs::remove_file(&partial).await;
                        return Err(format!("HLS/DASH 下载重试 3 次仍失败：{last_stream_error}"));
                    }

                    if final_media_path.exists() { let _ = fs::remove_file(&final_media_path).await; }
                    fs::rename(&partial, &final_media_path).await.map_err(|e| e.to_string())?;
                    if item.kind == "video" {
                        debug_log!("video prepare start id={} path={}", item.id, final_media_path.display());
                        let prepared = prepare_video_cancellable(
                            &client,
                            &final_media_path,
                            &cancellation,
                        )
                        .await?;
                        if !prepared {
                            let _ = fs::remove_file(&final_media_path).await;
                            remove_playable_marker(&output, &final_media_path).await;
                            emit("cancelled", 0, None, None);
                            return Ok(());
                        }
                        if cancellation.is_cancelled() {
                            let _ = fs::remove_file(&final_media_path).await;
                            remove_playable_marker(&output, &final_media_path).await;
                            emit("cancelled", 0, None, None);
                            return Ok(());
                        }
                        write_playable_marker(&output, &final_media_path).await?;
                        debug_log!("video prepare complete id={}", item.id);
                    }
                    let size = fs::metadata(&final_media_path).await.ok().map(|value| value.len()).unwrap_or(0);
                    debug_log!("item complete id={} bytes={} path={}", item.id, size, final_media_path.display());
                    emit("completed", size, Some(size), None);
                    return Ok(());
                }

                // 3. HTTP 文件 Range 断点续传与失败自动重试机制
                let is_direct_mp4_video = item.kind == "video"
                    && final_path
                        .extension()
                        .and_then(|value| value.to_str())
                        .is_some_and(|value| value.eq_ignore_ascii_case("mp4"));
                let source_cache = target_dir
                    .join(".banma-source-cache")
                    .join(format!("{}.mp4", item.id));
                if is_direct_mp4_video && source_cache.is_file() {
                    debug_log!(
                        "video source cache hit id={} path={}",
                        item.id,
                        source_cache.display()
                    );
                    let _ = fs::remove_file(&final_path).await;
                    fs::rename(&source_cache, &final_path)
                        .await
                        .map_err(|error| format!("恢复视频源文件缓存失败：{error}"))?;
                    match prepare_video_cancellable(&client, &final_path, &cancellation).await {
                        Ok(true) => {
                            write_playable_marker(&output, &final_path).await?;
                            let size = fs::metadata(&final_path)
                                .await
                                .map(|value| value.len())
                                .unwrap_or(0);
                            emit("completed", size, Some(size), None);
                            return Ok(());
                        }
                        Ok(false) => {
                            let _ = fs::create_dir_all(source_cache.parent().unwrap_or(&target_dir)).await;
                            let _ = fs::rename(&final_path, &source_cache).await;
                            emit("cancelled", 0, None, None);
                            return Ok(());
                        }
                        Err(error) => {
                            let _ = fs::create_dir_all(source_cache.parent().unwrap_or(&target_dir)).await;
                            let _ = fs::rename(&final_path, &source_cache).await;
                            return Err(error);
                        }
                    }
                }
                let partial = final_path.with_extension(format!("{}.part", final_path.extension().and_then(|e| e.to_str()).unwrap_or("download")));
                let max_retries = 3;
                let mut attempt = 0;
                let mut last_error = String::new();

                'retry_loop: while attempt < max_retries {
                    if cancellation.is_cancelled() {
                        emit("cancelled", 0, None, None);
                        return Ok(());
                    }

                    attempt += 1;

                    let mut downloaded_bytes = if partial.exists() {
                        fs::metadata(&partial).await.map(|m| m.len()).unwrap_or(0)
                    } else {
                        0
                    };

                    let mut req = client.get(&item.url)
                        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36");
                    if item.url.contains("yuanfudao.com") {
                        req = req.header("Referer", "https://conan.yuanfudao.com/");
                    }
                    if downloaded_bytes > 0 {
                        req = req.header("Range", format!("bytes={}-", downloaded_bytes));
                    }
                    debug_log!(
                        "http attempt id={} attempt={}/{} resume_bytes={}",
                        item.id,
                        attempt,
                        max_retries,
                        downloaded_bytes
                    );

                    let resp_result = tokio::select! {
                        result = req.send() => result,
                        _ = wait_for_cancellation(&cancellation) => {
                            emit("cancelled", downloaded_bytes, None, None);
                            return Ok(());
                        }
                    };
                    let response = match resp_result {
                        Ok(resp) => resp,
                        Err(e) => {
                            last_error = format!("网络连接异常: {e}");
                            debug_log!("http connect failed id={} attempt={} error={}", item.id, attempt, e);
                            if attempt < max_retries {
                                tokio::time::sleep(std::time::Duration::from_millis(500 * (1 << (attempt - 1)))).await;
                                continue 'retry_loop;
                            } else {
                                break 'retry_loop;
                            }
                        }
                    };

                    let status = response.status();
                    debug_log!("http response id={} attempt={} status={}", item.id, attempt, status);
                    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN || status == reqwest::StatusCode::NOT_FOUND {
                        return Err(format!("HTTP 错误: {}", status));
                    }

                    let (mut file, total_bytes) = if status == reqwest::StatusCode::PARTIAL_CONTENT {
                        let file = tokio::fs::OpenOptions::new()
                            .create(true)
                            .append(true)
                            .open(&partial)
                            .await
                            .map_err(|e| format!("打开临时文件失败: {e}"))?;
                        let cl = response.content_length();
                        let total = cl.map(|len| downloaded_bytes + len);
                        (file, total)
                    } else if status == reqwest::StatusCode::RANGE_NOT_SATISFIABLE {
                        let _ = fs::remove_file(&partial).await;
                        downloaded_bytes = 0;
                        let file = fs::File::create(&partial).await.map_err(|e| format!("创建文件失败: {e}"))?;
                        (file, None)
                    } else if status.is_success() {
                        downloaded_bytes = 0;
                        let file = fs::File::create(&partial).await.map_err(|e| format!("创建文件失败: {e}"))?;
                        (file, response.content_length())
                    } else {
                        last_error = format!("服务器返回 HTTP {}", status);
                        if attempt < max_retries {
                            tokio::time::sleep(std::time::Duration::from_millis(500 * (1 << (attempt - 1)))).await;
                            continue 'retry_loop;
                        } else {
                            break 'retry_loop;
                        }
                    };

                    let mut stream = response.bytes_stream();
                    let mut received = downloaded_bytes;
                    emit("downloading", received, total_bytes, None);

                    let mut stream_error = None;
                    loop {
                        if cancellation.is_cancelled() {
                            let _ = file.flush().await;
                            drop(file);
                            emit("cancelled", received, total_bytes, None);
                            return Ok(());
                        }

                        // 网络流卡住时也要定期检查取消标记，避免一直等待下一个数据块。
                        let chunk_res = tokio::select! {
                            chunk = stream.next() => chunk,
                            _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {
                                continue;
                            }
                        };
                        let Some(chunk_res) = chunk_res else { break; };

                        match chunk_res {
                            Ok(chunk) => {
                                if let Err(e) = file.write_all(&chunk).await {
                                    stream_error = Some(format!("写入磁盘失败: {e}"));
                                    break;
                                }
                                received += chunk.len() as u64;
                                emit("downloading", received, total_bytes, None);
                            }
                            Err(e) => {
                                stream_error = Some(format!("传输中断: {e}"));
                                break;
                            }
                        }
                    }

                    let _ = file.flush().await;
                    drop(file);

                    if let Some(err) = stream_error {
                        last_error = err;
                        debug_log!("http stream failed id={} attempt={} error={}", item.id, attempt, last_error);
                        if attempt < max_retries {
                            tokio::time::sleep(std::time::Duration::from_millis(500 * (1 << (attempt - 1)))).await;
                            continue 'retry_loop;
                        } else {
                            break 'retry_loop;
                        }
                    }

                    if let Some(expected) = total_bytes {
                        if received < expected {
                            last_error = format!("下载不完整 (已接收 {} / 总共 {})", received, expected);
                            if attempt < max_retries {
                                tokio::time::sleep(std::time::Duration::from_millis(500 * (1 << (attempt - 1)))).await;
                                continue 'retry_loop;
                            } else {
                                break 'retry_loop;
                            }
                        }
                    }

                    if final_path.exists() { let _ = fs::remove_file(&final_path).await; }
                    fs::rename(&partial, &final_path).await.map_err(|e| e.to_string())?;
                    if is_direct_mp4_video {
                        match prepare_video_cancellable(&client, &final_path, &cancellation).await {
                            Ok(true) => {}
                            Ok(false) => {
                                let _ = fs::create_dir_all(source_cache.parent().unwrap_or(&target_dir)).await;
                                let _ = fs::remove_file(&source_cache).await;
                                let _ = fs::rename(&final_path, &source_cache).await;
                                remove_playable_marker(&output, &final_path).await;
                                emit("cancelled", received, total_bytes, None);
                                return Ok(());
                            }
                            Err(error) => {
                                let _ = fs::create_dir_all(source_cache.parent().unwrap_or(&target_dir)).await;
                                let _ = fs::remove_file(&source_cache).await;
                                let _ = fs::rename(&final_path, &source_cache).await;
                                remove_playable_marker(&output, &final_path).await;
                                return Err(format!(
                                    "{error}；已保留下载源文件，重试时不会重新下载"
                                ));
                            }
                        }
                        write_playable_marker(&output, &final_path).await?;
                    }
                    emit("completed", received, total_bytes.or(Some(received)), None);
                    debug_log!("item complete id={} bytes={} path={}", item.id, received, final_path.display());
                    return Ok(());
                }

                Err(if last_error.is_empty() { "下载失败".to_string() } else { last_error })
            }.await;

            let error = result.err();
            if let Some(message) = error.as_ref() {
                debug_log!("item failed id={} error={}", item.id, message);
                emit("failed", 0, None, Some(message.clone()));
            } else {
                debug_log!("item finished id={}", item.id);
            }
            error
        }));
    }

    let mut errors = Vec::new();
    for handle in handles {
        match handle.await {
            Ok(Some(error)) => errors.push(error),
            Err(error) => errors.push(format!("下载任务异常退出：{error}")),
            Ok(None) => {}
        }
    }

    if let Ok(mut cancellations) = state.download_cancellations.lock() {
        for (id, _) in item_flags {
            cancellations.remove(&id);
        }
    }

    if errors.is_empty() {
        debug_log!("batch complete status=success");
        Ok(())
    } else {
        debug_log!("batch complete status=failed count={}", errors.len());
        Err(format!(
            "{} 个资源下载失败，可点击“重试全部失败/中断”：{}",
            errors.len(),
            errors.into_iter().take(3).collect::<Vec<_>>().join("；")
        ))
    }
}

#[tauri::command]
pub(crate) fn cancel_download(
    state: State<'_, AppState>,
    request: Option<CancelRequest>,
) -> Result<(), String> {
    let cancellations = state
        .download_cancellations
        .lock()
        .map_err(|_| "获取取消状态失败")?;
    if let Some(target_id) = request.and_then(|r| r.id) {
        debug_log!("cancel requested scope=item id={}", target_id);
        if let Some(flag) = cancellations.get(&target_id) {
            flag.store(true, Ordering::Release);
        }
    } else {
        // 同时中止仍处于“解析专辑子项”的批量任务。
        let _generation = state.download_generation.fetch_add(1, Ordering::AcqRel) + 1;
        state.album_load_generation.fetch_add(1, Ordering::Relaxed);
        debug_log!(
            "cancel requested scope=all active_items={} generation={}",
            cancellations.len(),
            _generation
        );
        for flag in cancellations.values() {
            flag.store(true, Ordering::Release);
        }
    }
    Ok(())
}
