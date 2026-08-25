use super::*;

#[tauri::command]
pub(crate) async fn load_product_catalog(
    state: State<'_, AppState>,
    request: ProductRequest,
) -> Result<ProductContent, String> {
    let session = product_session(&state, &request.product)?;
    let user_id = session.user_id.unwrap_or_default();
    if request.product == "zebra" {
        let mut content = ProductContent {
            entries: Vec::new(),
            videos: Vec::new(),
            cursor: None,
            warning: None,
        };
        let mut errors = Vec::new();

        // 1. 拓展推荐资源
        let extension_url = format!("{ACCOUNT_HOST}/conan-zvas/android/recommend-resource/v3");
        match fetch_product_json(
            &state,
            "zebra",
            extension_url,
            vec![("userId", user_id.clone())],
            "加载拓展推荐内容",
        )
        .await
        {
            Ok(value) => {
                let parsed = parse_product_content(&value, "zebra");
                content.entries.extend(parsed.entries);
            }
            Err(error) => errors.push(error),
        }

        // 2. 随身听/儿歌音频专辑列表
        let fm_url =
            format!("{ACCOUNT_HOST}/conan-english-song/android/users/{user_id}/albums/play-list");
        match fetch_product_json(
            &state,
            "zebra",
            fm_url,
            vec![("english", "false".into()), ("allSubject", "true".into())],
            "加载随身听专辑",
        )
        .await
        {
            Ok(value) => {
                let albums = parse_fm_album_categories(&value, "随身听");
                content.entries.extend(albums);
            }
            Err(error) => errors.push(error),
        }

        // 3. 有声故事专辑列表
        let story_url = format!(
            "{ACCOUNT_HOST}/conan-english-song/android/users/{user_id}/albums/story-play-list"
        );
        match fetch_product_json(&state, "zebra", story_url, vec![], "加载有声故事专辑").await
        {
            Ok(value) => {
                let albums = parse_fm_album_categories(&value, "有声故事");
                content.entries.extend(albums);
            }
            Err(error) => errors.push(error),
        }

        // 4. VIP课程包
        let packs_url =
            format!("{ACCOUNT_HOST}/conan-zsc-course/android/featuredCoursePackage/packs");
        match fetch_product_json(
            &state,
            "zebra",
            packs_url,
            vec![("userId", user_id.clone())],
            "加载VIP课程包",
        )
        .await
        {
            Ok(value) => {
                let pack_refs = featured_pack_refs(&value);
                if pack_refs.is_empty() {
                    errors.push(format!("VIP课程包返回字段暂未识别：{}", json_shape(&value)));
                }
                for (pack_id, cover_url) in pack_refs {
                    let detail_url = format!(
                        "{ACCOUNT_HOST}/conan-zsc-course/android/featuredCoursePackage/packDetail"
                    );
                    let detail = fetch_product_json(
                        &state,
                        "zebra",
                        detail_url,
                        vec![("userId", user_id.clone()), ("packId", pack_id.clone())],
                        "加载VIP课程包详情",
                    )
                    .await
                    .ok();
                    let title = detail
                        .as_ref()
                        .and_then(|value| json_find_string(value, &["name", "packName", "title"]))
                        .unwrap_or_else(|| format!("VIP课程包 {pack_id}"));
                    let subtitle = detail.as_ref().and_then(|value| {
                        json_find_string(
                            value,
                            &["courseNumDesc", "stageName", "subjectName", "description"],
                        )
                    });
                    content.entries.push(ContentEntry {
                        id: pack_id,
                        title,
                        subtitle,
                        cover_url,
                        kind: "pack".into(),
                        locked: false,
                        action_url: None,
                        parent_id: None,
                        has_detail: true,
                    });
                }
            }
            Err(error) => errors.push(error),
        }

        // 5. 学习课程
        let mission_url =
            format!("{ACCOUNT_HOST}/conan-mission/android/users/{user_id}/multi-subject-missions");
        match fetch_product_json(
            &state,
            "zebra",
            mission_url,
            vec![("cursor", String::new())],
            "加载学习课程",
        )
        .await
        {
            Ok(value) => {
                let parsed = parse_product_content(&value, "zebra");
                content.entries.extend(parsed.entries);
            }
            Err(error) => errors.push(error),
        }

        let mut entry_seen = HashSet::new();
        content
            .entries
            .retain(|e| entry_seen.insert(format!("{}:{}", e.kind, e.id)));
        if content.entries.is_empty() && content.videos.is_empty() {
            if errors.is_empty() {
                return Err("拓展接口已返回数据，但当前版本尚未识别出课程包或音频；请保留此提示用于继续适配".into());
            }
            return Err(format!("拓展内容读取失败：{}", errors.join("；")));
        }
        if !errors.is_empty() {
            content.warning = Some(errors.join("；"));
        }
        return Ok(content);
    }
    let (url, query) = match request.product.as_str() {
        "pedia" => (format!("{ACCOUNT_HOST}/conan-pedia-growth/android/learn/home-tab"), vec![]),
        "aioral" => (
            format!("{ACCOUNT_HOST}/conan-mission-oral/android/users/{user_id}/independent/phone/learn-data"),
            vec![("firstPage", "true".into())],
        ),
        _ => return Err("不支持的斑马产品".into()),
    };
    let value = fetch_product_json(&state, &request.product, url, query, "加载课程").await?;
    Ok(parse_product_content(&value, &request.product))
}

#[tauri::command]
pub(crate) async fn load_content_detail(
    state: State<'_, AppState>,
    request: ContentDetailRequest,
) -> Result<ProductContent, String> {
    let session = product_session(&state, &request.product)?;
    let user_id = session.user_id.unwrap_or_default();
    let id = request.entry_id.trim();
    if id.is_empty()
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
    {
        return Err("内容标识无效".into());
    }
    let parent = request.parent_id.unwrap_or_else(|| "0".into());
    let (url, query) = match (request.product.as_str(), request.entry_kind.as_str()) {
        ("pedia", "pack") => (
            format!("{ACCOUNT_HOST}/conan-zsc-course/android/encyclopedia/packDetail"),
            vec![("comeFrom", "0".into()), ("packId", id.into()), ("userId", user_id.clone())],
        ),
        ("pedia", "course") | ("pedia", "episode") => (
            format!("{ACCOUNT_HOST}/conan-pedia-course/android/encyclopedia-episodes/V2/{id}"),
            vec![("packId", parent), ("trial", "false".into())],
        ),
        ("aioral", "mission") | ("zebra", "mission") => (
            format!("{ACCOUNT_HOST}/conan-mission/android/users/{user_id}/missions/{id}"), vec![],
        ),
        ("zebra", "pack") => (
            format!("{ACCOUNT_HOST}/conan-zsc-course/android/featuredCoursePackage/packDetail"),
            vec![("userId", user_id.clone()), ("packId", id.into())],
        ),
        ("zebra", "episode") if parent.contains(':') => {
            let (pack_id, course_id) = parent.split_once(':').ok_or("VIP课节参数无效")?;
            (
                format!("{ACCOUNT_HOST}/conan-english-episode/android/featured-course-episodes/{id}"),
                vec![("packId", pack_id.into()), ("courseId", course_id.into())],
            )
        },
        ("aioral", "episode") | ("zebra", "episode") => (
            format!("{ACCOUNT_HOST}/conan-english-episode/android/episodes/{id}"), vec![("missionId", parent)],
        ),
        ("zebra", "album") | ("zebra", "song") | ("zebra", "audio") => {
            let songs_url = format!("{ACCOUNT_HOST}/conan-english-song/android/users/{user_id}/albums/{id}/songs");
            let mut songs_content = ProductContent { entries: Vec::new(), videos: Vec::new(), cursor: None, warning: None };
            let mut fetched_ok = false;
            for song_type in ["0", "2", "1"] {
                for english in ["false", "true"] {
                    if let Ok(value) = fetch_product_json(
                        &state,
                        "zebra",
                        songs_url.clone(),
                        vec![("english", english.into()), ("type", song_type.into())],
                        "加载专辑音频列表",
                    ).await {
                        let album_title = json_find_string(&value, &["albumName", "name", "title"]);
                        let parsed = parse_product_content_scoped(&value, "zebra", album_title.as_deref());
                        if !parsed.videos.is_empty() || !parsed.entries.is_empty() {
                            merge_product_content(&mut songs_content, parsed);
                            fetched_ok = true;
                        }
                    }
                }
            }
            if !fetched_ok {
                if let Ok(value) = fetch_product_json(
                    &state,
                    "zebra",
                    songs_url,
                    vec![],
                    "加载专辑音频列表",
                ).await {
                    let album_title = json_find_string(&value, &["albumName", "name", "title"]);
                    let parsed = parse_product_content_scoped(&value, "zebra", album_title.as_deref());
                    merge_product_content(&mut songs_content, parsed);
                }
            }
            return if songs_content.videos.is_empty() && songs_content.entries.is_empty() {
                Err("音频专辑已返回，但未识别到可下载的资源曲目".into())
            } else {
                Ok(songs_content)
            };
        },
        (_, "unit") if request.product == "aioral" => (
            format!("{ACCOUNT_HOST}/conan-mission-oral/android/users/{user_id}/independent/refresh-unit"),
            vec![("unitUniqId", id.into()), ("unitCardType", "1".into())],
        ),
        _ => {
            if let Some(action_url) = request.action_url.filter(|url| url.starts_with(ACCOUNT_HOST)) {
                (action_url, vec![])
            } else {
                return Ok(ProductContent {
                    entries: Vec::new(),
                    videos: Vec::new(),
                    cursor: None,
                    warning: Some("当前内容为分类卡片或未提供单独详情列表".into()),
                });
            }
        }
    };
    let value = fetch_product_json(&state, &request.product, url, query, "加载内容详情").await?;
    if request.product == "zebra" && request.entry_kind == "pack" {
        let mut parsed = parse_featured_pack_detail(&value, id, None);
        if parsed.entries.is_empty() {
            let albums_url = format!(
                "{ACCOUNT_HOST}/conan-zsc-course/android/featuredCoursePackage/pack-albums2"
            );
            if let Ok(albums) = fetch_product_json(
                &state,
                "zebra",
                albums_url,
                vec![("userId", user_id), ("packId", id.into())],
                "加载VIP课程专辑",
            )
            .await
            {
                parsed = parse_featured_pack_detail(&albums, id, None);
            }
        }
        if parsed.entries.is_empty() && parsed.videos.is_empty() {
            Err("VIP课程包已返回，但没有识别到课节或可下载媒体".into())
        } else {
            Ok(parsed)
        }
    } else {
        Ok(parse_product_content_scoped(
            &value,
            &request.product,
            request.entry_title.as_deref(),
        ))
    }
}

#[tauri::command]
pub(crate) async fn load_albums_resources(
    state: State<'_, AppState>,
    request: BatchAlbumRequest,
) -> Result<Vec<ResourceItem>, String> {
    let load_generation = state.album_load_generation.load(Ordering::Relaxed);
    let _started = std::time::Instant::now();
    debug_log!(
        "album resolve start product={} albums={} generation={}",
        request.product,
        request.album_ids.len(),
        load_generation
    );
    let session = product_session(&state, &request.product)?;
    let user_id = session.user_id.unwrap_or_default();
    let mut all_resources = Vec::new();
    let mut seen_urls = HashSet::new();

    for album_id in request.album_ids {
        if state.album_load_generation.load(Ordering::Relaxed) != load_generation {
            debug_log!("album resolve cancelled generation={}", load_generation);
            return Err("CANCELLED:已停止解析专辑".into());
        }
        let id = album_id.trim();
        if id.is_empty() {
            continue;
        }

        let mut album_resources = Vec::new();
        let requested_title = request
            .album_titles
            .get(id)
            .map(|title| title.trim())
            .filter(|title| !title.is_empty())
            .map(str::to_owned);

        // 1. 尝试作为随身听/故事专辑加载曲目
        if request.product == "zebra" {
            let songs_url = format!(
                "{ACCOUNT_HOST}/conan-english-song/android/users/{user_id}/albums/{id}/songs"
            );
            for song_type in ["0", "2", "1"] {
                for english in ["false", "true"] {
                    if state.album_load_generation.load(Ordering::Relaxed) != load_generation {
                        return Err("CANCELLED:已停止解析专辑".into());
                    }
                    if let Ok(value) = fetch_product_json(
                        &state,
                        "zebra",
                        songs_url.clone(),
                        vec![
                            ("english", english.to_string()),
                            ("type", song_type.to_string()),
                        ],
                        "加载专辑音频列表",
                    )
                    .await
                    {
                        let album_name = requested_title
                            .clone()
                            .or_else(|| json_find_string(&value, &["albumName", "name", "title"]))
                            .unwrap_or_else(|| format!("专辑_{id}"));
                        let parsed =
                            parse_product_content_scoped(&value, "zebra", Some(&album_name));
                        album_resources.extend(parsed.videos);
                    }
                }
            }
            if album_resources.is_empty() {
                if let Ok(value) =
                    fetch_product_json(&state, "zebra", songs_url, vec![], "加载专辑音频列表").await
                {
                    let album_name = requested_title
                        .clone()
                        .or_else(|| json_find_string(&value, &["albumName", "name", "title"]))
                        .unwrap_or_else(|| format!("专辑_{id}"));
                    let parsed = parse_product_content_scoped(&value, "zebra", Some(&album_name));
                    album_resources.extend(parsed.videos);
                }
            }
        }

        // 2. 尝试作为 VIP 课程包加载课节与视频
        if request.product == "zebra" {
            let pack_url =
                format!("{ACCOUNT_HOST}/conan-zsc-course/android/featuredCoursePackage/packDetail");
            if let Ok(value) = fetch_product_json(
                &state,
                "zebra",
                pack_url,
                vec![("userId", user_id.clone()), ("packId", id.to_string())],
                "加载VIP课程包详情",
            )
            .await
            {
                let pack_name = requested_title
                    .clone()
                    .or_else(|| json_find_string(&value, &["name", "packName", "title"]))
                    .unwrap_or_else(|| format!("VIP课程包_{id}"));
                let parsed = parse_featured_pack_detail(&value, id, Some(&pack_name));
                album_resources.extend(parsed.videos);
                for ep in parsed.entries {
                    if state.album_load_generation.load(Ordering::Relaxed) != load_generation {
                        return Err("CANCELLED:已停止解析专辑".into());
                    }
                    if let Some(parent) = ep.parent_id {
                        if let Some((pack_id, course_id)) = parent.split_once(':') {
                            let ep_url = format!("{ACCOUNT_HOST}/conan-english-episode/android/featured-course-episodes/{}", ep.id);
                            if let Ok(ep_val) = fetch_product_json(
                                &state,
                                "zebra",
                                ep_url,
                                vec![
                                    ("packId", pack_id.to_string()),
                                    ("courseId", course_id.to_string()),
                                ],
                                "加载VIP课节详情",
                            )
                            .await
                            {
                                let episode_path = format!("{pack_name}/{}", ep.title);
                                let ep_parsed = parse_product_content_scoped(
                                    &ep_val,
                                    "zebra",
                                    Some(&episode_path),
                                );
                                album_resources.extend(ep_parsed.videos);
                            }
                        }
                    }
                }
            }
        }

        // 3. 尝试作为学习任务加载
        if request.product == "aioral" || request.product == "zebra" {
            let mission_url =
                format!("{ACCOUNT_HOST}/conan-mission/android/users/{user_id}/missions/{id}");
            if let Ok(value) = fetch_product_json(
                &state,
                &request.product,
                mission_url,
                vec![],
                "加载任务详情",
            )
            .await
            {
                let mission_name = requested_title
                    .clone()
                    .or_else(|| json_find_string(&value, &["name", "missionName", "title"]))
                    .unwrap_or_else(|| format!("任务_{id}"));
                let parsed =
                    parse_product_content_scoped(&value, &request.product, Some(&mission_name));
                album_resources.extend(parsed.videos);
                for entry in parsed.entries {
                    if state.album_load_generation.load(Ordering::Relaxed) != load_generation {
                        return Err("CANCELLED:已停止解析专辑".into());
                    }
                    if entry.kind == "episode" {
                        let ep_url = format!(
                            "{ACCOUNT_HOST}/conan-english-episode/android/episodes/{}",
                            entry.id
                        );
                        if let Ok(ep_val) = fetch_product_json(
                            &state,
                            &request.product,
                            ep_url,
                            vec![("missionId", id.to_string())],
                            "加载课节详情",
                        )
                        .await
                        {
                            let episode_path = format!("{mission_name}/{}", entry.title);
                            let ep_parsed = parse_product_content_scoped(
                                &ep_val,
                                &request.product,
                                Some(&episode_path),
                            );
                            album_resources.extend(ep_parsed.videos);
                        }
                    }
                }
            }
        }

        // 4. 尝试作为百科课程包加载
        if request.product == "pedia" {
            let pack_url =
                format!("{ACCOUNT_HOST}/conan-zsc-course/android/encyclopedia/packDetail");
            if let Ok(value) = fetch_product_json(
                &state,
                "pedia",
                pack_url,
                vec![
                    ("comeFrom", "0".into()),
                    ("packId", id.to_string()),
                    ("userId", user_id.clone()),
                ],
                "加载百科课程包",
            )
            .await
            {
                let pedia_name = requested_title
                    .clone()
                    .or_else(|| json_find_string(&value, &["name", "packName", "title"]))
                    .unwrap_or_else(|| format!("百科课程_{id}"));
                let parsed = parse_product_content_scoped(&value, "pedia", Some(&pedia_name));
                album_resources.extend(parsed.videos);
                for entry in parsed.entries {
                    if state.album_load_generation.load(Ordering::Relaxed) != load_generation {
                        return Err("CANCELLED:已停止解析专辑".into());
                    }
                    let ep_url = format!(
                        "{ACCOUNT_HOST}/conan-pedia-course/android/encyclopedia-episodes/V2/{}",
                        entry.id
                    );
                    if let Ok(ep_val) = fetch_product_json(
                        &state,
                        "pedia",
                        ep_url,
                        vec![("packId", id.to_string()), ("trial", "false".into())],
                        "加载百科课节",
                    )
                    .await
                    {
                        let episode_path = format!("{pedia_name}/{}", entry.title);
                        let ep_parsed =
                            parse_product_content_scoped(&ep_val, "pedia", Some(&episode_path));
                        album_resources.extend(ep_parsed.videos);
                    }
                }
            }
        }

        for (resource_index, mut item) in album_resources.into_iter().enumerate() {
            if item.sequence.is_none() {
                item.sequence = Some(resource_index + 1);
            }
            // 保留解析出的课程/课节目录，并统一挂到用户选择的专辑目录下面。
            if let Some(title) = requested_title.as_ref() {
                let child = item.subfolder.as_deref().unwrap_or("").trim();
                item.subfolder = Some(
                    if child.is_empty() || child == title || child.starts_with(&format!("{title}/"))
                    {
                        if child.is_empty() {
                            title.clone()
                        } else {
                            child.to_string()
                        }
                    } else {
                        format!("{title}/{child}")
                    },
                );
            } else if item.subfolder.as_deref().is_none_or(str::is_empty) {
                item.subfolder = Some(format!("内容_{id}"));
            }
            // 同一 URL 可能是多个专辑共享的片头/绘本资源；跨专辑不能去重，否则后选
            // 专辑会少文件。ID 也带上目录作用域，避免进度与取消状态互相覆盖。
            let scoped_key = format!("{}\n{}", item.subfolder.as_deref().unwrap_or(""), item.url);
            item.id = hex::encode(Sha256::digest(scoped_key.as_bytes()))[..16].to_string();
            if seen_urls.insert(scoped_key) {
                all_resources.push(item);
            }
        }
    }

    debug_log!(
        "album resolve complete resources={} elapsed_ms={}",
        all_resources.len(),
        _started.elapsed().as_millis()
    );
    Ok(all_resources)
}
