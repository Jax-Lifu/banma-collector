use super::*;

fn title_from_url(url: &str) -> String {
    url.split(['?', '#'])
        .next()
        .unwrap_or(url)
        .rsplit('/')
        .next()
        .filter(|value| !value.is_empty())
        .unwrap_or("未命名资源")
        .to_string()
}

pub(crate) fn json_string(value: &serde_json::Value, names: &[&str]) -> Option<String> {
    names
        .iter()
        .find_map(|name| value.get(*name))
        .and_then(|v| {
            v.as_str()
                .map(str::to_owned)
                .or_else(|| v.as_i64().map(|n| n.to_string()))
        })
}

pub(crate) fn json_find_string(value: &serde_json::Value, names: &[&str]) -> Option<String> {
    if let Some(found) = json_string(value, names) {
        return Some(found);
    }
    match value {
        serde_json::Value::Array(values) => values
            .iter()
            .find_map(|value| json_find_string(value, names)),
        serde_json::Value::Object(values) => values
            .values()
            .find_map(|value| json_find_string(value, names)),
        _ => None,
    }
}

pub(crate) fn map_string(
    map: &serde_json::Map<String, serde_json::Value>,
    names: &[&str],
) -> Option<String> {
    names
        .iter()
        .find_map(|name| map.get(*name))
        .and_then(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .or_else(|| value.as_i64().map(|number| number.to_string()))
                .or_else(|| value.as_u64().map(|number| number.to_string()))
        })
        .filter(|value| !value.is_empty() && value != "0")
}

pub(crate) fn map_sequence(map: &serde_json::Map<String, serde_json::Value>) -> Option<usize> {
    [
        "sequence",
        "seq",
        "sort",
        "sortIndex",
        "order",
        "index",
        "position",
        "episodeNo",
        "serialNo",
        "rank",
    ]
    .iter()
    .find_map(|name| map.get(*name))
    .and_then(|value| value.as_u64().or_else(|| value.as_str()?.parse().ok()))
    .and_then(|value| usize::try_from(value).ok())
    .map(|value| if value == 0 { 1 } else { value })
}

pub(crate) fn is_media_url(value: &str, key: &str) -> bool {
    if !value.starts_with("http://") && !value.starts_with("https://") {
        return false;
    }
    let clean = value
        .split(['?', '#'])
        .next()
        .unwrap_or(value)
        .to_ascii_lowercase();
    let filename = clean.rsplit('/').next().unwrap_or("");
    let key_lower = key.to_ascii_lowercase();

    // Check by file extension
    let has_known_ext = filename.ends_with(".mp4")
        || filename.ends_with(".m3u8")
        || filename.ends_with(".mpd")
        || filename.ends_with(".mov")
        || filename.ends_with(".webm")
        || filename.ends_with(".flv")
        || filename.ends_with(".mkv")
        || filename.ends_with(".avi")
        || filename.ends_with(".mp3")
        || filename.ends_with(".m4a")
        || filename.ends_with(".aac")
        || filename.ends_with(".wav")
        || filename.ends_with(".ogg")
        || filename.ends_with(".flac")
        || filename.ends_with(".wma")
        || filename.ends_with(".png")
        || filename.ends_with(".jpg")
        || filename.ends_with(".jpeg")
        || filename.ends_with(".webp")
        || filename.ends_with(".gif")
        || filename.ends_with(".svg")
        || filename.ends_with(".bmp")
        || filename.ends_with(".avif")
        || filename.ends_with(".pdf")
        || filename.ends_with(".zip")
        || filename.ends_with(".epub")
        || filename.ends_with(".doc")
        || filename.ends_with(".docx")
        || filename.ends_with(".tar")
        || filename.ends_with(".gz")
        || filename.ends_with(".7z")
        || filename.ends_with(".json")
        || filename.ends_with(".svga")
        || filename.ends_with(".lottie");

    if has_known_ext {
        return true;
    }

    // Check by field key hints
    let is_resource_key = key_lower.contains("video")
        || key_lower.contains("audio")
        || key_lower.contains("playurl")
        || key_lower.contains("downloadurl")
        || key_lower.contains("song")
        || key_lower.contains("music")
        || key_lower.contains("sound")
        || key_lower.contains("voice")
        || key_lower.contains("bgm")
        || key_lower.contains("cover")
        || key_lower.contains("image")
        || key_lower.contains("pic")
        || key_lower.contains("thumb")
        || key_lower.contains("pdf")
        || key_lower.contains("zip")
        || key_lower.contains("resource")
        || key_lower.contains("attachment")
        || key_lower.contains("anim");

    is_resource_key
        && !filename.ends_with(".html")
        && !filename.ends_with(".htm")
        && !filename.ends_with(".php")
        && !filename.ends_with(".jsp")
        && !filename.ends_with(".js")
        && !filename.ends_with(".css")
}

pub(crate) fn extract_extension_and_kind(url: &str, hint_key: &str) -> (&'static str, String) {
    let clean = url.split(['?', '#']).next().unwrap_or(url);
    let filename = clean.rsplit('/').next().unwrap_or("");
    let raw_ext = if let Some((_, ext)) = filename.rsplit_once('.') {
        ext.to_ascii_lowercase()
    } else {
        String::new()
    };

    let valid_ext = if raw_ext.len() <= 6 && raw_ext.chars().all(|c| c.is_ascii_alphanumeric()) {
        raw_ext
    } else {
        String::new()
    };

    let key_lower = hint_key.to_ascii_lowercase();
    let is_audio_hint = key_lower.contains("audio")
        || key_lower.contains("sound")
        || key_lower.contains("voice")
        || key_lower.contains("music")
        || key_lower.contains("song")
        || key_lower.contains("bgm");
    let is_video_hint = key_lower.contains("video") || key_lower.contains("playurl");
    let is_image_hint = key_lower.contains("image")
        || key_lower.contains("cover")
        || key_lower.contains("pic")
        || key_lower.contains("thumb")
        || key_lower.contains("photo");
    let is_doc_hint = key_lower.contains("pdf")
        || key_lower.contains("doc")
        || key_lower.contains("epub")
        || key_lower.contains("zip")
        || key_lower.contains("pack");
    let is_data_hint = key_lower.contains("json")
        || key_lower.contains("svga")
        || key_lower.contains("lottie")
        || key_lower.contains("anim");

    match valid_ext.as_str() {
        "mp4" | "m3u8" | "mpd" | "mov" | "webm" | "flv" | "mkv" | "avi" => ("video", valid_ext),
        "mp3" | "m4a" | "aac" | "wav" | "ogg" | "flac" | "wma" | "m4r" | "opus" => {
            ("audio", valid_ext)
        }
        "jpg" | "jpeg" | "png" | "webp" | "gif" | "avif" | "svg" | "bmp" => ("image", valid_ext),
        "json" | "svga" | "lottie" => ("data", valid_ext),
        "pdf" | "zip" | "epub" | "doc" | "docx" | "tar" | "gz" | "7z" | "rar" => {
            ("document", valid_ext)
        }
        _ => {
            if is_audio_hint {
                ("audio", "mp3".to_string())
            } else if is_video_hint {
                ("video", "mp4".to_string())
            } else if is_image_hint {
                ("image", "png".to_string())
            } else if is_doc_hint {
                ("document", "pdf".to_string())
            } else if is_data_hint {
                ("data", "json".to_string())
            } else {
                ("other", valid_ext)
            }
        }
    }
}

pub(crate) fn parse_product_content_scoped(
    value: &serde_json::Value,
    product: &str,
    default_subfolder: Option<&str>,
) -> ProductContent {
    #[allow(clippy::too_many_arguments)]
    fn walk(
        value: &serde_json::Value,
        product: &str,
        entries: &mut Vec<ContentEntry>,
        videos: &mut Vec<ResourceItem>,
        cursor: &mut Option<String>,
        context: Option<&str>,
        subfolder: Option<&str>,
        sequence: Option<usize>,
        language: Option<&str>,
        in_interactive_content: bool,
    ) {
        match value {
            serde_json::Value::Array(values) => {
                for (index, value) in values.iter().enumerate() {
                    walk(
                        value,
                        product,
                        entries,
                        videos,
                        cursor,
                        context,
                        subfolder,
                        Some(index + 1),
                        language,
                        in_interactive_content,
                    );
                }
            }
            serde_json::Value::Object(map) => {
                if cursor.is_none() {
                    *cursor = map_string(map, &["cursor", "nextCursor"]);
                }
                let identity = [
                    ("packId", "pack"),
                    ("courseId", "course"),
                    ("lessonMissionId", "mission"),
                    ("missionId", "mission"),
                    ("episodeId", "episode"),
                    ("unitUniqId", "unit"),
                    ("albumId", "album"),
                    ("songId", "song"),
                    ("bookId", "book"),
                    ("audioId", "audio"),
                ]
                .iter()
                .find_map(|(key, kind)| map_string(map, &[*key]).map(|id| (id, *kind)));
                let title = map_string(
                    map,
                    &[
                        "title",
                        "name",
                        "courseName",
                        "missionName",
                        "episodeName",
                        "unitName",
                        "displayName",
                        "songName",
                        "albumName",
                        "audioName",
                        "storyName",
                    ],
                );
                if let Some((id, kind)) = identity {
                    let display = title.clone().unwrap_or_else(|| {
                        format!(
                            "{} {id}",
                            match kind {
                                "pack" => "课程包",
                                "course" => "课程",
                                "mission" => "学习任务",
                                "episode" => "课节",
                                "unit" => "学习单元",
                                "album" => "音频专辑",
                                "song" => "音频曲目",
                                "audio" => "音频",
                                "book" => "绘本",
                                _ => "学习内容",
                            }
                        )
                    });
                    let action_url = map_string(map, &["jumpUrl", "actionUrl", "url"]);
                    let has_detail =
                        matches!(kind, "pack" | "course" | "mission" | "album" | "book")
                            || (kind == "episode" && (context.is_some() || action_url.is_some()))
                            || action_url
                                .as_ref()
                                .is_some_and(|u| u.starts_with(ACCOUNT_HOST));
                    entries.push(ContentEntry {
                        id,
                        title: display,
                        subtitle: map_string(
                            map,
                            &[
                                "subTitle",
                                "subtitle",
                                "desc",
                                "description",
                                "studyStatusDesc",
                                "epDesc",
                                "learningProgress",
                                "content",
                                "lessonInfo",
                                "hint",
                            ],
                        ),
                        cover_url: map_string(
                            map,
                            &[
                                "coverImageUrl",
                                "coverUrl",
                                "imageUrl",
                                "imgUrl",
                                "mobileHeadImageUrl",
                                "thumbnailImgUrl",
                            ],
                        ),
                        kind: kind.into(),
                        locked: map
                            .get("lock")
                            .or_else(|| map.get("locked"))
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false)
                            || map.get("valid").and_then(|v| v.as_bool()) == Some(false)
                            || map.get("hasPermission").and_then(|v| v.as_bool()) == Some(false)
                            || map.get("studyStatus").and_then(|v| v.as_i64()) == Some(3),
                        action_url,
                        parent_id: context.map(str::to_owned),
                        has_detail,
                    });
                }
                let current_subfolder =
                    map_string(map, &["albumName", "packName", "courseName", "missionName"])
                        .or_else(|| subfolder.map(str::to_owned));
                let next_context = map_string(
                    map,
                    &[
                        "missionId",
                        "lessonMissionId",
                        "courseId",
                        "packId",
                        "albumId",
                    ],
                )
                .or_else(|| context.map(str::to_owned));
                let current_sequence = map_sequence(map).or(sequence);
                for (key, child) in map {
                    let key_lower = key.to_ascii_lowercase();
                    // 百科同一课节会把中文正文放在 contents、英文正文放在
                    // englishCourseContents。语言属于所在分支而不是媒体 URL 本身，
                    // 必须沿递归树传递，才能正确标注同一视频的中英文版本。
                    let child_language = if product.eq_ignore_ascii_case("pedia")
                        && key_lower.contains("english")
                        && key_lower.contains("content")
                    {
                        Some("英文")
                    } else if product.eq_ignore_ascii_case("pedia")
                        && key_lower == "contents"
                        && map.keys().any(|name| {
                            let name = name.to_ascii_lowercase();
                            name.contains("english") && name.contains("content")
                        })
                    {
                        Some("中文")
                    } else {
                        language
                    };
                    let child_in_interactive_content = in_interactive_content
                        || key_lower.contains("encyquestion")
                        || key_lower.contains("cardquestion")
                        || key_lower.contains("encyentryaudios");
                    // 百科课程包详情会附带 recommendEncyclopedia / recommendVO 等推荐卡片。
                    // 移动端将它们作为跳转到其他课程的推荐入口，不属于当前课程包的离线内容。
                    // 在递归入口跳过整棵推荐子树，避免推荐课程的目录和素材混入下载列表。
                    if product.eq_ignore_ascii_case("pedia")
                        && key.to_ascii_lowercase().starts_with("recommend")
                    {
                        continue;
                    }
                    // medalPopInfo 是完成课程进度后由课程界面展示的勋章奖励弹窗，
                    // 其中 audioUrl 只是弹窗提示音/配音，不属于课程正文音频。
                    if product.eq_ignore_ascii_case("pedia")
                        && key.eq_ignore_ascii_case("medalPopInfo")
                    {
                        continue;
                    }
                    // openingAnimationUrl、transAnimationVideoUrl / AudioUrl 等均为
                    // 播放页的开场或章节转场素材不属于课程正文。
                    if product.eq_ignore_ascii_case("pedia")
                        && key.to_ascii_lowercase().contains("animation")
                    {
                        continue;
                    }
                    if let Some(url) = child.as_str().filter(|url| is_media_url(url, key)) {
                        // 百科接口会把视频时间轴的逐帧预览图和课程资源放在同一棵 JSON 中。
                        // 播放器只在拖动进度时按 offset 使用这些 imageUrl，不把它们当作
                        // 独立课件；桌面端同样不应展示或下载几十张视频截图。
                        let is_timeline_preview = product.eq_ignore_ascii_case("pedia")
                            && key == "imageUrl"
                            && map.get("offset").is_some_and(|offset| {
                                offset.as_u64().is_some()
                                    || offset
                                        .as_str()
                                        .and_then(|value| value.parse::<u64>().ok())
                                        .is_some()
                            });
                        if is_timeline_preview {
                            continue;
                        }
                        let (kind, extension) = extract_extension_and_kind(url, key);
                        // unityResourceUrl 下的 DAT 是移动端三维模型的运行数据，
                        // 依赖旋转、点击热区等交互代码，既不是课件也不能独立预览。
                        if product.eq_ignore_ascii_case("pedia")
                            && extension.eq_ignore_ascii_case("dat")
                        {
                            continue;
                        }
                        // 百科互动答题中的开场提示、卡片介绍、反馈音效和局部条目发音
                        // 依赖题目交互界面，脱离交互后不是可独立使用的课程音频。
                        if product.eq_ignore_ascii_case("pedia")
                            && kind == "audio"
                            && in_interactive_content
                        {
                            continue;
                        }
                        // 百科响应中绝大多数 JPG/PNG 是播放器背景、会员引导、封面、
                        // 水印或 Unity 运行素材。只有接口明确标记为课件/附件/学习材料的
                        // 图片才作为可下载资源；封面仍由 ContentEntry.cover_url 用于界面。
                        let is_downloadable_pedia_image =
                            if product.eq_ignore_ascii_case("pedia") && kind == "image" {
                                let key_lower = key.to_ascii_lowercase();
                                [
                                    "attachment",
                                    "courseware",
                                    "handout",
                                    "material",
                                    "document",
                                    "download",
                                ]
                                .iter()
                                .any(|hint| key_lower.contains(hint))
                            } else {
                                true
                            };
                        if !is_downloadable_pedia_image {
                            continue;
                        }
                        // 百科中的 Android.zip、模型名.zip 等是 Unity/3D 互动课程的
                        // 平台运行包。ZIP 只有被明确标记为课件或附件时才提供下载。
                        if product.eq_ignore_ascii_case("pedia")
                            && extension.eq_ignore_ascii_case("zip")
                        {
                            let key_lower = key.to_ascii_lowercase();
                            let is_course_attachment = [
                                "attachment",
                                "courseware",
                                "handout",
                                "material",
                                "document",
                                "download",
                            ]
                            .iter()
                            .any(|hint| key_lower.contains(hint));
                            if !is_course_attachment {
                                continue;
                            }
                        }
                        let id = hex::encode(Sha256::digest(url.as_bytes()))[..16].to_string();
                        videos.push(ResourceItem {
                            id,
                            title: title.clone().unwrap_or_else(|| title_from_url(url)),
                            url: url.into(),
                            kind: kind.into(),
                            extension,
                            size: None,
                            source: product.into(),
                            subfolder: current_subfolder.clone(),
                            sequence: current_sequence,
                            quality: (kind == "video")
                                .then(|| video_quality_from_key(key))
                                .flatten(),
                            language: language.map(str::to_owned),
                        });
                    }
                    walk(
                        child,
                        product,
                        entries,
                        videos,
                        cursor,
                        next_context.as_deref(),
                        current_subfolder.as_deref(),
                        current_sequence,
                        child_language,
                        child_in_interactive_content,
                    );
                }
            }
            _ => {}
        }
    }
    let mut entries = Vec::new();
    let mut videos = Vec::new();
    let mut cursor = None;
    walk(
        value,
        product,
        &mut entries,
        &mut videos,
        &mut cursor,
        None,
        default_subfolder,
        None,
        None,
        false,
    );
    let mut entry_seen = HashSet::new();
    entries.retain(|entry| entry_seen.insert(format!("{}:{}", entry.kind, entry.id)));
    // 同一 URL 可能在新旧字段中重复出现；优先保留带清晰度语义的字段。
    let mut unique_videos = Vec::<ResourceItem>::new();
    let mut url_indexes = HashMap::<String, usize>::new();
    for video in videos {
        if let Some(index) = url_indexes.get(&video.url).copied() {
            if unique_videos[index].quality.is_none() && video.quality.is_some() {
                // preloadURLs 会重复正文视频地址，但通常缺少可读标题；只补清晰度，
                // 保留正文节点上的课程名和目录信息。
                unique_videos[index].quality = video.quality;
            }
            if unique_videos[index].language.is_none() && video.language.is_some() {
                unique_videos[index].language = video.language;
            }
        } else {
            url_indexes.insert(video.url.clone(), unique_videos.len());
            unique_videos.push(video);
        }
    }
    let mut lesson_video_indexes = HashMap::<String, usize>::new();
    let mut deduplicated_videos = Vec::<ResourceItem>::new();
    for video in unique_videos {
        if video.kind != "video" || !is_uuid_media_title(&video.title) {
            deduplicated_videos.push(video);
            continue;
        }
        let key = format!(
            "{}:{}:{}:{}",
            video.subfolder.as_deref().unwrap_or(""),
            video.sequence.unwrap_or(0),
            video.quality.as_deref().unwrap_or("默认"),
            video.language.as_deref().unwrap_or("默认")
        );
        if let Some(index) = lesson_video_indexes.get(&key).copied() {
            if video_variant_preference(&video)
                > video_variant_preference(&deduplicated_videos[index])
            {
                deduplicated_videos[index] = video;
            }
        } else {
            lesson_video_indexes.insert(key, deduplicated_videos.len());
            deduplicated_videos.push(video);
        }
    }
    let mut videos = deduplicated_videos;
    // DASH/HLS 地址通常只带 UUID 文件名。完成清晰度去重后再用课程目录名
    // 替换技术性标题，让资源列表和最终 MP4 文件名面向用户而不是面向 CDN。
    for video in &mut videos {
        if video.kind == "video"
            && (is_uuid_media_title(&video.title) || video.title.trim() == "课程视频")
        {
            if let Some(lesson_name) = video
                .subfolder
                .as_deref()
                .and_then(|folder| folder.rsplit('/').find(|part| !part.trim().is_empty()))
            {
                video.title = lesson_name.to_string();
            }
        }
    }
    ProductContent {
        entries,
        videos,
        cursor,
        warning: None,
    }
}

fn is_uuid_media_title(title: &str) -> bool {
    let stem = Path::new(title)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or(title);
    stem.len() == 36
        && stem.chars().enumerate().all(|(index, value)| {
            if matches!(index, 8 | 13 | 18 | 23) {
                value == '-'
            } else {
                value.is_ascii_hexdigit()
            }
        })
}

fn video_variant_preference(item: &ResourceItem) -> u8 {
    // 同为默认 1080P 时优先 DASH：可稳定选择 AVC 轨道并由 FFmpeg 直接完成 CENC 解密。
    if item.extension.eq_ignore_ascii_case("mpd") {
        2
    } else {
        1
    }
}

fn video_quality_from_key(key: &str) -> Option<String> {
    let key = key.to_ascii_lowercase();
    if key.contains("uhd") {
        Some("4K".into())
    } else if key.contains("udvideourl") {
        Some("1080P".into())
    } else if key.contains("hdvideourl") {
        Some("720P".into())
    } else if key.contains("sdvideourl") {
        Some("标清".into())
    } else {
        None
    }
}
