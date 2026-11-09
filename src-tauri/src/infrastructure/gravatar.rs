use std::path::Path;
use std::time::{Duration, SystemTime};

use base64::Engine;
use md5::{Digest, Md5};

/// Loads a Gravatar for the account email and keeps the bytes on disk.
/// A missing image is remembered for a day so the snapshot does not ask again.
pub async fn load(http: &reqwest::Client, cache_dir: &Path, email: &str) -> String {
    let email = email.trim().to_lowercase();
    if !email.contains('@') {
        return String::new();
    }
    let hash = format!("{:x}", Md5::digest(email.as_bytes()));
    let file = cache_dir.join(format!("{hash}.img"));
    let miss = cache_dir.join(format!("{hash}.miss"));
    if fresh(&file, Duration::from_secs(7 * 24 * 60 * 60)) {
        if let Some(url) = read_image(&file) {
            return url;
        }
    }
    if fresh(&miss, Duration::from_secs(24 * 60 * 60)) {
        return String::new();
    }
    let url = format!("https://www.gravatar.com/avatar/{hash}?s=128&d=404");
    let Ok(response) = http.get(&url).timeout(Duration::from_secs(4)).send().await else {
        return read_image(&file).unwrap_or_default();
    };
    if !response.status().is_success() {
        let _ = std::fs::create_dir_all(cache_dir);
        let _ = std::fs::write(&miss, b"");
        return String::new();
    }
    let bytes = response.bytes().await.unwrap_or_default();
    let Some(data) = data_url(&bytes) else {
        let _ = std::fs::create_dir_all(cache_dir);
        let _ = std::fs::write(&miss, b"");
        return String::new();
    };
    let _ = std::fs::create_dir_all(cache_dir);
    let _ = std::fs::write(&file, &bytes);
    let _ = std::fs::remove_file(&miss);
    data
}

fn fresh(path: &Path, max_age: Duration) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    meta.modified()
        .ok()
        .and_then(|time| SystemTime::now().duration_since(time).ok())
        .is_some_and(|age| age < max_age)
}

fn read_image(path: &Path) -> Option<String> {
    data_url(&std::fs::read(path).ok()?)
}

fn data_url(bytes: &[u8]) -> Option<String> {
    let mime = if bytes.starts_with(b"\x89PNG") {
        "image/png"
    } else if bytes.starts_with(&[0xff, 0xd8]) {
        "image/jpeg"
    } else if bytes.starts_with(b"GIF8") {
        "image/gif"
    } else if bytes.len() > 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        "image/webp"
    } else {
        return None;
    };
    Some(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}
