//! Release checking and archive download/extraction.

use crate::paths::install_root;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::Emitter;

#[cfg(target_os = "macos")]
pub const RELEASE_API: &str = "https://api.github.com/repos/gootecks/3sxtra/releases/tags/macos-rolling";
#[cfg(not(target_os = "macos"))]
pub const RELEASE_API: &str = "https://api.github.com/repos/gootecks/3sxtra/releases/tags/rolling-pre-release";

#[derive(Deserialize, Debug)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Deserialize, Debug)]
#[allow(dead_code)]
struct GitHubRelease {
    name: String,
    created_at: String,
    assets: Vec<GitHubAsset>,
}

#[derive(Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveTask {
    name: String,
    url: String,
    extract_path: String,
    marker_file: String,
    strip_root: bool,
    force_update: bool,
    version_id: Option<String>,
}

#[derive(Deserialize, Serialize, Clone)]
pub struct UpdateManifest {
    version: String,
    archives: Option<Vec<ArchiveTask>>,
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
/// Marker the macOS engine install must produce, relative to install_root().
pub const MAC_ENGINE_MARKER: &str = "3sx.app/Contents/MacOS/3sx";

/// `3SX-<sha>-macos-universal.zip` → `Some(sha)`. Launcher (`3SXtra-…`) and
/// `.dmg` assets are rejected.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn mac_engine_asset_sha(name: &str) -> Option<&str> {
    name.strip_prefix("3SX-")?
        .strip_suffix("-macos-universal.zip")
        .filter(|s| !s.is_empty())
}

/// Short shas of different lengths match by prefix.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn same_sha(a: &str, b: &str) -> bool {
    !a.is_empty() && !b.is_empty() && (a.starts_with(b) || b.starts_with(a))
}

#[tauri::command]
pub async fn check_updates() -> Result<Option<UpdateManifest>, String> {
    let client = reqwest::Client::builder()
        .user_agent("3SX-Launcher")
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client.get(RELEASE_API).send().await.map_err(|e| format!("Failed to fetch release: {}", e))?;
    if !resp.status().is_success() {
        return Ok(None);
    }
    let release: GitHubRelease = resp.json().await.map_err(|e| format!("Invalid release JSON: {}", e))?;

    let root = install_root();
    let local_version = std::fs::read_to_string(root.join("launcher_version.txt")).unwrap_or_default();
    let mut archives = vec![];

    // ── macOS: one self-contained 3sx.app zip, installed to <pref>/engine ──
    #[cfg(target_os = "macos")]
    if let Some(asset) = release.assets.iter().find(|a| mac_engine_asset_sha(&a.name).is_some()) {
        let remote_sha = mac_engine_asset_sha(&asset.name).unwrap_or_default();
        let current_sha = crate::engine::find_engine()
            .and_then(|app| crate::engine::read_engine_version(&app))
            .and_then(|v| v.sha)
            .unwrap_or_default();
        let up_to_date = local_version.trim() == release.created_at.trim() || same_sha(&current_sha, remote_sha);
        if !up_to_date {
            archives.push(ArchiveTask {
                name: "3SX Core Engine".to_string(),
                url: asset.browser_download_url.clone(),
                extract_path: ".".to_string(),
                marker_file: MAC_ENGINE_MARKER.to_string(),
                strip_root: false,
                force_update: true,
                version_id: Some(release.created_at.clone()),
            });
        }
    }

    // ── Asset Pack (Windows/Linux; the macOS app bundle carries its assets) ──
    // The asset zip is named like "3SX-assets-v1-3a760a0.zip".
    // Compare the "vN" in the filename against local assets/ASSET_VERSION.
    #[cfg(not(target_os = "macos"))]
    {
        let local_asset_version = std::fs::read_to_string(root.join("assets").join("ASSET_VERSION"))
            .unwrap_or_default()
            .trim()
            .to_string();

        if let Some(asset_zip) = release.assets.iter().find(|a| a.name.contains("assets") && a.name.ends_with(".zip")) {
            // Extract version from filename: "3SX-assets-v1-abc1234.zip" -> "1"
            let remote_asset_version = asset_zip.name
                .split("-v")
                .nth(1)
                .and_then(|s| s.split('-').next())
                .unwrap_or("0")
                .to_string();

            if local_asset_version.is_empty() || local_asset_version != remote_asset_version {
                archives.push(ArchiveTask {
                    name: "3SX Asset Pack".to_string(),
                    url: asset_zip.browser_download_url.clone(),
                    extract_path: ".".to_string(), // zip already contains assets/ folder
                    marker_file: "assets/ASSET_VERSION".to_string(),
                    strip_root: false, // zip is structured as assets/... already
                    force_update: true,
                    version_id: None, // version tracked by ASSET_VERSION file inside the zip
                });
            }
        }

        // ── Engine Binary ──────────────────────────────────────────
        if local_version.trim() != release.created_at.trim() {
            let os_str = if cfg!(target_os = "windows") { "windows" } else { "linux" };
            if let Some(asset) = release.assets.iter().find(|a| {
                a.name.contains(os_str) && !a.name.contains("Launcher") &&
                (a.name.ends_with(".zip") || a.name.ends_with(".tar.gz"))
            }) {
                archives.push(ArchiveTask {
                    name: "3SX Core Engine".to_string(),
                    url: asset.browser_download_url.clone(),
                    extract_path: ".".to_string(),
                    marker_file: if cfg!(target_os = "windows") { "3sx.exe".to_string() } else { "3sx".to_string() },
                    strip_root: true, // The release archive has a top-level directory
                    force_update: true,
                    version_id: Some(release.created_at.clone()),
                });
            }
        }
    }

    Ok(Some(UpdateManifest {
        version: release.created_at,
        archives: Some(archives),
    }))
}

fn run_tool(cmd: &str, args: &[&std::ffi::OsStr]) -> Result<(), String> {
    let status = std::process::Command::new(cmd)
        .args(args)
        .status()
        .map_err(|e| format!("Failed to run {}: {}", cmd, e))?;
    if status.success() { Ok(()) } else { Err(format!("{} exited with {}", cmd, status)) }
}

/// macOS: unpack a `ditto`-made app zip into `dest`, replacing what's there.
/// `ditto` keeps exec bits, symlinks and framework layout; the quarantine
/// attribute is stripped so Gatekeeper doesn't translocate the engine.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn install_app_zip(zip: &Path, dest: &Path) -> Result<(), String> {
    let staging = dest.join(".staging");
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging).map_err(|e| e.to_string())?;
    run_tool("/usr/bin/ditto", &["-x".as_ref(), "-k".as_ref(), zip.as_os_str(), staging.as_os_str()])?;

    // The app is at the zip root (ditto --keepParent) or one folder down.
    let app: PathBuf = std::iter::once(staging.join("3sx.app"))
        .chain(
            std::fs::read_dir(&staging)
                .map_err(|e| e.to_string())?
                .flatten()
                .map(|e| e.path().join("3sx.app")),
        )
        .find(|a| crate::engine::engine_binary(a).is_file())
        .ok_or_else(|| "Downloaded archive does not contain 3sx.app/Contents/MacOS/3sx".to_string())?;
    let _ = run_tool("/usr/bin/xattr", &["-dr".as_ref(), "com.apple.quarantine".as_ref(), app.as_os_str()]);

    let target = dest.join("3sx.app");
    let _ = std::fs::remove_dir_all(&target);
    std::fs::rename(&app, &target).map_err(|e| format!("Failed to install engine: {}", e))?;
    let _ = std::fs::remove_dir_all(&staging);
    Ok(())
}

fn strip_first(path: PathBuf, strip_root: bool) -> PathBuf {
    if strip_root {
        path.components().skip(1).collect()
    } else {
        path
    }
}

fn extract_targz(archive_path: &Path, extract_dir: &Path, strip_root: bool) -> Result<(), String> {
    let tar_file = std::fs::File::open(archive_path).map_err(|e| format!("Failed to read temp tar.gz: {}", e))?;
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(tar_file));
    for entry in archive.entries().map_err(|e| format!("tar read error: {}", e))? {
        let mut entry = entry.map_err(|e| format!("tar entry error: {}", e))?;
        let entry_path = entry.path().map_err(|e| format!("tar path error: {}", e))?.into_owned();
        let stripped = strip_first(entry_path, strip_root);
        if stripped.as_os_str().is_empty() {
            continue;
        }
        let target_path = extract_dir.join(&stripped);
        if entry.header().entry_type().is_dir() {
            std::fs::create_dir_all(&target_path).unwrap_or_default();
        } else {
            if let Some(p) = target_path.parent() {
                std::fs::create_dir_all(p).unwrap_or_default();
            }
            if let Ok(mut out_file) = std::fs::File::create(&target_path) {
                let _ = std::io::copy(&mut entry, &mut out_file);
            }
        }
    }
    Ok(())
}

fn extract_zip(archive_path: &Path, extract_dir: &Path, strip_root: bool) -> Result<(), String> {
    let file_reader = std::fs::File::open(archive_path).map_err(|e| format!("Failed to read temp zip: {}", e))?;
    let mut archive = zip::ZipArchive::new(file_reader).map_err(|e| format!("Invalid ZIP: {}", e))?;
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| format!("Error reading ZIP file {}: {}", i, e))?;
        let Some(out_path) = file.enclosed_name().map(|p| p.to_owned()) else { continue };
        let stripped = strip_first(out_path, strip_root);
        if stripped.as_os_str().is_empty() {
            continue;
        }
        let target_path = extract_dir.join(stripped);
        if file.name().ends_with('/') || file.is_dir() {
            std::fs::create_dir_all(&target_path).unwrap_or_default();
        } else {
            if let Some(p) = target_path.parent() {
                std::fs::create_dir_all(p).unwrap_or_default();
            }
            if let Ok(mut out_file) = std::fs::File::create(&target_path) {
                let _ = std::io::copy(&mut file, &mut out_file);
            }
            // Keep exec bits (3sx, helper tools) from the archive.
            #[cfg(unix)]
            if let Some(mode) = file.unix_mode() {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&target_path, std::fs::Permissions::from_mode(mode & 0o7777));
            }
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn download_and_extract_archive(
    window: tauri::Window,
    url: String,
    extract_path: String,
    marker_file: String,
    strip_root: bool,
    version_id: Option<String>,
) -> Result<(), String> {
    let root = install_root();

    // Unbound the download time limitation for poor networks
    let client = reqwest::Client::builder().build().map_err(|e| e.to_string())?;
    let resp = client.get(&url).send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("Download failed with status: {}", resp.status()));
    }
    let total_size = resp.content_length().unwrap_or(0);

    use futures_util::StreamExt;
    use std::io::Write;

    let is_targz = url.ends_with(".tar.gz") || url.ends_with(".tgz");
    let temp_ext = if is_targz { "tar.gz" } else { "zip" };

    // Stream to a temp file on disk instead of holding the archive in RAM.
    let temp_file_name = format!("{}.{}.part", marker_file.replace(['/', '\\'], "_"), temp_ext);
    let temp_path = root.join(&temp_file_name);
    let mut file = std::fs::File::create(&temp_path).map_err(|e| format!("Failed to create temp file: {}", e))?;

    let mut downloaded: u64 = 0;
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("Stream error: {}", e))?;
        file.write_all(&chunk).map_err(|e| format!("Write error: {}", e))?;
        downloaded += chunk.len() as u64;
        if total_size > 0 {
            let _ = window.emit("download-progress", (downloaded as f64 / total_size as f64) * 100.0);
        }
    }
    file.flush().unwrap_or_default();
    drop(file);

    let extract_dir = root.join(&extract_path);
    let result = if cfg!(target_os = "macos") && !is_targz {
        install_app_zip(&temp_path, &extract_dir)
    } else if is_targz {
        extract_targz(&temp_path, &extract_dir, strip_root)
    } else {
        extract_zip(&temp_path, &extract_dir, strip_root)
    };
    let _ = std::fs::remove_file(&temp_path);
    result?;

    if !root.join(&marker_file).exists() {
        return Err(format!("Archive extracted but marker file {} was not found", marker_file));
    }
    if let Some(vid) = version_id {
        let _ = std::fs::write(root.join("launcher_version.txt"), vid);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_only_engine_zip() {
        assert_eq!(mac_engine_asset_sha("3SX-abc1234-macos-universal.zip"), Some("abc1234"));
        assert_eq!(mac_engine_asset_sha("3SX-abc1234-macos-universal.dmg"), None);
        assert_eq!(mac_engine_asset_sha("3SXtra-abc1234-macos-universal.zip"), None);
        assert_eq!(mac_engine_asset_sha("3SX-Launcher-windows.zip"), None);
        assert!(same_sha("abc1234", "abc1234def"));
        assert!(!same_sha("", "abc"));
        assert!(!same_sha("abc1234", "abc1235"));
    }
}
