mod engine;
mod paths;
mod rom;
mod updater;

use ini::Ini;
use paths::{get_config_file_path, get_game_root, get_mappings_file_path, install_root};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone)]
pub struct GameConfig {
    pub key: String,
    pub value: String,
}

// ────────────────────────────────────────────────────────────
// Game Launch
// ────────────────────────────────────────────────────────────

/// The engine executable on Windows/Linux (`<game_root>/3sx[.exe]`).
#[cfg(not(target_os = "macos"))]
fn engine_exe() -> PathBuf {
    get_game_root().join(format!("3sx{}", std::env::consts::EXE_SUFFIX))
}

/// The engine binary inside the discovered `3sx.app` on macOS.
#[cfg(target_os = "macos")]
fn engine_exe() -> PathBuf {
    engine::find_engine()
        .map(|app| engine::engine_binary(&app))
        .unwrap_or_else(|| get_game_root().join("3sx.app/Contents/MacOS/3sx"))
}

#[tauri::command]
fn is_game_installed() -> Result<bool, String> {
    Ok(engine_exe().exists())
}

#[tauri::command]
fn launch_game() -> Result<String, String> {
    #[cfg(target_os = "macos")]
    {
        let app = engine::find_engine().ok_or("3sx.app not found. Install the game first.")?;
        engine::launch(&app, &paths::standard_pref_path().join("logs"))?;
        Ok(format!("Game launched: {}", app.display()))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let game_root = get_game_root();
        let exe_path = engine_exe();
        if !exe_path.exists() {
            return Err(format!("Game executable not found at: {}", exe_path.display()));
        }
        std::process::Command::new(&exe_path)
            .current_dir(&game_root)
            .spawn()
            .map(|_| "Game launched successfully".to_string())
            .map_err(|e| format!("Failed to launch game: {}", e))
    }
}

// ────────────────────────────────────────────────────────────
// Config Management
// The game's config file is a FLAT key=value format (no sections).
// We must use the "General" / None section in rust-ini to match this.
// ────────────────────────────────────────────────────────────

fn read_flat_ini(path: &std::path::Path) -> Result<Vec<GameConfig>, String> {
    if !path.exists() {
        return Ok(vec![]);
    }
    let conf = Ini::load_from_file(path).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for (section, prop) in conf.iter() {
        // Only read the global (sectionless) entries — the game doesn't use sections
        if section.is_some() {
            continue;
        }
        for (key, value) in prop.iter() {
            out.push(GameConfig { key: key.to_string(), value: value.to_string() });
        }
    }
    Ok(out)
}

#[tauri::command]
fn get_config() -> Result<Vec<GameConfig>, String> {
    read_flat_ini(&get_config_file_path())
}

#[tauri::command]
fn save_config(key: String, value: String) -> Result<(), String> {
    let path = get_config_file_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let mut conf = if path.exists() { Ini::load_from_file(&path).unwrap_or_default() } else { Ini::new() };
    // Write to the global (sectionless) area — matches the game's flat format
    conf.with_section(None::<String>).set(&key, &value);
    conf.write_to_file(&path).map_err(|e| e.to_string())
}

// ────────────────────────────────────────────────────────────
// Mappings (mappings.ini) — also flat key=value
// ────────────────────────────────────────────────────────────

#[tauri::command]
fn get_mappings() -> Result<Vec<GameConfig>, String> {
    read_flat_ini(&get_mappings_file_path())
}

#[tauri::command]
fn save_mapping(player: String, action: String, input: String) -> Result<(), String> {
    let path = get_mappings_file_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let mut lines: Vec<String> = std::fs::read_to_string(&path)
        .map(|c| c.lines().map(str::to_string).collect())
        .unwrap_or_default();

    let prefix = format!("{}_mapping=", player);
    let target_start = format!("{}{},", prefix, action);
    let new_line = format!("{}{},{}", prefix, action, input);

    match lines.iter_mut().find(|l| l.starts_with(&target_start)) {
        Some(line) => *line = new_line,
        None => lines.push(new_line),
    }
    std::fs::write(&path, lines.join("\n")).map_err(|e| e.to_string())
}

// ────────────────────────────────────────────────────────────
// ROM (SF33RD.AFS)
// ────────────────────────────────────────────────────────────

#[tauri::command]
fn get_rom_status() -> rom::RomStatus {
    rom::rom_status()
}

#[tauri::command]
async fn find_rom_candidates() -> Result<Vec<rom::RomCandidate>, String> {
    tauri::async_runtime::spawn_blocking(|| rom::probe_candidates(&rom::current_probe()))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn import_rom(path: String) -> Result<rom::ImportResult, String> {
    tauri::async_runtime::spawn_blocking(move || rom::import(std::path::Path::new(&path)))
        .await
        .map_err(|e| e.to_string())?
}

// ────────────────────────────────────────────────────────────
// Utilities
// ────────────────────────────────────────────────────────────

#[tauri::command]
fn check_file_exists(path: String) -> Result<bool, String> {
    Ok(install_root().join(&path).exists())
}

/// UTC `YYYY-MM-DD` from seconds since the epoch (Hinnant's civil_from_days).
fn utc_date(secs: i64) -> String {
    let z = secs.div_euclid(86400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{:04}-{:02}-{:02}", year, month, day)
}

#[tauri::command]
fn get_local_version() -> Result<String, String> {
    // 0. macOS: the discovered engine's ENGINE_VERSION build time
    #[cfg(target_os = "macos")]
    if let Some(v) = engine::find_engine().and_then(|app| engine::read_engine_version(&app)) {
        return Ok(v.built);
    }
    // 1. launcher_version.txt (written by auto-updater downloads)
    if let Ok(content) = std::fs::read_to_string(install_root().join("launcher_version.txt")) {
        let trimmed = content.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }
    // 2. Engine exe modification time (for local builds)
    if let Ok(modified) = std::fs::metadata(engine_exe()).and_then(|m| m.modified()) {
        let secs = modified.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs() as i64;
        return Ok(utc_date(secs));
    }
    Ok("UNKNOWN".to_string())
}

#[tauri::command]
fn get_launcher_build_date() -> Result<String, String> {
    Ok(env!("LAUNCHER_BUILD_DATE").to_string())
}

/// `--diagnose`: print engine/ROM discovery as JSON and exit (support aid).
fn diagnose() -> serde_json::Value {
    let exe = std::env::current_exe().unwrap_or_default();
    let candidates: Vec<serde_json::Value> =
        engine::engine_candidate_paths(&exe, &paths::standard_pref_path(), &paths::home_dir())
            .into_iter()
            .map(|p| {
                serde_json::json!({
                    "path": p,
                    "exists": engine::engine_binary(&p).is_file(),
                    "version": engine::read_engine_version(&p).map(|v| (v.built, v.sha)),
                })
            })
            .collect();
    serde_json::json!({
        "launcher_exe": exe,
        "engine_exe": engine_exe(),
        "engine_app": engine::find_engine(),
        "engine_candidates": candidates,
        "pref_path": paths::get_pref_path(),
        "install_root": install_root(),
        "rom_status": rom::rom_status(),
        "rom_candidates": rom::probe_candidates(&rom::current_probe()),
        "local_version": get_local_version().ok(),
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--diagnose") {
        println!("{}", serde_json::to_string_pretty(&diagnose()).unwrap_or_default());
        return;
    }
    if args.iter().any(|a| a == "--launch") {
        match launch_game() {
            Ok(msg) => println!("{}", msg),
            Err(err) => {
                eprintln!("{}", err);
                std::process::exit(1);
            }
        }
        return;
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            is_game_installed,
            launch_game,
            get_config,
            save_config,
            get_mappings,
            save_mapping,
            updater::check_updates,
            updater::download_and_extract_archive,
            check_file_exists,
            get_local_version,
            get_launcher_build_date,
            get_rom_status,
            find_rom_candidates,
            import_rom
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::utc_date;

    #[test]
    fn utc_date_known_values() {
        assert_eq!(utc_date(0), "1970-01-01");
        assert_eq!(utc_date(951_782_400), "2000-02-29");
        assert_eq!(utc_date(1_791_331_200), "2026-10-07");
    }
}
