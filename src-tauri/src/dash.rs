use super::*;
use crate::download_cancel::{wait_for_cancellation, DownloadCancellation};
use futures_util::stream::{self, StreamExt};
use quick_xml::{events::Event, Reader, XmlVersion};
use reqwest::header::{CONTENT_RANGE, RANGE};
use std::sync::atomic::AtomicU64;

const DASH_CHUNK_SIZE: u64 = 8 * 1024 * 1024;
const DASH_CHUNK_CONCURRENCY: usize = 8;
const DASH_CHUNK_RETRIES: usize = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
struct DashRepresentation {
    content_type: String,
    mime_type: String,
    codecs: String,
    bandwidth: u64,
    base_url: String,
    has_segment_base: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DashOnDemandPlan {
    video_url: String,
    audio_url: Option<String>,
}

#[derive(Debug)]
pub(crate) enum DashParallelOutcome {
    Completed,
    Unsupported(String),
    Cancelled,
}

pub(crate) async fn cleanup_dash_workspace(output: &Path) {
    let _ = fs::remove_dir_all(output.with_extension("dash-parts")).await;
}

fn local_name(name: &[u8]) -> &[u8] {
    name.rsplit(|value| *value == b':').next().unwrap_or(name)
}

fn attribute_value(
    _reader: &Reader<&[u8]>,
    event: &quick_xml::events::BytesStart<'_>,
    name: &[u8],
) -> Result<Option<String>, String> {
    for attribute in event.attributes().with_checks(false) {
        let attribute = attribute.map_err(|error| format!("DASH 属性格式无效：{error}"))?;
        if local_name(attribute.key.as_ref()).eq_ignore_ascii_case(name) {
            return attribute
                .normalized_value(XmlVersion::Implicit1_0)
                .map(|value| Some(value.into_owned()))
                .map_err(|error| format!("DASH 属性解码失败：{error}"));
        }
    }
    Ok(None)
}

fn parse_on_demand_representations(manifest: &str) -> Result<Vec<DashRepresentation>, String> {
    let mut reader = Reader::from_str(manifest);
    reader.config_mut().trim_text(true);
    let mut adaptation_type = String::new();
    let mut adaptation_mime = String::new();
    let mut current: Option<DashRepresentation> = None;
    let mut reading_base_url = false;
    let mut representations = Vec::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(event)) => match local_name(event.name().as_ref()) {
                b"AdaptationSet" => {
                    adaptation_type =
                        attribute_value(&reader, &event, b"contentType")?.unwrap_or_default();
                    adaptation_mime =
                        attribute_value(&reader, &event, b"mimeType")?.unwrap_or_default();
                }
                b"Representation" => {
                    let mime_type = attribute_value(&reader, &event, b"mimeType")?
                        .unwrap_or_else(|| adaptation_mime.clone());
                    let content_type = if adaptation_type.is_empty() {
                        mime_type.split('/').next().unwrap_or_default().to_string()
                    } else {
                        adaptation_type.clone()
                    };
                    current = Some(DashRepresentation {
                        content_type,
                        mime_type,
                        codecs: attribute_value(&reader, &event, b"codecs")?.unwrap_or_default(),
                        bandwidth: attribute_value(&reader, &event, b"bandwidth")?
                            .and_then(|value| value.parse().ok())
                            .unwrap_or(0),
                        base_url: String::new(),
                        has_segment_base: false,
                    });
                }
                b"BaseURL" if current.is_some() => reading_base_url = true,
                b"SegmentBase" if current.is_some() => {
                    if let Some(representation) = current.as_mut() {
                        representation.has_segment_base = true;
                    }
                }
                _ => {}
            },
            Ok(Event::Empty(event)) => {
                if local_name(event.name().as_ref()) == b"SegmentBase" {
                    if let Some(representation) = current.as_mut() {
                        representation.has_segment_base = true;
                    }
                }
            }
            Ok(Event::Text(text)) if reading_base_url => {
                if let Some(representation) = current.as_mut() {
                    representation.base_url = text
                        .decode()
                        .map_err(|error| format!("DASH BaseURL 解码失败：{error}"))?
                        .into_owned();
                }
            }
            Ok(Event::End(event)) => match local_name(event.name().as_ref()) {
                b"BaseURL" => reading_base_url = false,
                b"Representation" => {
                    if let Some(representation) = current.take() {
                        representations.push(representation);
                    }
                }
                b"AdaptationSet" => {
                    adaptation_type.clear();
                    adaptation_mime.clear();
                }
                _ => {}
            },
            Ok(Event::Eof) => break,
            Err(error) => return Err(format!("DASH 清单 XML 无效：{error}")),
            _ => {}
        }
    }
    Ok(representations)
}

fn build_on_demand_plan(manifest_url: &str, manifest: &str) -> Result<DashOnDemandPlan, String> {
    let base =
        url::Url::parse(manifest_url).map_err(|error| format!("DASH 清单地址无效：{error}"))?;
    let representations = parse_on_demand_representations(manifest)?;
    let resolve = |value: &str| {
        base.join(value)
            .map(|value| value.to_string())
            .map_err(|error| format!("DASH 媒体地址无效：{error}"))
    };

    let is_avc = |representation: &&DashRepresentation| {
        let codec = representation.codecs.to_ascii_lowercase();
        codec.starts_with("avc1") || codec.starts_with("avc3")
    };
    let video = representations
        .iter()
        .filter(|representation| {
            representation.has_segment_base
                && !representation.base_url.is_empty()
                && (representation.content_type.eq_ignore_ascii_case("video")
                    || representation.mime_type.starts_with("video/"))
        })
        .filter(is_avc)
        .max_by_key(|representation| representation.bandwidth)
        .ok_or_else(|| "DASH On-Demand 清单中没有可并行下载的 AVC 视频轨道".to_string())?;
    let audio = representations
        .iter()
        .filter(|representation| {
            representation.has_segment_base
                && !representation.base_url.is_empty()
                && (representation.content_type.eq_ignore_ascii_case("audio")
                    || representation.mime_type.starts_with("audio/"))
        })
        .max_by_key(|representation| representation.bandwidth);

    Ok(DashOnDemandPlan {
        video_url: resolve(&video.base_url)?,
        audio_url: audio.map(|value| resolve(&value.base_url)).transpose()?,
    })
}

async fn request_with_range(
    client: &reqwest::Client,
    url: &str,
    start: u64,
    end: u64,
) -> Result<reqwest::Response, String> {
    client
        .get(url)
        .header("User-Agent", "ZebraAndroid/1.0")
        .header("Referer", "https://conan.yuanfudao.com/")
        .header(RANGE, format!("bytes={start}-{end}"))
        .send()
        .await
        .map_err(|error| format!("DASH 分片连接失败：{error}"))
}

async fn remote_size(client: &reqwest::Client, url: &str) -> Result<u64, String> {
    let response = request_with_range(client, url, 0, 0).await?;
    if response.status() != reqwest::StatusCode::PARTIAL_CONTENT {
        return Err(format!(
            "CDN 不支持 HTTP Range（返回 {}）",
            response.status()
        ));
    }
    let content_range = response
        .headers()
        .get(CONTENT_RANGE)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| "CDN 未返回 Content-Range".to_string())?;
    content_range
        .rsplit_once('/')
        .and_then(|(_, total)| total.parse::<u64>().ok())
        .filter(|total| *total > 0)
        .ok_or_else(|| format!("CDN Content-Range 无效：{content_range}"))
}

#[derive(Clone)]
struct ChunkJob {
    path: PathBuf,
    start: u64,
    end: u64,
}

fn chunk_jobs(directory: &Path, total: u64) -> Vec<ChunkJob> {
    (0..total.div_ceil(DASH_CHUNK_SIZE))
        .map(|index| {
            let start = index * DASH_CHUNK_SIZE;
            let end = (start + DASH_CHUNK_SIZE - 1).min(total - 1);
            ChunkJob {
                path: directory.join(format!("{index:06}.chunk")),
                start,
                end,
            }
        })
        .collect()
}

async fn download_chunk(
    client: &reqwest::Client,
    url: &str,
    job: &ChunkJob,
    cancellation: &DownloadCancellation,
) -> Result<u64, String> {
    let expected = job.end - job.start + 1;
    if fs::metadata(&job.path)
        .await
        .is_ok_and(|metadata| metadata.len() == expected)
    {
        return Ok(expected);
    }
    let temporary = job.path.with_extension("part");
    let _ = fs::remove_file(&temporary).await;
    let mut last_error = String::new();

    for attempt in 1..=DASH_CHUNK_RETRIES {
        if cancellation.is_cancelled() {
            return Err("CANCELLED".into());
        }
        let response = tokio::select! {
            response = request_with_range(client, url, job.start, job.end) => response,
            _ = wait_for_cancellation(cancellation) => return Err("CANCELLED".into()),
        };
        let response = match response {
            Ok(response) if response.status() == reqwest::StatusCode::PARTIAL_CONTENT => response,
            Ok(response) => {
                last_error = format!("CDN 返回 HTTP {}", response.status());
                continue;
            }
            Err(error) => {
                last_error = error;
                continue;
            }
        };
        let mut file = fs::File::create(&temporary)
            .await
            .map_err(|error| format!("创建 DASH 分片失败：{error}"))?;
        let mut stream = response.bytes_stream();
        let mut received = 0u64;
        let mut stream_error = None;
        loop {
            let next = tokio::select! {
                next = stream.next() => next,
                _ = wait_for_cancellation(cancellation) => {
                    stream_error = Some("CANCELLED".to_string());
                    break;
                }
            };
            let Some(next) = next else { break };
            match next {
                Ok(bytes) => {
                    file.write_all(&bytes)
                        .await
                        .map_err(|error| format!("写入 DASH 分片失败：{error}"))?;
                    received += bytes.len() as u64;
                }
                Err(error) => {
                    stream_error = Some(format!("DASH 分片传输中断：{error}"));
                    break;
                }
            }
        }
        file.flush()
            .await
            .map_err(|error| format!("刷新 DASH 分片失败：{error}"))?;
        drop(file);
        if stream_error.as_deref() == Some("CANCELLED") {
            let _ = fs::remove_file(&temporary).await;
            return Err("CANCELLED".into());
        }
        if let Some(error) = stream_error {
            last_error = error;
        } else if received == expected {
            let _ = fs::remove_file(&job.path).await;
            fs::rename(&temporary, &job.path)
                .await
                .map_err(|error| format!("保存 DASH 分片失败：{error}"))?;
            return Ok(received);
        } else {
            last_error = format!("DASH 分片长度不符（{received}/{expected}）");
        }
        let _ = fs::remove_file(&temporary).await;
        if attempt < DASH_CHUNK_RETRIES {
            tokio::time::sleep(std::time::Duration::from_millis(300 * attempt as u64)).await;
        }
    }
    Err(last_error)
}

async fn combine_chunks(jobs: &[ChunkJob], output: &Path) -> Result<(), String> {
    let _ = fs::remove_file(output).await;
    let mut destination = fs::File::create(output)
        .await
        .map_err(|error| format!("创建 DASH 轨道文件失败：{error}"))?;
    for job in jobs {
        let mut source = fs::File::open(&job.path)
            .await
            .map_err(|error| format!("打开 DASH 分片失败：{error}"))?;
        tokio::io::copy(&mut source, &mut destination)
            .await
            .map_err(|error| format!("合并 DASH 分片失败：{error}"))?;
    }
    destination
        .flush()
        .await
        .map_err(|error| format!("刷新 DASH 轨道文件失败：{error}"))
}

async fn mux_tracks(
    video: &Path,
    audio: Option<&Path>,
    output: &Path,
    decryption_key: Option<&str>,
    cancellation: &DownloadCancellation,
) -> Result<bool, String> {
    let _ = fs::remove_file(output).await;
    let mut command = Command::new(crate::runtime_tools::command("ffmpeg"));
    crate::runtime_tools::hide_window(&mut command);
    command.args(["-nostdin", "-y", "-hide_banner", "-loglevel", "error"]);
    if let Some(key) = decryption_key {
        command.args(["-decryption_key", key]);
    }
    command.arg("-i").arg(video);
    if let Some(audio) = audio {
        if let Some(key) = decryption_key {
            command.args(["-decryption_key", key]);
        }
        command.arg("-i").arg(audio);
    }
    command.args(["-map", "0:v:0"]);
    if audio.is_some() {
        command.args(["-map", "1:a:0?"]);
    }
    command
        .args(["-c", "copy", "-movflags", "+faststart"])
        .arg(output)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let child = command
        .spawn()
        .map_err(|error| format!("启动 DASH 本地合并失败：{error}"))?;
    let result = tokio::select! {
        result = child.wait_with_output() => Some(result.map_err(|error| format!("等待 DASH 合并失败：{error}"))?),
        _ = wait_for_cancellation(cancellation) => None,
    };
    let Some(result) = result else {
        let _ = fs::remove_file(output).await;
        return Ok(false);
    };
    if !result.status.success() {
        let _ = fs::remove_file(output).await;
        return Err(format!(
            "DASH 本地合并失败{}",
            command_error_detail(&result.stderr)
        ));
    }
    Ok(true)
}

pub(crate) async fn try_download_dash_parallel<F>(
    client: &reqwest::Client,
    manifest_url: &str,
    output: &Path,
    decryption_key: Option<&str>,
    cancellation: &DownloadCancellation,
    progress: F,
) -> Result<DashParallelOutcome, String>
where
    F: Fn(u64, u64) + Sync,
{
    let response = tokio::select! {
        response = client.get(manifest_url)
            .header("User-Agent", "ZebraAndroid/1.0")
            .header("Referer", "https://conan.yuanfudao.com/")
            .send() => response.map_err(|error| format!("读取 DASH 清单失败：{error}"))?,
        _ = wait_for_cancellation(cancellation) => return Ok(DashParallelOutcome::Cancelled),
    };
    let manifest = response
        .error_for_status()
        .map_err(|error| format!("DASH 清单请求失败：{error}"))?
        .text()
        .await
        .map_err(|error| format!("读取 DASH 清单失败：{error}"))?;
    let plan = match build_on_demand_plan(manifest_url, &manifest) {
        Ok(plan) => plan,
        Err(reason) => return Ok(DashParallelOutcome::Unsupported(reason)),
    };
    let video_size = match remote_size(client, &plan.video_url).await {
        Ok(size) => size,
        Err(reason) => return Ok(DashParallelOutcome::Unsupported(reason)),
    };
    let audio_size = match plan.audio_url.as_deref() {
        Some(url) => match remote_size(client, url).await {
            Ok(size) => Some(size),
            Err(reason) => return Ok(DashParallelOutcome::Unsupported(reason)),
        },
        None => None,
    };
    let total = video_size + audio_size.unwrap_or(0);
    let workspace = output.with_extension("dash-parts");
    let video_directory = workspace.join("video");
    let audio_directory = workspace.join("audio");
    fs::create_dir_all(&video_directory)
        .await
        .map_err(|error| format!("创建 DASH 分片目录失败：{error}"))?;
    if audio_size.is_some() {
        fs::create_dir_all(&audio_directory)
            .await
            .map_err(|error| format!("创建 DASH 音频分片目录失败：{error}"))?;
    }
    let video_jobs = chunk_jobs(&video_directory, video_size);
    let audio_jobs = audio_size.map(|size| chunk_jobs(&audio_directory, size));
    let received = AtomicU64::new(0);
    let existing = video_jobs
        .iter()
        .chain(audio_jobs.iter().flatten())
        .filter_map(|job| {
            std::fs::metadata(&job.path)
                .ok()
                .filter(|metadata| metadata.len() == job.end - job.start + 1)
                .map(|metadata| metadata.len())
        })
        .sum::<u64>();
    received.store(existing, Ordering::Release);
    progress(existing, total);

    let jobs = video_jobs
        .iter()
        .cloned()
        .map(|job| (plan.video_url.clone(), job))
        .chain(audio_jobs.iter().flatten().cloned().map(|job| {
            (
                plan.audio_url.as_ref().expect("audio URL exists").clone(),
                job,
            )
        }))
        .collect::<Vec<_>>();
    let results = stream::iter(jobs)
        .map(|(url, job)| {
            let received = &received;
            let progress = &progress;
            async move {
                let already_complete = fs::metadata(&job.path)
                    .await
                    .is_ok_and(|metadata| metadata.len() == job.end - job.start + 1);
                let size = download_chunk(client, &url, &job, cancellation).await?;
                if !already_complete {
                    let current = received.fetch_add(size, Ordering::AcqRel) + size;
                    progress(current, total);
                }
                Ok::<(), String>(())
            }
        })
        .buffer_unordered(DASH_CHUNK_CONCURRENCY)
        .collect::<Vec<_>>()
        .await;
    for result in results {
        if let Err(error) = result {
            if error == "CANCELLED" || cancellation.is_cancelled() {
                return Ok(DashParallelOutcome::Cancelled);
            }
            return Err(format!("DASH 分片并行下载失败：{error}"));
        }
    }

    let video_track = workspace.join("video.mp4");
    combine_chunks(&video_jobs, &video_track).await?;
    let audio_track = if let Some(audio_jobs) = audio_jobs.as_ref() {
        let path = workspace.join("audio.mp4");
        combine_chunks(audio_jobs, &path).await?;
        Some(path)
    } else {
        None
    };
    if !mux_tracks(
        &video_track,
        audio_track.as_deref(),
        output,
        decryption_key,
        cancellation,
    )
    .await?
    {
        return Ok(DashParallelOutcome::Cancelled);
    }
    cleanup_dash_workspace(output).await;
    Ok(DashParallelOutcome::Completed)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ON_DEMAND_MPD: &str = r#"<?xml version="1.0"?>
<MPD xmlns="urn:mpeg:dash:schema:mpd:2011">
  <Period><AdaptationSet contentType="audio">
    <Representation bandwidth="128000" codecs="mp4a.40.2" mimeType="audio/mp4">
      <BaseURL>audio/main.mp4</BaseURL><SegmentBase indexRange="100-200" />
    </Representation>
  </AdaptationSet><AdaptationSet contentType="video">
    <Representation bandwidth="9000000" codecs="hvc1.1.6" mimeType="video/mp4">
      <BaseURL>video/hevc.mp4</BaseURL><SegmentBase indexRange="100-200" />
    </Representation>
    <Representation bandwidth="12000000" codecs="avc1.640033" mimeType="video/mp4">
      <BaseURL>video/avc.mp4</BaseURL><SegmentBase indexRange="100-200" />
    </Representation>
  </AdaptationSet></Period>
</MPD>"#;

    #[test]
    fn selects_avc_and_resolves_on_demand_tracks() {
        let plan = build_on_demand_plan(
            "https://cdn.example.com/course/manifest.mpd?token=secret",
            ON_DEMAND_MPD,
        )
        .unwrap();
        assert_eq!(
            plan.video_url,
            "https://cdn.example.com/course/video/avc.mp4"
        );
        assert_eq!(
            plan.audio_url.as_deref(),
            Some("https://cdn.example.com/course/audio/main.mp4")
        );
    }

    #[test]
    fn splits_ranges_without_gaps() {
        let jobs = chunk_jobs(Path::new("parts"), DASH_CHUNK_SIZE * 2 + 17);
        assert_eq!(jobs.len(), 3);
        assert_eq!((jobs[0].start, jobs[0].end), (0, DASH_CHUNK_SIZE - 1));
        assert_eq!(
            (jobs[1].start, jobs[1].end),
            (DASH_CHUNK_SIZE, DASH_CHUNK_SIZE * 2 - 1)
        );
        assert_eq!(
            (jobs[2].start, jobs[2].end),
            (DASH_CHUNK_SIZE * 2, DASH_CHUNK_SIZE * 2 + 16)
        );
    }

    #[tokio::test(flavor = "current_thread")]
    #[ignore = "requires the live Banma CDN, media key service and ffprobe"]
    async fn live_on_demand_manifest_downloads_and_muxes() {
        let manifest = "https://maple-online.fbcontent.cn/public/release/trans/app-CONAN_ZDT_ENCYCLOPEDIA_1080_ENCRYPT_DASH/451696ba-0824-42a9-b503-da112ed020bd/451696ba-0824-42a9-b503-da112ed020bd.mpd";
        let client = reqwest::Client::new();
        let (key, _) = manifest_decryption_context(&client, manifest)
            .await
            .unwrap();
        let generation = Arc::new(AtomicU64::new(0));
        let cancellation = DownloadCancellation {
            item_flag: Arc::new(AtomicBool::new(false)),
            generation,
            batch_generation: 0,
        };
        let output = std::env::temp_dir().join("banma-dash-parallel-live.part.mp4");
        let _ = fs::remove_file(&output).await;
        cleanup_dash_workspace(&output).await;
        let outcome = try_download_dash_parallel(
            &client,
            manifest,
            &output,
            key.as_deref(),
            &cancellation,
            |_, _| {},
        )
        .await
        .unwrap();
        assert!(matches!(outcome, DashParallelOutcome::Completed));
        let probe = Command::new(crate::runtime_tools::command("ffprobe"))
            .args([
                "-v",
                "error",
                "-show_entries",
                "stream=codec_type",
                "-of",
                "csv=p=0",
            ])
            .arg(&output)
            .output()
            .await
            .unwrap();
        assert!(
            probe.status.success(),
            "{}",
            String::from_utf8_lossy(&probe.stderr)
        );
        let streams = String::from_utf8_lossy(&probe.stdout);
        assert!(streams.contains("video"));
        assert!(streams.contains("audio"));
        let _ = fs::remove_file(&output).await;
        cleanup_dash_workspace(&output).await;
    }
}
