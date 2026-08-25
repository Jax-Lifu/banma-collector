use super::*;
use aes::cipher::BlockEncrypt;

#[test]
fn unwraps_media_key_with_native_player_algorithm() {
    let kid = "0351b315c4574bd69d6b09f6e2cd09fd";
    let expected = hex::decode("00112233445566778899aabbccddeeff").unwrap();
    let mask = hex::decode(MEDIA_KEY_MASK).unwrap();
    let kid_bytes = hex::decode(kid).unwrap();
    let wrapping_key = kid_bytes
        .iter()
        .zip(mask.iter())
        .map(|(kid_byte, mask_byte)| kid_byte ^ mask_byte)
        .collect::<Vec<_>>();
    let cipher = Aes128::new_from_slice(&wrapping_key).unwrap();
    let mut encrypted = aes::Block::clone_from_slice(&expected);
    cipher.encrypt_block(&mut encrypted);

    assert_eq!(
        unwrap_media_key(kid, &hex::encode(encrypted)).unwrap(),
        hex::encode(expected)
    );
}

#[tokio::test(flavor = "current_thread")]
async fn playable_marker_is_migrated_to_root_cache() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let output_root = std::env::temp_dir().join(format!(
        "banma-playable-marker-{}-{unique}",
        std::process::id()
    ));
    let media_path = output_root.join("月球/认识月球/001_认识月球_中文_4K.mp4");
    fs::create_dir_all(media_path.parent().unwrap())
        .await
        .unwrap();
    fs::write(&media_path, b"test video placeholder")
        .await
        .unwrap();

    let legacy_marker = legacy_playable_marker_path(&media_path);
    fs::write(&legacy_marker, PLAYABLE_MARKER_VERSION)
        .await
        .unwrap();

    assert!(has_current_playable_marker(&output_root, &media_path).await);
    let cached_marker = playable_marker_path(&output_root, &media_path);
    assert!(cached_marker.starts_with(output_root.join(".banma-cache/playable")));
    assert_eq!(
        fs::read(&cached_marker).await.unwrap(),
        PLAYABLE_MARKER_VERSION
    );
    assert!(!legacy_marker.exists());

    remove_playable_marker(&output_root, &media_path).await;
    assert!(!cached_marker.exists());
    let _ = fs::remove_dir_all(&output_root).await;
}

#[tokio::test(flavor = "current_thread")]
async fn playable_marker_follows_legacy_filename_upgrade() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let output_root = std::env::temp_dir().join(format!(
        "banma-playable-move-{}-{unique}",
        std::process::id()
    ));
    let old_path = output_root.join("认识月球_中文_4K.mp4");
    let new_path = output_root.join("001_认识月球_中文_4K.mp4");

    write_playable_marker(&output_root, &old_path)
        .await
        .unwrap();
    move_playable_marker(&output_root, &old_path, &new_path).await;

    assert!(!has_current_playable_marker(&output_root, &old_path).await);
    assert!(has_current_playable_marker(&output_root, &new_path).await);
    let _ = fs::remove_dir_all(&output_root).await;
}

#[test]
fn video_target_directory_can_separate_languages() {
    let item = ResourceItem {
        id: "moon-cn".into(),
        title: "认识月球".into(),
        url: "https://cdn.example.com/moon-cn.mpd".into(),
        kind: "video".into(),
        extension: "mpd".into(),
        size: None,
        source: "pedia".into(),
        subfolder: Some("月球/认识月球".into()),
        sequence: Some(1),
        quality: Some("4K".into()),
        language: Some("中文".into()),
    };
    let output = Path::new("downloads");

    assert_eq!(
        resource_target_dir(output, &item, true),
        output.join("中文/月球/认识月球")
    );
    assert_eq!(
        resource_target_dir(output, &item, false),
        output.join("月球/认识月球")
    );
}

#[test]
fn unlabeled_video_keeps_the_course_directory() {
    let item = ResourceItem {
        id: "moon-default".into(),
        title: "认识月球".into(),
        url: "https://cdn.example.com/moon.mp4".into(),
        kind: "video".into(),
        extension: "mp4".into(),
        size: None,
        source: "pedia".into(),
        subfolder: Some("月球/认识月球".into()),
        sequence: Some(1),
        quality: None,
        language: None,
    };

    assert_eq!(
        resource_target_dir(Path::new("downloads"), &item, true),
        Path::new("downloads/月球/认识月球")
    );
}

#[tokio::test(flavor = "current_thread")]
#[ignore = "requires the live Banma CDN and ffmpeg"]
async fn live_dash_key_decrypts_real_manifest() {
    let manifest = "https://maple-online.fbcontent.cn/public/release/trans/app-CONAN_ZDT_ENCYCLOPEDIA_1080_ENCRYPT_DASH/451696ba-0824-42a9-b503-da112ed020bd/451696ba-0824-42a9-b503-da112ed020bd.mpd";
    let client = reqwest::Client::new();
    let (key, avc_stream_index) = manifest_decryption_context(&client, manifest)
        .await
        .unwrap();
    let key = key.expect("encrypted manifest");
    let avc_stream_index = avc_stream_index.expect("AVC representation");
    let output_path = std::env::temp_dir().join("斑马-中文路径-live-dash-check.mp4");
    let _ = fs::remove_file(&output_path).await;
    let download = Command::new(crate::runtime_tools::command("ffmpeg"))
        .args([
            "-nostdin",
            "-y",
            "-hide_banner",
            "-loglevel",
            "error",
            "-cenc_decryption_key",
            &key,
            "-i",
            manifest,
            "-map",
            &format!("0:v:{avc_stream_index}"),
            "-map",
            "0:a:0?",
            "-t",
            "8",
            "-c",
            "copy",
        ])
        .arg(&output_path)
        .output()
        .await
        .unwrap();
    assert!(
        download.status.success(),
        "{}",
        String::from_utf8_lossy(&download.stderr)
    );
    // 回归 Bento4 在 Windows 下无法直接打开中文专辑路径的问题。
    prepare_playable_video(&client, &output_path).await.unwrap();
    let probe = Command::new(crate::runtime_tools::command("ffprobe"))
        .args([
            "-v",
            "error",
            "-show_entries",
            "stream=codec_name,codec_type,width,height",
            "-of",
            "compact=p=0:nk=1",
        ])
        .arg(&output_path)
        .output()
        .await
        .unwrap();
    assert!(probe.status.success());
    eprintln!("{}", String::from_utf8_lossy(&probe.stdout));
    let _ = fs::remove_file(&output_path).await;
}
