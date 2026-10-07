//! Path resolution — mirrors the engine's paths.c logic.

use directories::UserDirs;
use std::path::PathBuf;

/// Returns the game's install directory (parent of the launcher exe).
/// The expected Windows/Linux layout is:
///   <game_root>/tools/launcher/3sx-launcher.exe
///   <game_root>/3sx.exe
pub fn get_game_root() -> PathBuf {
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            // Release layout: launcher is in game root directly
            let release_marker = exe_dir.join("assets").join("ASSET_VERSION");
            let release_marker_win = exe_dir.join("3sx.exe");
            let release_marker_unix = exe_dir.join("3sx");
            if release_marker.exists() || release_marker_win.exists() || release_marker_unix.exists() {
                return exe_dir.to_path_buf();
            }

            // Dev layout: Tauri 2 places exes deep in target/debug/.
            // Walk up directories (up to 6 levels mapping back to repo root)
            let mut current = exe_dir.to_path_buf();
            for _ in 0..6 {
                if current.join("assets").join("ASSET_VERSION").exists()
                    || (current.join("package.json").exists() && current.join("src-tauri").exists())
                {
                    return std::fs::canonicalize(&current).unwrap_or(current.clone());
                }
                if let Some(parent) = current.parent() {
                    current = parent.to_path_buf();
                } else {
                    break;
                }
            }

            // Fallback
            return exe_dir.to_path_buf();
        }
    }
    PathBuf::from(".")
}

pub fn home_dir() -> PathBuf {
    UserDirs::new()
        .map(|u| u.home_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

/// The engine's standard (non-portable) preference directory, i.e.
/// SDL_GetPrefPath("CrowdedStreet", "3SX"). Created if missing.
pub fn standard_pref_path() -> PathBuf {
    let home = home_dir();

    #[cfg(target_os = "windows")]
    let base_path = home.join("AppData").join("Roaming");

    #[cfg(target_os = "macos")]
    let base_path = home.join("Library").join("Application Support");

    #[cfg(target_os = "linux")]
    let base_path = home.join(".local").join("share");

    // Fallback for any other obscure OS
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    let base_path = home.join(".config");

    let path = base_path.join("CrowdedStreet").join("3SX");
    if !path.exists() {
        let _ = std::fs::create_dir_all(&path);
    }
    path
}

/// The directory the engine considers its "base path" (SDL_GetBasePath).
/// On macOS that is `<engine>.app/Contents/Resources/` of the discovered
/// engine; elsewhere it is the game root next to the launcher.
pub fn engine_base_dir() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        crate::engine::find_engine().map(|app| app.join("Contents").join("Resources"))
    }
    #[cfg(not(target_os = "macos"))]
    {
        Some(get_game_root())
    }
}

/// Preference path — Portable Mode first (a `config/` folder in the engine's
/// base dir, exactly like paths.c), then the standard SDL pref path.
pub fn get_pref_path() -> PathBuf {
    if let Some(base) = engine_base_dir() {
        let portable_path = base.join("config");
        if portable_path.is_dir() {
            return portable_path;
        }
    }
    standard_pref_path()
}

/// Where the updater installs engine downloads and keeps launcher_version.txt.
/// macOS: `<pref>/engine/` (the launcher .app itself may be read-only/signed).
/// Windows/Linux: the game root next to the launcher.
pub fn install_root() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        let dir = standard_pref_path().join("engine");
        let _ = std::fs::create_dir_all(&dir);
        dir
    }
    #[cfg(not(target_os = "macos"))]
    {
        get_game_root()
    }
}

pub fn get_config_file_path() -> PathBuf {
    get_pref_path().join("config")
}

pub fn get_mappings_file_path() -> PathBuf {
    get_pref_path().join("mappings.ini")
}
