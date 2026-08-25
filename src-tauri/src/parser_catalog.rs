use super::*;

pub(crate) fn parse_product_content(value: &serde_json::Value, product: &str) -> ProductContent {
    parse_product_content_scoped(value, product, None)
}

pub(crate) fn parse_fm_album_categories(
    value: &serde_json::Value,
    default_category: &str,
) -> Vec<ContentEntry> {
    let mut entries = Vec::new();
    let mut seen_ids = HashSet::new();

    fn extract_albums(
        cat_name: &str,
        cat_val: &serde_json::Value,
        entries: &mut Vec<ContentEntry>,
        seen_ids: &mut HashSet<String>,
    ) {
        if let Some(albums) = cat_val.get("albums").and_then(|v| v.as_array()) {
            for album in albums {
                if let Some(album_map) = album.as_object() {
                    let id = map_string(album_map, &["id", "albumId"]).unwrap_or_default();
                    if id.is_empty() || !seen_ids.insert(id.clone()) {
                        continue;
                    }
                    let name = map_string(album_map, &["name", "title", "albumName"])
                        .unwrap_or_else(|| format!("音频专辑 {id}"));
                    let song_count = album_map
                        .get("songCount")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0);
                    let cover_url =
                        map_string(album_map, &["coverImageUrl", "coverUrl", "imageUrl"]);
                    let subtitle = if song_count > 0 {
                        Some(format!("{cat_name} · {song_count} 首音频"))
                    } else {
                        Some(cat_name.to_string())
                    };
                    entries.push(ContentEntry {
                        id,
                        title: name,
                        subtitle,
                        cover_url,
                        kind: "album".into(),
                        locked: false,
                        action_url: None,
                        parent_id: None,
                        has_detail: true,
                    });
                }
            }
        }
    }

    fn walk_categories(
        val: &serde_json::Value,
        default_cat: &str,
        entries: &mut Vec<ContentEntry>,
        seen_ids: &mut HashSet<String>,
    ) {
        match val {
            serde_json::Value::Object(map) => {
                if let Some(name) = map_string(map, &["name", "categoryName", "title"]) {
                    if map.contains_key("albums") {
                        extract_albums(&name, val, entries, seen_ids);
                        return;
                    }
                }
                for (key, child) in map {
                    let cat_label = if key.contains("english") {
                        "英文随身听"
                    } else if key.contains("chinese") {
                        "中文随身听"
                    } else if key.contains("story") {
                        "有声故事"
                    } else {
                        default_cat
                    };
                    if let Some(cat_array) = child.as_array() {
                        for item in cat_array {
                            let item_name = map_string(
                                item.as_object().unwrap_or(&serde_json::Map::new()),
                                &["name", "title"],
                            )
                            .unwrap_or_else(|| cat_label.to_string());
                            extract_albums(&item_name, item, entries, seen_ids);
                        }
                    } else {
                        walk_categories(child, default_cat, entries, seen_ids);
                    }
                }
            }
            serde_json::Value::Array(arr) => {
                for item in arr {
                    walk_categories(item, default_cat, entries, seen_ids);
                }
            }
            _ => {}
        }
    }

    walk_categories(value, default_category, &mut entries, &mut seen_ids);
    entries
}

pub(crate) fn merge_product_content(target: &mut ProductContent, source: ProductContent) {
    target.entries.extend(source.entries);
    target.videos.extend(source.videos);
    if target.cursor.is_none() {
        target.cursor = source.cursor;
    }
    if target.warning.is_none() {
        target.warning = source.warning;
    }
    let mut entry_seen = HashSet::new();
    target
        .entries
        .retain(|entry| entry_seen.insert(format!("{}:{}", entry.kind, entry.id)));
    let mut media_seen = HashSet::new();
    target
        .videos
        .retain(|item| media_seen.insert(item.url.clone()));
}

pub(crate) fn featured_pack_refs(value: &serde_json::Value) -> Vec<(String, Option<String>)> {
    fn walk(value: &serde_json::Value, result: &mut Vec<(String, Option<String>)>) {
        match value {
            serde_json::Value::Array(values) => values.iter().for_each(|value| walk(value, result)),
            serde_json::Value::Object(map) => {
                if let Some(id) = map_string(map, &["packId", "featuredCoursePackageId"]) {
                    result.push((
                        id,
                        map_string(map, &["imageUrl", "coverUrl", "coverImageUrl"]),
                    ));
                }
                map.values().for_each(|value| walk(value, result));
            }
            _ => {}
        }
    }
    let mut result = Vec::new();
    walk(value, &mut result);
    let mut seen = HashSet::new();
    result.retain(|(id, _)| seen.insert(id.clone()));
    result
}

pub(crate) fn parse_featured_pack_detail(
    value: &serde_json::Value,
    fallback_pack_id: &str,
    pack_title: Option<&str>,
) -> ProductContent {
    fn walk(value: &serde_json::Value, fallback_pack_id: &str, entries: &mut Vec<ContentEntry>) {
        match value {
            serde_json::Value::Array(values) => values
                .iter()
                .for_each(|value| walk(value, fallback_pack_id, entries)),
            serde_json::Value::Object(map) => {
                if let (Some(episode_id), Some(course_id)) = (
                    map_string(map, &["episodeId"]),
                    map_string(map, &["courseId"]),
                ) {
                    let pack_id =
                        map_string(map, &["packId"]).unwrap_or_else(|| fallback_pack_id.to_owned());
                    entries.push(ContentEntry {
                        id: episode_id,
                        title: map_string(map, &["name", "title", "courseName"])
                            .unwrap_or_else(|| format!("课节 {course_id}")),
                        subtitle: map_string(map, &["unlockToast", "description", "desc"]),
                        cover_url: map_string(map, &["imageUrl", "coverUrl", "coverImageUrl"]),
                        kind: "episode".into(),
                        locked: map.get("valid").and_then(|value| value.as_bool()) == Some(false)
                            || map.get("locked").and_then(|value| value.as_bool()) == Some(true),
                        action_url: None,
                        parent_id: Some(format!("{pack_id}:{course_id}")),
                        has_detail: true,
                    });
                }
                map.values()
                    .for_each(|value| walk(value, fallback_pack_id, entries));
            }
            _ => {}
        }
    }
    let generic = parse_product_content_scoped(value, "zebra", pack_title);
    let mut entries = Vec::new();
    walk(value, fallback_pack_id, &mut entries);
    let mut seen = HashSet::new();
    entries.retain(|entry| seen.insert(entry.id.clone()));
    ProductContent {
        entries,
        videos: generic.videos,
        cursor: generic.cursor,
        warning: generic.warning,
    }
}

pub(crate) fn json_shape(value: &serde_json::Value) -> String {
    fn keys(value: &serde_json::Value) -> Vec<String> {
        value
            .as_object()
            .map(|map| map.keys().take(16).cloned().collect())
            .unwrap_or_default()
    }
    let mut result = keys(value);
    if let Some(data) = value.get("data").or_else(|| value.get("result")) {
        result.extend(keys(data).into_iter().map(|key| format!("data.{key}")));
    }
    if result.is_empty() {
        value.to_string().chars().take(120).collect()
    } else {
        result.join(", ")
    }
}
