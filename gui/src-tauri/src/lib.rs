use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{Mutex, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Manager};

use base64::{engine::general_purpose, Engine as _};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum MediaKind {
    Static,
    Live,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct WallpaperItem {
    path: String,
    name: String,
    kind: MediaKind,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct StartupFolder {
    path: Option<String>,
    source: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct MediaSource {
    path: String,
    src: Option<String>,
    reason: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GuiState {
    version: u8,
    last_folder: Option<String>,
    last_monitor: Option<String>,
    preview_mode: bool,
}

impl GuiState {
    fn load() -> Self {
        let Some(path) = gui_state_path() else {
            return Self::default_with_version();
        };
        let Ok(raw) = fs::read_to_string(path) else {
            return Self::default_with_version();
        };
        serde_json::from_str(&raw).unwrap_or_else(|_| Self::default_with_version())
    }

    fn default_with_version() -> Self {
        Self {
            version: 1,
            last_folder: None,
            last_monitor: Some("all".to_string()),
            preview_mode: false,
        }
    }

    fn save(&self) -> Result<(), String> {
        let path =
            gui_state_path().ok_or_else(|| "failed to resolve GUI state path".to_string())?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "failed to create GUI state dir {}: {error}",
                    parent.display()
                )
            })?;
        }
        let raw = serde_json::to_string_pretty(self)
            .map_err(|error| format!("failed to serialize GUI state: {error}"))?;
        fs::write(&path, raw)
            .map_err(|error| format!("failed to write GUI state {}: {error}", path.display()))
    }
}

#[tauri::command]
fn resolve_startup_folder(input_path: Option<String>) -> Result<StartupFolder, String> {
    if let Some(path) = input_path
        .or_else(first_path_arg)
        .and_then(|path| existing_dir(PathBuf::from(path)))
    {
        return Ok(StartupFolder {
            path: Some(path.display().to_string()),
            source: "argument".to_string(),
        });
    }

    let state = GuiState::load();
    if let Some(path) = state
        .last_folder
        .as_ref()
        .and_then(|path| existing_dir(PathBuf::from(path)))
    {
        return Ok(StartupFolder {
            path: Some(path.display().to_string()),
            source: "last-folder".to_string(),
        });
    }

    if let Some(path) = dirs::home_dir()
        .map(|home| {
            home.join(".config")
                .join("omarchy")
                .join("current")
                .join("theme")
                .join("backgrounds")
        })
        .and_then(existing_dir)
    {
        return Ok(StartupFolder {
            path: Some(path.display().to_string()),
            source: "theme-backgrounds".to_string(),
        });
    }

    if let Some(path) = dirs::home_dir()
        .map(|home| home.join("Pictures").join("Wallpapers"))
        .and_then(existing_dir)
    {
        return Ok(StartupFolder {
            path: Some(path.display().to_string()),
            source: "pictures-wallpapers".to_string(),
        });
    }

    Ok(StartupFolder {
        path: None,
        source: "none".to_string(),
    })
}

#[tauri::command]
fn scan_folder(path: String) -> Result<Vec<WallpaperItem>, String> {
    let path = fs::canonicalize(&path)
        .map_err(|error| format!("failed to resolve folder {path}: {error}"))?;
    if !path.is_dir() {
        return Err(format!("not a folder: {}", path.display()));
    }

    let mut items = Vec::new();
    for entry in fs::read_dir(&path)
        .map_err(|error| format!("failed to read folder {}: {error}", path.display()))?
    {
        let entry = entry.map_err(|error| {
            format!("failed to read entry in folder {}: {error}", path.display())
        })?;
        let item_path = entry.path();
        if !item_path.is_file() {
            continue;
        }
        let Some(kind) = classify_path(&item_path) else {
            continue;
        };
        let name = item_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_string();
        items.push(WallpaperItem {
            path: item_path.display().to_string(),
            name,
            kind,
        });
    }

    items.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(items)
}

#[tauri::command]
fn get_status() -> Result<serde_json::Value, String> {
    let output = run_jobowalls(["status", "--json"])?;
    serde_json::from_str(&output).map_err(|error| format!("failed to parse status JSON: {error}"))
}

#[tauri::command]
fn preview_plan(path: String, monitor: Option<String>) -> Result<serde_json::Value, String> {
    let mut args = vec![
        "set".to_string(),
        path,
        "--dry-run".to_string(),
        "--json".to_string(),
    ];
    if let Some(monitor) = monitor.filter(|monitor| !monitor.is_empty()) {
        args.push("--monitor".to_string());
        args.push(monitor);
    }
    let output = run_jobowalls(args)?;
    serde_json::from_str(&output).map_err(|error| format!("failed to parse plan JSON: {error}"))
}

#[tauri::command]
fn apply_wallpaper(path: String, monitor: Option<String>) -> Result<(), String> {
    let mut args = vec!["set".to_string(), path];
    if let Some(monitor) = monitor.filter(|monitor| !monitor.is_empty()) {
        args.push("--monitor".to_string());
        args.push(monitor);
    }
    run_jobowalls(args).map(|_| ())
}

#[tauri::command]
fn get_media_source(path: String) -> Result<MediaSource, String> {
    let path_buf = PathBuf::from(&path);
    let Some(kind) = classify_path(&path_buf) else {
        return Ok(MediaSource {
            path,
            src: None,
            reason: Some("unsupported media type".to_string()),
        });
    };

    let preview_path = if matches!(kind, MediaKind::Live) {
        generate_video_poster(&path_buf)?
    } else {
        path_buf
    };

    Ok(MediaSource {
        path,
        src: Some(preview_path.display().to_string()),
        reason: None,
    })
}

#[tauri::command]
fn get_media_data_source(path: String) -> Result<MediaSource, String> {
    let path_buf = PathBuf::from(&path);
    let Some(kind) = classify_path(&path_buf) else {
        return Ok(MediaSource {
            path,
            src: None,
            reason: Some("unsupported media type".to_string()),
        });
    };

    let preview_path = if matches!(kind, MediaKind::Live) {
        generate_video_poster(&path_buf)?
    } else {
        path_buf
    };

    data_source(path, &preview_path)
}

#[tauri::command]
fn get_live_preview_source(path: String) -> Result<MediaSource, String> {
    let path_buf = PathBuf::from(&path);
    if !matches!(classify_path(&path_buf), Some(MediaKind::Live)) {
        return get_media_source(path);
    }

    let preview_path = match cached_video_animation_path(&path_buf) {
        Ok(Some(path)) => path,
        _ => generate_video_animation(&path_buf)?,
    };
    Ok(MediaSource {
        path,
        src: Some(preview_path.display().to_string()),
        reason: None,
    })
}

#[tauri::command]
fn get_live_preview_data_source(path: String) -> Result<MediaSource, String> {
    let path_buf = PathBuf::from(&path);
    if !matches!(classify_path(&path_buf), Some(MediaKind::Live)) {
        return get_media_data_source(path);
    }

    let preview_path = match cached_video_animation_path(&path_buf) {
        Ok(Some(path)) => path,
        _ => generate_video_animation(&path_buf)?,
    };
    data_source(path, &preview_path)
}

#[tauri::command]
fn warm_live_preview(path: String) -> Result<(), String> {
    let path_buf = PathBuf::from(path);
    if !matches!(classify_path(&path_buf), Some(MediaKind::Live)) {
        return Ok(());
    }

    std::thread::spawn(move || {
        let _ = generate_video_poster(&path_buf);
        let _ = cached_video_animation_path(&path_buf)
            .ok()
            .flatten()
            .or_else(|| generate_video_animation(&path_buf).ok());
    });

    Ok(())
}

#[tauri::command]
fn save_last_folder(path: String) -> Result<(), String> {
    let mut state = GuiState::load();
    state.version = 1;
    state.last_folder = Some(path);
    state.save()
}

#[tauri::command]
fn close_picker(app: AppHandle) {
    app.exit(0);
}

fn first_path_arg() -> Option<String> {
    std::env::args()
        .skip(1)
        .find(|arg| !arg.starts_with('-') && arg != "jobowalls-gui")
}

fn existing_dir(path: PathBuf) -> Option<PathBuf> {
    let expanded = expand_home(path);
    if expanded.is_dir() {
        fs::canonicalize(&expanded).ok().or(Some(expanded))
    } else {
        None
    }
}

fn expand_home(path: PathBuf) -> PathBuf {
    let Some(raw) = path.to_str() else {
        return path;
    };
    if raw == "~" {
        return dirs::home_dir().unwrap_or(path);
    }
    if let Some(rest) = raw.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    path
}

fn classify_path(path: &Path) -> Option<MediaKind> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    match ext.as_str() {
        "jpg" | "jpeg" | "png" | "webp" | "bmp" | "gif" => Some(MediaKind::Static),
        "mp4" | "webm" | "mkv" | "mov" | "avi" => Some(MediaKind::Live),
        _ => None,
    }
}

fn data_source(path: String, preview_path: &Path) -> Result<MediaSource, String> {
    let bytes = fs::read(preview_path)
        .map_err(|error| format!("failed to read preview {}: {error}", preview_path.display()))?;
    let mime = mime_for_path(preview_path);
    let encoded = general_purpose::STANDARD.encode(bytes);

    Ok(MediaSource {
        path,
        src: Some(format!("data:{mime};base64,{encoded}")),
        reason: None,
    })
}

fn mime_for_path(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .as_deref()
    {
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("gif") => "image/gif",
        Some("bmp") => "image/bmp",
        _ => "image/png",
    }
}

fn generate_video_poster(path: &Path) -> Result<PathBuf, String> {
    let cache_path = video_poster_cache_path(path)?;
    if cache_path.exists() {
        return Ok(cache_path);
    }

    if let Some(parent) = cache_path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create thumbnail cache {}: {error}",
                parent.display()
            )
        })?;
    }

    if try_ffmpegthumbnailer(path, &cache_path)? || try_ffmpeg(path, &cache_path)? {
        return Ok(cache_path);
    }

    Err(
        "failed to generate live wallpaper preview: ffmpeg or ffmpegthumbnailer is required"
            .to_string(),
    )
}

fn generate_video_animation(path: &Path) -> Result<PathBuf, String> {
    let cache_path = video_animation_cache_path(path)?;
    if cache_path.exists() {
        return Ok(cache_path);
    }

    if let Some(parent) = cache_path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create animated preview cache {}: {error}",
                parent.display()
            )
        })?;
    }

    with_generation_lock(cache_path.clone(), || {
        if cache_path.exists() {
            return Ok(cache_path);
        }

        let status = match Command::new("ffmpeg")
            .args([
                "-y",
                "-hide_banner",
                "-loglevel",
                "error",
                "-ss",
                "00:00:00.5",
                "-t",
                "0.75",
                "-i",
            ])
            .arg(path)
            .args([
                "-vf",
                "fps=5,scale=420:-1:flags=fast_bilinear",
                "-loop",
                "0",
                "-quality",
                "48",
                "-compression_level",
                "0",
            ])
            .arg(&cache_path)
            .status()
        {
            Ok(status) => status,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return generate_video_poster(path);
            }
            Err(error) => return Err(format!("failed to start ffmpeg: {error}")),
        };

        if status.success() && cache_path.exists() {
            Ok(cache_path)
        } else {
            generate_video_poster(path)
        }
    })
}

fn cached_video_animation_path(path: &Path) -> Result<Option<PathBuf>, String> {
    let cache_path = video_animation_cache_path(path)?;
    Ok(cache_path.exists().then_some(cache_path))
}

fn try_ffmpegthumbnailer(input: &Path, output: &Path) -> Result<bool, String> {
    let status = match Command::new("ffmpegthumbnailer")
        .args(["-i"])
        .arg(input)
        .args(["-o"])
        .arg(output)
        .args(["-s", "960", "-q", "8", "-t", "10%"])
        .status()
    {
        Ok(status) => status,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(format!("failed to start ffmpegthumbnailer: {error}")),
    };

    Ok(status.success() && output.exists())
}

fn try_ffmpeg(input: &Path, output: &Path) -> Result<bool, String> {
    let status = match Command::new("ffmpeg")
        .args([
            "-y",
            "-hide_banner",
            "-loglevel",
            "error",
            "-ss",
            "00:00:01",
            "-i",
        ])
        .arg(input)
        .args(["-frames:v", "1", "-vf", "scale=960:-1"])
        .arg(output)
        .status()
    {
        Ok(status) => status,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(format!("failed to start ffmpeg: {error}")),
    };

    Ok(status.success() && output.exists())
}

fn video_poster_cache_path(path: &Path) -> Result<PathBuf, String> {
    video_cache_path(path, "jpg")
}

fn video_animation_cache_path(path: &Path) -> Result<PathBuf, String> {
    video_cache_path(path, "webp")
}

fn video_cache_path(path: &Path, extension: &str) -> Result<PathBuf, String> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("failed to read metadata for {}: {error}", path.display()))?;
    let modified = metadata
        .modified()
        .unwrap_or(SystemTime::UNIX_EPOCH)
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let key = format!(
        "{}:{}:{}:{}",
        path.display(),
        metadata.len(),
        modified,
        extension
    );
    let name = fnv1a_hex(key.as_bytes());
    let cache_dir = dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("jobowalls")
        .join("gui-thumbnails");
    Ok(cache_dir.join(format!("{name}.{extension}")))
}

fn generation_locks() -> &'static Mutex<HashSet<PathBuf>> {
    static LOCKS: OnceLock<Mutex<HashSet<PathBuf>>> = OnceLock::new();
    LOCKS.get_or_init(|| Mutex::new(HashSet::new()))
}

fn with_generation_lock<F>(cache_path: PathBuf, generate: F) -> Result<PathBuf, String>
where
    F: FnOnce() -> Result<PathBuf, String>,
{
    loop {
        let inserted = {
            let mut locks = generation_locks()
                .lock()
                .map_err(|_| "failed to lock preview generation state".to_string())?;
            locks.insert(cache_path.clone())
        };

        if inserted {
            let result = generate();
            if let Ok(mut locks) = generation_locks().lock() {
                locks.remove(&cache_path);
            }
            return result;
        }

        if cache_path.exists() {
            return Ok(cache_path);
        }

        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

fn fnv1a_hex(bytes: &[u8]) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn run_jobowalls<I, S>(args: I) -> Result<String, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let binary = resolve_jobowalls_binary();
    let output = Command::new(&binary)
        .args(args)
        .output()
        .map_err(|error| format!("failed to start {}: {error}", binary.display()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let message = if !stderr.trim().is_empty() {
            stderr.trim().to_string()
        } else if !stdout.trim().is_empty() {
            stdout.trim().to_string()
        } else {
            format!("exited with {}", output.status)
        };
        return Err(format!("{} failed: {message}", binary.display()));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn resolve_jobowalls_binary() -> PathBuf {
    if let Some(path) = std::env::var_os("JOBOWALLS_BIN") {
        return PathBuf::from(path);
    }

    if let Ok(cwd) = std::env::current_dir() {
        for base in cwd.ancestors() {
            let candidate = base.join("target").join("debug").join("jobowalls");
            if candidate.exists() {
                return candidate;
            }
        }
    }

    PathBuf::from("jobowalls")
}

fn gui_state_path() -> Option<PathBuf> {
    if let Some(state_home) = std::env::var_os("XDG_STATE_HOME") {
        return Some(PathBuf::from(state_home).join("jobowalls").join("gui.json"));
    }

    dirs::home_dir().map(|home| {
        home.join(".local")
            .join("state")
            .join("jobowalls")
            .join("gui.json")
    })
}

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            resolve_startup_folder,
            scan_folder,
            get_status,
            preview_plan,
            apply_wallpaper,
            get_media_source,
            get_media_data_source,
            get_live_preview_source,
            get_live_preview_data_source,
            warm_live_preview,
            save_last_folder,
            close_picker,
        ])
        .setup(|app| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_focus();
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running jobowalls GUI");
}
