//! SF33RD.AFS discovery and import.
//!
//! Probe order follows the research notes (known engine layouts, then where
//! users stage files, then mounted discs). All scans are shallow; we never
//! walk `$HOME` recursively.

use serde::Serialize;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub const AFS_NAME: &str = "SF33RD.AFS";
pub const EXPECTED_SHA256: &str = "f9fa50f3a124ec9fa9465aa9c8546c2d867887eb39f711a070762a0324ba5604";
const USER_SCAN_DEPTH: usize = 2;
const MAX_DIR_ENTRIES: usize = 4000;

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct RomCandidate {
    pub path: String,
    pub source: String,
    /// "afs" (copy as-is) or "iso" (mounted and THIRD/SF33RD.AFS extracted)
    pub kind: String,
    pub size: u64,
    /// True when this is the file the engine already uses.
    pub installed: bool,
}

#[derive(Serialize, Clone, Debug)]
pub struct RomStatus {
    pub installed: bool,
    pub path: Option<String>,
}

#[derive(Serialize, Clone, Debug)]
pub struct ImportResult {
    pub path: String,
    pub sha256: String,
    pub matches_expected: bool,
}

/// Everything the probe needs, so tests can point it at a temp tree.
pub struct RomProbe {
    /// Effective pref dir (portable or standard) and the standard one.
    pub pref_dirs: Vec<PathBuf>,
    /// Engine .app bundles (or base dirs on non-macOS) that exist.
    pub engines: Vec<PathBuf>,
    pub launcher_app: Option<PathBuf>,
    pub launcher_dir: Option<PathBuf>,
    /// Folders that may hold 3SX*.app / 3sxw* installs (3sxw portable layout).
    pub app_dirs: Vec<PathBuf>,
    pub home: PathBuf,
    pub volumes: PathBuf,
}

/// Case-insensitive child lookup (fast path for exact case).
fn ci_child(dir: &Path, name: &str) -> Option<PathBuf> {
    let exact = dir.join(name);
    if exact.exists() {
        return Some(exact);
    }
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(name))
        .map(|e| e.path())
}

fn ci_path(base: &Path, parts: &[&str]) -> Option<PathBuf> {
    parts.iter().try_fold(base.to_path_buf(), |dir, p| ci_child(&dir, p))
}

fn is_iso(p: &Path) -> bool {
    p.extension().is_some_and(|e| e.eq_ignore_ascii_case("iso"))
}

struct Collector {
    out: Vec<RomCandidate>,
    seen: std::collections::HashSet<PathBuf>,
    installed: Option<PathBuf>,
}

impl Collector {
    fn add(&mut self, path: Option<PathBuf>, source: &str) {
        let Some(path) = path else { return };
        let Ok(meta) = std::fs::metadata(&path) else { return };
        if !meta.is_file() {
            return;
        }
        let canon = std::fs::canonicalize(&path).unwrap_or(path.clone());
        if !self.seen.insert(canon.clone()) {
            return;
        }
        self.out.push(RomCandidate {
            path: path.to_string_lossy().into_owned(),
            source: source.to_string(),
            kind: if is_iso(&path) { "iso" } else { "afs" }.to_string(),
            size: meta.len(),
            installed: self.installed.as_ref() == Some(&canon),
        });
    }

    fn add_afs(&mut self, base: &Path, parts: &[&str], source: &str) {
        self.add(ci_path(base, parts), source);
    }

    /// Shallow walk (depth ≤ USER_SCAN_DEPTH) for SF33RD.AFS and *.iso.
    fn scan_user_dir(&mut self, dir: &Path, depth: usize, source: &str) {
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        for entry in rd.flatten().take(MAX_DIR_ENTRIES) {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            let Ok(ft) = entry.file_type() else { continue };
            let path = entry.path();
            if ft.is_file() && (name.eq_ignore_ascii_case(AFS_NAME) || is_iso(&path)) {
                self.add(Some(path), source);
            } else if ft.is_dir() && depth < USER_SCAN_DEPTH && !name.to_ascii_lowercase().ends_with(".app") {
                self.scan_user_dir(&path, depth + 1, source);
            }
        }
    }
}

/// Collects ROM candidates in the documented probe order.
pub fn probe_candidates(p: &RomProbe) -> Vec<RomCandidate> {
    let installed = p
        .pref_dirs
        .first()
        .and_then(|d| ci_path(d, &["resources", AFS_NAME]))
        .and_then(|f| std::fs::canonicalize(f).ok());
    let mut c = Collector { out: vec![], seen: Default::default(), installed };

    // 1. Pref dir(s): <pref>/resources/SF33RD.AFS
    for pref in &p.pref_dirs {
        c.add_afs(pref, &["resources", AFS_NAME], "Pref folder");
    }

    // 2. 3sxtra portable mode and bundled rom/ for every engine we know about
    for app in &p.engines {
        let base = if app.extension().is_some_and(|e| e == "app") {
            app.join("Contents").join("Resources")
        } else {
            app.clone()
        };
        c.add_afs(&base, &["config", "resources", AFS_NAME], "Engine portable config");
        c.add_afs(&base, &["rom", AFS_NAME], "Engine rom folder");
    }

    // 3. 3sxw portable layout: <parent of 3SX*.app>/resources and 3sxw* folders
    for dir in &p.app_dirs {
        let Ok(rd) = std::fs::read_dir(dir) else { continue };
        for entry in rd.flatten().take(MAX_DIR_ENTRIES) {
            let lower = entry.file_name().to_string_lossy().to_ascii_lowercase();
            let path = entry.path();
            if lower.starts_with("3sx") && lower.ends_with(".app") {
                c.add_afs(dir, &["resources", AFS_NAME], "Beside 3SX app");
                c.add_afs(&path, &["Contents", "MacOS", "resources", AFS_NAME], "Inside 3SX app");
            } else if lower.starts_with("3sxw") && path.is_dir() {
                c.add_afs(&path, &["resources", AFS_NAME], "3SXW folder");
                c.add_afs(&path, &[AFS_NAME], "3SXW folder");
            }
        }
    }

    // 4. Next to and inside the launcher .app
    if let Some(dir) = &p.launcher_dir {
        c.add_afs(dir, &["resources", AFS_NAME], "Beside launcher");
        c.add_afs(dir, &[AFS_NAME], "Beside launcher");
    }
    if let Some(app) = &p.launcher_app {
        c.add_afs(app, &["Contents", "Resources", "resources", AFS_NAME], "Inside launcher");
    }

    // 5. User folders, shallow
    for name in ["Downloads", "Documents", "Desktop", "Games", "ROMs"] {
        c.scan_user_dir(&p.home.join(name), 0, name);
    }

    // 6. Mounted discs
    if let Ok(rd) = std::fs::read_dir(&p.volumes) {
        for vol in rd.flatten() {
            let v = vol.path();
            c.add_afs(&v, &["THIRD", AFS_NAME], "Mounted disc");
            c.add_afs(&v, &[AFS_NAME], "Mounted disc");
        }
    }

    c.out
}

/// The probe for the running launcher.
pub fn current_probe() -> RomProbe {
    let pref = crate::paths::get_pref_path();
    let std_pref = crate::paths::standard_pref_path();
    let mut pref_dirs = vec![pref.clone()];
    if std_pref != pref {
        pref_dirs.push(std_pref);
    }
    let exe = std::env::current_exe().unwrap_or_default();
    let launcher_app = crate::engine::enclosing_app(&exe);
    let launcher_dir = match &launcher_app {
        Some(app) => app.parent().map(Path::to_path_buf),
        None => exe.parent().map(Path::to_path_buf),
    };
    #[cfg(target_os = "macos")]
    let engines: Vec<PathBuf> = crate::engine::engine_candidate_paths(&exe, &crate::paths::standard_pref_path(), &crate::paths::home_dir())
        .into_iter()
        .filter(|a| crate::engine::engine_binary(a).is_file())
        .collect();
    #[cfg(not(target_os = "macos"))]
    let engines = vec![crate::paths::get_game_root()];
    RomProbe {
        pref_dirs,
        engines,
        launcher_app,
        app_dirs: [
            Some(PathBuf::from("/Applications")),
            Some(crate::paths::home_dir().join("Applications")),
            Some(crate::paths::home_dir().join("Downloads")),
            launcher_dir.clone(),
        ]
        .into_iter()
        .flatten()
        .collect(),
        launcher_dir,
        home: crate::paths::home_dir(),
        volumes: PathBuf::from("/Volumes"),
    }
}

/// Where the engine reads the ROM from: `<pref>/resources`, then `<base>/rom`.
pub fn rom_status() -> RomStatus {
    let mut found = ci_path(&crate::paths::get_pref_path(), &["resources", AFS_NAME]);
    if found.is_none() {
        found = crate::paths::engine_base_dir().and_then(|b| ci_path(&b, &["rom", AFS_NAME]));
    }
    RomStatus { installed: found.is_some(), path: found.map(|p| p.to_string_lossy().into_owned()) }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Copies `src` to `dst` (via `dst.part` + rename) and returns its SHA-256.
/// When `src` and `dst` are the same file it is only hashed.
pub fn copy_with_sha256(src: &Path, dst: &Path) -> Result<String, String> {
    let same = match (std::fs::canonicalize(src), std::fs::canonicalize(dst)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    };
    let mut input = std::fs::File::open(src).map_err(|e| format!("Cannot open {}: {}", src.display(), e))?;
    let part = dst.with_extension("AFS.part");
    let mut output = if same {
        None
    } else {
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        Some(std::fs::File::create(&part).map_err(|e| format!("Cannot write {}: {}", part.display(), e))?)
    };
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = input.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        if let Some(out) = output.as_mut() {
            out.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        }
    }
    if let Some(out) = output {
        out.sync_all().map_err(|e| e.to_string())?;
        drop(out);
        std::fs::rename(&part, dst).map_err(|e| e.to_string())?;
    }
    Ok(hex(&hasher.finalize()))
}

/// Mounts a disc image read-only and returns (mount point, AFS path inside).
#[cfg(target_os = "macos")]
fn mount_iso(iso: &Path) -> Result<(PathBuf, PathBuf), String> {
    let mnt = std::env::temp_dir().join(format!("3sx-iso-{}", std::process::id()));
    std::fs::create_dir_all(&mnt).map_err(|e| e.to_string())?;
    let status = std::process::Command::new("/usr/bin/hdiutil")
        .args(["attach", "-readonly", "-nobrowse", "-noverify", "-mountpoint"])
        .arg(&mnt)
        .arg(iso)
        .status()
        .map_err(|e| e.to_string())?;
    if !status.success() {
        return Err(format!("Could not mount {}", iso.display()));
    }
    match ci_path(&mnt, &["THIRD", AFS_NAME]).or_else(|| ci_path(&mnt, &[AFS_NAME])) {
        Some(afs) => Ok((mnt, afs)),
        None => {
            unmount(&mnt);
            Err(format!("{} does not contain THIRD/SF33RD.AFS", iso.display()))
        }
    }
}

#[cfg(target_os = "macos")]
fn unmount(mnt: &Path) {
    let _ = std::process::Command::new("/usr/bin/hdiutil").arg("detach").arg(mnt).status();
    let _ = std::fs::remove_dir(mnt);
}

/// Imports `src` (an AFS file or, on macOS, a PS2 disc image) into
/// `<pref>/resources/SF33RD.AFS`. A checksum mismatch is reported, not fatal.
pub fn import(src: &Path) -> Result<ImportResult, String> {
    let dst = crate::paths::get_pref_path().join("resources").join(AFS_NAME);
    let sha = if is_iso(src) {
        #[cfg(target_os = "macos")]
        {
            let (mnt, afs) = mount_iso(src)?;
            let r = copy_with_sha256(&afs, &dst);
            unmount(&mnt);
            r?
        }
        #[cfg(not(target_os = "macos"))]
        return Err("Disc image import is only supported on macOS; copy THIRD/SF33RD.AFS out of the image".into());
    } else {
        copy_with_sha256(src, &dst)?
    };
    Ok(ImportResult {
        path: dst.to_string_lossy().into_owned(),
        matches_expected: sha == EXPECTED_SHA256,
        sha256: sha,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(p: &Path) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, b"afs").unwrap();
    }

    #[test]
    fn probe_order_and_shallow_scan() {
        let tmp = std::env::temp_dir().join(format!("3sx-rom-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let home = tmp.join("home");
        touch(&tmp.join("pref/resources/SF33RD.AFS"));
        touch(&home.join("Downloads/ps2/THIRD/sf33rd.afs")); // depth 2, lower case
        touch(&home.join("Desktop/a/b/c/SF33RD.AFS")); // depth 3: ignored
        touch(&home.join("Documents/sfac.iso"));
        touch(&tmp.join("Volumes/SFAC/THIRD/SF33RD.AFS"));
        touch(&tmp.join("apps/3sxw-1.1/resources/SF33RD.AFS"));
        let probe = RomProbe {
            pref_dirs: vec![tmp.join("pref")],
            engines: vec![],
            launcher_app: None,
            launcher_dir: None,
            app_dirs: vec![tmp.join("apps")],
            home,
            volumes: tmp.join("Volumes"),
        };
        let got = probe_candidates(&probe);
        let sources: Vec<&str> = got.iter().map(|c| c.source.as_str()).collect();
        assert_eq!(sources, ["Pref folder", "3SXW folder", "Downloads", "Documents", "Mounted disc"]);
        assert!(got[0].installed && !got[1].installed);
        assert_eq!(got[3].kind, "iso");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn copy_hashes_and_handles_same_file() {
        let tmp = std::env::temp_dir().join(format!("3sx-rom-copy-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let src = tmp.join("src/SF33RD.AFS");
        touch(&src);
        let dst = tmp.join("pref/resources/SF33RD.AFS");
        let sha = copy_with_sha256(&src, &dst).unwrap();
        assert_eq!(sha, hex(&Sha256::digest(b"afs")));
        assert_eq!(std::fs::read(&dst).unwrap(), b"afs");
        assert_eq!(copy_with_sha256(&dst, &dst).unwrap(), sha);
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
