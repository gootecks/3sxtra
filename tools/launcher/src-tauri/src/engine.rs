//! macOS engine discovery and launch.
//!
//! On macOS the engine is a separate `3sx.app`. It can live in several places;
//! we probe them in a fixed order and pick the newest build by the first line
//! of `Contents/Resources/ENGINE_VERSION` (an ISO-8601 UTC timestamp). A
//! missing ENGINE_VERSION counts as oldest; ties go to the earlier candidate.
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use std::path::{Path, PathBuf};

pub const ENGINE_APP: &str = "3sx.app";

/// Parsed `ENGINE_VERSION`: line 1 build time, line 2 git short sha.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineVersion {
    pub built: String,
    pub sha: Option<String>,
}

/// If `exe` is `<X>.app/Contents/MacOS/<bin>`, returns `<X>.app`.
pub fn enclosing_app(exe: &Path) -> Option<PathBuf> {
    let macos = exe.parent()?;
    let contents = macos.parent()?;
    let app = contents.parent()?;
    let is_app = macos.file_name()? == "MacOS"
        && contents.file_name()? == "Contents"
        && app.extension().is_some_and(|e| e.eq_ignore_ascii_case("app"));
    is_app.then(|| app.to_path_buf())
}

/// The embedded engine inside a combined `3SXtra.app`.
pub fn embedded_engine(launcher_app: &Path) -> PathBuf {
    launcher_app.join("Contents").join("Resources").join("engine").join(ENGINE_APP)
}

/// Candidate engine locations, in contract order:
/// 1. `<pref>/engine/3sx.app` (installed by the launcher updater)
/// 2. the engine embedded in the launcher bundle
/// 3. `3sx.app` beside the launcher .app (or beside the bare exe in dev)
/// 4. `/Applications/3sx.app`
/// 5. `~/Applications/3sx.app`
pub fn engine_candidate_paths(exe: &Path, pref: &Path, home: &Path) -> Vec<PathBuf> {
    let mut out = vec![pref.join("engine").join(ENGINE_APP)];
    let beside_dir = match enclosing_app(exe) {
        Some(app) => {
            out.push(embedded_engine(&app));
            app.parent().map(Path::to_path_buf)
        }
        None => exe.parent().map(Path::to_path_buf),
    };
    if let Some(dir) = beside_dir {
        out.push(dir.join(ENGINE_APP));
    }
    out.push(PathBuf::from("/Applications").join(ENGINE_APP));
    out.push(home.join("Applications").join(ENGINE_APP));
    out
}

pub fn engine_binary(app: &Path) -> PathBuf {
    app.join("Contents").join("MacOS").join("3sx")
}

pub fn read_engine_version(app: &Path) -> Option<EngineVersion> {
    let text = std::fs::read_to_string(app.join("Contents").join("Resources").join("ENGINE_VERSION")).ok()?;
    let mut lines = text.lines().map(str::trim);
    let built = lines.next().filter(|l| !l.is_empty())?.to_string();
    let sha = lines.next().filter(|l| !l.is_empty()).map(str::to_string);
    Some(EngineVersion { built, sha })
}

/// Index of the newest candidate. `None` versions are oldest; on a tie the
/// earliest index wins. ISO-8601 UTC timestamps compare correctly as strings.
pub fn select_newest(versions: &[Option<String>]) -> Option<usize> {
    let mut best: Option<usize> = None;
    for (i, v) in versions.iter().enumerate() {
        match best {
            None => best = Some(i),
            Some(b) if *v > versions[b] => best = Some(i),
            _ => {}
        }
    }
    best
}

/// Picks the newest existing engine among `candidates`.
pub fn pick_engine(candidates: &[PathBuf]) -> Option<PathBuf> {
    let existing: Vec<&PathBuf> = candidates.iter().filter(|p| engine_binary(p).is_file()).collect();
    let versions: Vec<Option<String>> = existing
        .iter()
        .map(|p| read_engine_version(p).map(|v| v.built))
        .collect();
    select_newest(&versions).map(|i| existing[i].clone())
}

/// Discovers the engine for the running launcher.
pub fn find_engine() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let candidates = engine_candidate_paths(
        &exe,
        &crate::paths::standard_pref_path(),
        &crate::paths::home_dir(),
    );
    pick_engine(&candidates)
}

/// Launches `app` as a new instance via LaunchServices so its window comes to
/// the front. The engine's stdout/stderr go to `<log_dir>/engine-*.log`.
pub fn launch(app: &Path, log_dir: &Path) -> Result<(), String> {
    if !engine_binary(app).is_file() {
        return Err(format!("Game executable not found in: {}", app.display()));
    }
    let _ = std::fs::create_dir_all(log_dir);
    let status = std::process::Command::new("/usr/bin/open")
        .arg("-n")
        .arg("--stdout")
        .arg(log_dir.join("engine-stdout.log"))
        .arg("--stderr")
        .arg(log_dir.join("engine-stderr.log"))
        .arg("-a")
        .arg(app)
        .status()
        .map_err(|e| format!("Failed to run open: {}", e))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("open exited with {}", status))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &str) -> Option<String> {
        Some(v.to_string())
    }

    #[test]
    fn newest_version_wins() {
        let v = [s("2026-10-01T00:00:00Z"), s("2026-10-05T12:00:00Z"), None];
        assert_eq!(select_newest(&v), Some(1));
    }

    #[test]
    fn tie_prefers_earlier_candidate() {
        let v = [None, s("2026-10-05T12:00:00Z"), s("2026-10-05T12:00:00Z")];
        assert_eq!(select_newest(&v), Some(1));
        assert_eq!(select_newest(&[None, None]), Some(0));
        assert_eq!(select_newest(&[]), None);
    }

    #[test]
    fn missing_version_is_oldest() {
        let v = [None, s("2000-01-01T00:00:00Z")];
        assert_eq!(select_newest(&v), Some(1));
    }

    #[test]
    fn candidate_order_from_bundle() {
        let exe = Path::new("/Apps/3SXtra.app/Contents/MacOS/launcher");
        let c = engine_candidate_paths(exe, Path::new("/P"), Path::new("/H"));
        let want: Vec<PathBuf> = [
            "/P/engine/3sx.app",
            "/Apps/3SXtra.app/Contents/Resources/engine/3sx.app",
            "/Apps/3sx.app",
            "/Applications/3sx.app",
            "/H/Applications/3sx.app",
        ]
        .iter()
        .map(PathBuf::from)
        .collect();
        assert_eq!(c, want);
    }

    #[test]
    fn candidate_order_dev_exe() {
        let c = engine_candidate_paths(Path::new("/dev/target/debug/launcher"), Path::new("/P"), Path::new("/H"));
        assert_eq!(c[1], PathBuf::from("/dev/target/debug/3sx.app"));
        assert_eq!(c.len(), 4);
    }

    fn fake_engine(root: &Path, rel: &str, version: Option<&str>) -> PathBuf {
        let app = root.join(rel);
        std::fs::create_dir_all(app.join("Contents/MacOS")).unwrap();
        std::fs::create_dir_all(app.join("Contents/Resources")).unwrap();
        std::fs::write(engine_binary(&app), b"").unwrap();
        if let Some(v) = version {
            std::fs::write(app.join("Contents/Resources/ENGINE_VERSION"), v).unwrap();
        }
        app
    }

    #[test]
    fn pick_engine_on_disk() {
        let tmp = std::env::temp_dir().join(format!("3sx-engine-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let pref_engine = tmp.join("pref/engine/3sx.app");
        let embedded = fake_engine(&tmp, "Apps/3SXtra.app/Contents/Resources/engine/3sx.app", Some("2026-10-06T10:00:00Z\nabc1234\n"));
        let beside = fake_engine(&tmp, "Apps/3sx.app", Some("2026-10-01T00:00:00Z\n"));
        let candidates = vec![pref_engine.clone(), embedded.clone(), beside.clone()];

        // pref engine missing → embedded (newest) wins
        assert_eq!(pick_engine(&candidates), Some(embedded.clone()));
        assert_eq!(read_engine_version(&embedded).unwrap().sha.as_deref(), Some("abc1234"));

        // pref engine without ENGINE_VERSION is oldest
        fake_engine(&tmp, "pref/engine/3sx.app", None);
        assert_eq!(pick_engine(&candidates), Some(embedded.clone()));

        // same timestamp → earlier (pref) wins
        std::fs::write(pref_engine.join("Contents/Resources/ENGINE_VERSION"), "2026-10-06T10:00:00Z\n").unwrap();
        assert_eq!(pick_engine(&candidates), Some(pref_engine));
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
