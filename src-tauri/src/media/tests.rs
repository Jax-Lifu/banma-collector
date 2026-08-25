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
