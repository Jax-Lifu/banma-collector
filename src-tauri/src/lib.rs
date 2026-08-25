use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use futures_util::StreamExt;
use rand::thread_rng;
use regex::Regex;
use reqwest::cookie::CookieStore;
use rsa::{pkcs8::DecodePublicKey, Pkcs1v15Encrypt, RsaPublicKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
};

use tauri::{Emitter, Manager, State};
use tauri_plugin_opener::OpenerExt;
use tokio::{
    fs,
    io::{AsyncReadExt, AsyncWriteExt},
    process::Command,
    sync::Semaphore,
};

// Debug 构建的下载诊断日志直接写入运行 `pnpm tauri dev` 的终端。
// Release 构建会在编译期移除，且任何密钥、Cookie 都不得传入该宏。
#[cfg(debug_assertions)]
macro_rules! debug_log {
    ($($arg:tt)*) => {{
        eprintln!("[banma-debug][{}] {}", module_path!(), format_args!($($arg)*));
    }};
}

#[cfg(not(debug_assertions))]
macro_rules! debug_log {
    ($($arg:tt)*) => {};
}

mod api;
mod auth;
mod catalog;
mod download_cancel;
mod downloads;
mod filesystem;
mod media;
mod parser;
mod parser_catalog;
mod runtime_tools;
mod secure_storage;
mod state;

use api::*;
use media::*;
use parser::*;
use parser_catalog::*;
use state::*;

const ACCOUNT_HOST: &str = "https://conan.yuanfudao.com";
const MEDIA_KEY_HOST: &str = "https://maple.yuanfudao.com";
const LOGIN_PUBLIC_KEY: &str = "MIGfMA0GCSqGSIb3DQEBAQUAA4GNADCBiQKBgQDSovT1rrwzrGoMCFb6z8e+5lzVdAD5o8krGIwdfxrVE2OnMijUZdkQk7etPJvZ2JOVXghthAGUUJkDUE8n2ZMNFKPjMrQJI49ewVzqWOKOvgU6Iu60Sn0xpeietP1wWXBkszdV1WfNBJUo2hhPDnIPMGzzdfLW5rMu+tczeUriJQIDAQAB";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SmsRequest {
    phone: String,
    product: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LoginRequest {
    phone: String,
    code: String,
    product: String,
}

#[derive(Debug, Deserialize)]
struct ProductRequest {
    product: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ContentDetailRequest {
    product: String,
    entry_id: String,
    entry_title: Option<String>,
    entry_kind: String,
    parent_id: Option<String>,
    action_url: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BatchAlbumRequest {
    product: String,
    album_ids: Vec<String>,
    #[serde(default)]
    album_titles: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ContentEntry {
    id: String,
    title: String,
    subtitle: Option<String>,
    cover_url: Option<String>,
    kind: String,
    locked: bool,
    action_url: Option<String>,
    parent_id: Option<String>,
    has_detail: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProductContent {
    entries: Vec<ContentEntry>,
    videos: Vec<ResourceItem>,
    cursor: Option<String>,
    warning: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceItem {
    id: String,
    title: String,
    url: String,
    kind: String,
    extension: String,
    size: Option<u64>,
    source: String,
    subfolder: Option<String>,
    #[serde(default)]
    sequence: Option<usize>,
    #[serde(default)]
    quality: Option<String>,
    #[serde(default)]
    language: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadRequest {
    items: Vec<ResourceItem>,
    output_dir: String,
    concurrency: usize,
    product: String,
    #[serde(default)]
    separate_languages: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PreviewPathRequest {
    item: ResourceItem,
    output_dir: String,
    #[serde(default)]
    separate_languages: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelRequest {
    pub id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DownloadProgress {
    id: String,
    status: String,
    received: u64,
    total: Option<u64>,
    error: Option<String>,
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::*;

    #[test]
    fn dash_manifest_is_kept_as_manifest_until_download() {
        let (kind, extension) = extract_extension_and_kind(
            "https://cdn.example.com/course/video.mpd?token=abc",
            "playUrl",
        );
        assert_eq!(kind, "video");
        assert_eq!(extension, "mpd");

        let item = ResourceItem {
            id: "resource-id".into(),
            title: "课程视频.mpd".into(),
            url: "https://cdn.example.com/course/video.mpd".into(),
            kind: kind.into(),
            extension,
            size: None,
            source: "test".into(),
            subfolder: Some("专辑/A".into()),
            sequence: Some(7),
            quality: None,
            language: None,
        };
        assert!(safe_filename(&item).starts_with("007_"));
        assert!(safe_filename(&item).ends_with(".mp4"));
        assert_eq!(
            safe_folder_name(item.subfolder.as_deref().unwrap()),
            "专辑_A"
        );
        assert_eq!(
            safe_subfolder_path("专辑/子课程"),
            PathBuf::from("专辑").join("子课程")
        );
    }

    #[test]
    fn album_resources_keep_api_order_in_filenames() {
        let value = serde_json::json!({
            "songs": [
                { "title": "第一集", "audioUrl": "https://cdn.example.com/one.mp3" },
                { "title": "第二集", "audioUrl": "https://cdn.example.com/two.mp3" }
            ]
        });
        let parsed = parse_product_content_scoped(&value, "zebra", Some("测试专辑"));
        let audio = parsed
            .videos
            .into_iter()
            .filter(|item| item.kind == "audio")
            .collect::<Vec<_>>();
        assert_eq!(audio.len(), 2);
        assert_eq!(audio[0].sequence, Some(1));
        assert_eq!(audio[1].sequence, Some(2));
        assert!(safe_filename(&audio[0]).starts_with("001_第一集_"));
        assert!(safe_filename(&audio[1]).starts_with("002_第二集_"));
    }

    #[test]
    fn media_title_does_not_create_a_double_extension() {
        let item = ResourceItem {
            id: "70444ee9ae47768b".into(),
            title: "30f861b9-ae26-49ce-959b-e7e60753e62b.mp4".into(),
            url: "https://cdn.example.com/video.mp4".into(),
            kind: "video".into(),
            extension: "mp4".into(),
            size: None,
            source: "test".into(),
            subfolder: None,
            sequence: Some(1),
            quality: None,
            language: None,
        };
        assert_eq!(
            safe_filename(&item),
            "001_30f861b9-ae26-49ce-959b-e7e60753e62b_70444ee9ae47768b.mp4"
        );
    }

    #[test]
    fn pedia_video_variants_keep_one_source_per_quality() {
        let value = serde_json::json!({
            "episodes": [{
                "name": "造纸术和印刷术",
                "episodeId": "768",
                "primaryVideo": {
                    "encryptDashUdVideoUrl": "https://cdn.example.com/b3460b9b-81db-4c3a-b0c7-db7fcc857cda.mpd",
                    "encryptUhdVideoUrl": "https://cdn.example.com/734432bf-065e-4330-a0af-0e065877ede2.mpd",
                    "sdVideoUrl": "https://cdn.example.com/58163c98-0e61-4aed-ac10-29d465f4b904.mp4",
                    "udVideoUrl": "https://cdn.example.com/8c316ea6-5e32-4222-8507-832a0cebb725.mp4",
                    "videoUrl": "https://cdn.example.com/75f3f591-b351-4173-91e3-a349c6e894c1.mp4"
                },
                "legacyVideo": {
                    "encryptUhdVideoUrl": "https://cdn.example.com/9ea1c79a-0710-44f7-9bab-74cf6473c78e.mp4",
                    "hdVideoUrl": "https://cdn.example.com/75f3f591-b351-4173-91e3-a349c6e894c1.mp4",
                    "udVideoUrl": "https://cdn.example.com/8c316ea6-5e32-4222-8507-832a0cebb725.mp4",
                    "videoUrl": "https://cdn.example.com/58163c98-0e61-4aed-ac10-29d465f4b904.mp4"
                }
            }]
        });
        let parsed = parse_product_content_scoped(&value, "pedia", Some("四大发明/造纸术和印刷术"));
        let lesson_videos = parsed
            .videos
            .into_iter()
            .filter(|item| item.kind == "video")
            .collect::<Vec<_>>();
        assert_eq!(lesson_videos.len(), 4);
        let qualities = lesson_videos
            .iter()
            .map(|item| item.quality.as_deref().unwrap_or("默认"))
            .collect::<HashSet<_>>();
        assert_eq!(qualities, HashSet::from(["4K", "1080P", "720P", "标清"]));
        let full_hd = lesson_videos
            .iter()
            .find(|item| item.quality.as_deref() == Some("1080P"))
            .expect("1080P video");
        assert_eq!(
            full_hd.url,
            "https://cdn.example.com/b3460b9b-81db-4c3a-b0c7-db7fcc857cda.mpd"
        );
        assert!(lesson_videos
            .iter()
            .all(|item| item.title == "造纸术和印刷术"));
    }

    #[test]
    fn pedia_chinese_and_english_videos_keep_language_in_filename() {
        let value = serde_json::json!({
            "chapters": [{
                "contents": [{
                    "title": "课程视频",
                    "encryptDashUdVideoUrl": "https://cdn.example.com/moon-cn.mpd"
                }],
                "englishCourseContents": [{
                    "title": "课程视频",
                    "videoUrl": "https://cdn.example.com/moon-en.mp4"
                }]
            }]
        });

        let parsed = parse_product_content_scoped(&value, "pedia", Some("认识月球"));
        let videos = parsed
            .videos
            .iter()
            .filter(|item| item.kind == "video")
            .collect::<Vec<_>>();

        assert_eq!(videos.len(), 2);
        let chinese = videos
            .iter()
            .find(|item| item.language.as_deref() == Some("中文"))
            .expect("Chinese video");
        let english = videos
            .iter()
            .find(|item| item.language.as_deref() == Some("英文"))
            .expect("English video");
        assert_eq!(chinese.quality.as_deref(), Some("1080P"));
        assert!(safe_filename(chinese).contains("认识月球_中文_1080P_"));
        assert!(safe_filename(english).contains("认识月球_英文_"));
    }

    #[test]
    fn pedia_runtime_images_are_not_downloadable_courseware() {
        let value = serde_json::json!({
            "episodeId": "768",
            "name": "造纸术和印刷术",
            "openingAnimationImageUrl": "https://cdn.example.com/opening.jpg",
            "vipGuideInfo": {
                "imageUrl": "https://cdn.example.com/member-benefits.png"
            },
            "cover": {
                "imageUrl": "https://cdn.example.com/cover.jpg"
            },
            "coursewareImageUrl": "https://cdn.example.com/paper-making-handout.png",
            "videoPreviewFrames": [
                {
                    "imageUrl": "https://cdn.example.com/f04c5a48-17aa-490c-9df4-6298d978e5f5-00001.jpg",
                    "offset": 10000
                },
                {
                    "imageUrl": "https://cdn.example.com/f04c5a48-17aa-490c-9df4-6298d978e5f5-00002.jpg",
                    "offset": "20000"
                }
            ]
        });

        let parsed = parse_product_content_scoped(&value, "pedia", Some("四大发明/造纸术和印刷术"));
        let image_urls = parsed
            .videos
            .iter()
            .filter(|item| item.kind == "image")
            .map(|item| item.url.as_str())
            .collect::<Vec<_>>();

        assert_eq!(
            image_urls,
            vec!["https://cdn.example.com/paper-making-handout.png"]
        );
        assert!(!image_urls.contains(&"https://cdn.example.com/opening.jpg"));
        assert!(!image_urls.contains(&"https://cdn.example.com/member-benefits.png"));
        assert!(!image_urls.contains(&"https://cdn.example.com/cover.jpg"));
        assert!(!image_urls.iter().any(|url| url.contains("-00001.jpg")));
        assert!(!image_urls.iter().any(|url| url.contains("-00002.jpg")));
    }

    #[test]
    fn pedia_unity_dat_is_not_a_downloadable_course_resource() {
        let value = serde_json::json!({
            "chapters": [{
                "contents": [{
                    "title": "恐龙的外表",
                    "unityResources": [{
                        "unityResourceUrl": "https://cdn.example.com/dinosaur-model.dat",
                        "interactionTypes": [1, 2, 3],
                        "hasClickZone": true
                    }],
                    "videoUrl": "https://cdn.example.com/dinosaur.mp4"
                }],
                "englishCourseContents": []
            }]
        });

        let parsed = parse_product_content_scoped(&value, "pedia", Some("恐龙/恐龙的外表"));
        assert!(parsed.videos.iter().all(|item| item.extension != "dat"));
        assert!(parsed
            .videos
            .iter()
            .any(|item| item.url.ends_with("dinosaur.mp4")));
    }

    #[test]
    fn pedia_medal_popup_audio_is_not_course_audio() {
        let value = serde_json::json!({
            "packId": 76,
            "name": "微生物",
            "courseNarrationAudioUrl": "https://cdn.example.com/course.mp3",
            "pop": {
                "popType": 7,
                "medalPopInfo": {
                    "medalName": "菌菌博士",
                    "seriesName": "自然",
                    "audioUrl": "https://cdn.example.com/b0clzigfo.mp3"
                }
            }
        });

        let parsed = parse_product_content_scoped(&value, "pedia", Some("微生物"));
        let audio_urls = parsed
            .videos
            .iter()
            .filter(|item| item.kind == "audio")
            .map(|item| item.url.as_str())
            .collect::<Vec<_>>();

        assert_eq!(audio_urls, vec!["https://cdn.example.com/course.mp3"]);
        assert!(!audio_urls.iter().any(|url| url.contains("b0clzigfo")));
    }

    #[test]
    fn pedia_recommendations_are_not_part_of_the_selected_pack() {
        let value = serde_json::json!({
            "packId": "pack-four-inventions",
            "name": "四大发明",
            "courses": [{
                "courseId": "paper-making",
                "courseName": "造纸术和印刷术",
                "videoUrl": "https://cdn.example.com/paper-making.mp4"
            }],
            "recommendEncyclopedia": {
                "packId": "pack-human-body",
                "name": "认识人体",
                "coverImageUrl": "https://cdn.example.com/human-body-cover.jpg",
                "videoUrl": "https://cdn.example.com/human-body-preview.mp4"
            },
            "recommendVO": {
                "imageUrl": "https://cdn.example.com/recommend-card.png"
            }
        });

        let parsed = parse_product_content_scoped(&value, "pedia", Some("四大发明"));

        assert!(parsed.entries.iter().any(|entry| entry.title == "四大发明"));
        assert!(parsed
            .entries
            .iter()
            .any(|entry| entry.title == "造纸术和印刷术"));
        assert!(!parsed.entries.iter().any(|entry| entry.title == "认识人体"));
        assert!(parsed
            .videos
            .iter()
            .any(|item| item.url.ends_with("paper-making.mp4")));
        assert!(!parsed
            .videos
            .iter()
            .any(|item| item.url.contains("human-body") || item.url.contains("recommend-card")));
    }

    #[test]
    fn pedia_app_animation_zip_is_not_a_course_attachment() {
        let value = serde_json::json!({
            "courseId": "paper-making",
            "courseName": "造纸术和印刷术",
            "openingAnimationUrl": "https://cdn.example.com/player-loading-animation.zip",
            "transAnimationVideoUrl": "https://cdn.example.com/chapter-transition.mp4",
            "transAnimationAudioUrl": "https://cdn.example.com/chapter-transition.mp3",
            "runtimeBundles": [
                {
                    "name": "阿根廷龙",
                    "url": "https://cdn.example.com/argentinosaurus.zip"
                },
                {
                    "platform": "Android",
                    "resourceUrl": "https://cdn.example.com/Android.zip"
                }
            ],
            "attachmentUrl": "https://cdn.example.com/course-materials.zip"
        });

        let parsed = parse_product_content_scoped(&value, "pedia", Some("四大发明/造纸术和印刷术"));

        assert!(!parsed
            .videos
            .iter()
            .any(|item| item.url.ends_with("player-loading-animation.zip")));
        assert!(!parsed
            .videos
            .iter()
            .any(|item| item.url.contains("chapter-transition")));
        assert!(!parsed.videos.iter().any(|item| {
            item.url.ends_with("argentinosaurus.zip") || item.url.ends_with("Android.zip")
        }));
        assert!(parsed
            .videos
            .iter()
            .any(|item| item.url.ends_with("course-materials.zip")));
    }

    #[test]
    fn pedia_interactive_question_audio_is_not_standalone_course_audio() {
        let value = serde_json::json!({
            "courseId": "dinosaur",
            "courseName": "探秘恐龙世界",
            "narrationAudioUrl": "https://cdn.example.com/course-narration.mp3",
            "encyQuestion": {
                "cardQuestionContent": {
                    "openingAudioUrl": "https://cdn.example.com/question-opening.mp3",
                    "card": {
                        "introductionAudioUrl": "https://cdn.example.com/card-introduction.mp3",
                        "encyEntryAudios": [
                            { "audioUrl": "https://cdn.example.com/entry-pronunciation.mp3" }
                        ]
                    },
                    "items": [
                        { "feedbackAudioUrl": "https://cdn.example.com/correct-feedback.mp3" }
                    ]
                }
            }
        });

        let parsed = parse_product_content_scoped(&value, "pedia", Some("恐龙/探秘恐龙世界"));
        let audio_urls = parsed
            .videos
            .iter()
            .filter(|item| item.kind == "audio")
            .map(|item| item.url.as_str())
            .collect::<Vec<_>>();

        assert_eq!(
            audio_urls,
            vec!["https://cdn.example.com/course-narration.mp3"]
        );
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState::new())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            auth::request_sms,
            auth::phone_login,
            auth::login_session,
            auth::logout,
            catalog::load_product_catalog,
            catalog::load_content_detail,
            catalog::load_albums_resources,
            downloads::download_resources,
            downloads::cancel_download,
            filesystem::reveal_path,
            filesystem::resource_preview_path
        ])
        .setup(|app| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_title("斑马资源库");
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
