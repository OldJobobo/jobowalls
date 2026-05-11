use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    process::{Child, Command},
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex, OnceLock,
    },
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::AppHandle;

use base64::{engine::general_purpose, Engine as _};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum MediaKind {
    Static,
    Live,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum PreviewQuality {
    Fast,
    Balanced,
    Pretty,
}

impl PreviewQuality {
    fn profile(self) -> LivePreviewProfile {
        match self {
            Self::Fast => LivePreviewProfile {
                duration_secs: 2,
                fps: 6,
                width: 540,
                crf: 30,
                cache_name: "preview-fast-v1.mp4",
            },
            Self::Balanced => LivePreviewProfile {
                duration_secs: 3,
                fps: 8,
                width: 720,
                crf: 28,
                cache_name: "preview-balanced-v1.mp4",
            },
            Self::Pretty => LivePreviewProfile {
                duration_secs: 4,
                fps: 10,
                width: 1080,
                crf: 24,
                cache_name: "preview-pretty-v1.mp4",
            },
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct LivePreviewProfile {
    duration_secs: u8,
    fps: u8,
    width: u16,
    crf: u8,
    cache_name: &'static str,
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct OmarchyThemeColors {
    background: String,
    foreground: String,
    accent: String,
    selection_background: String,
    selection_foreground: String,
    muted: String,
    surface: String,
    surface_raised: String,
    warning: String,
    error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GuiConfig {
    #[serde(rename = "defaultMonitor", alias = "default_monitor")]
    default_monitor: String,
    #[serde(rename = "previewQuality", alias = "preview_quality")]
    preview_quality: PreviewQuality,
    #[serde(rename = "rememberLastFolder", alias = "remember_last_folder")]
    remember_last_folder: bool,
    #[serde(rename = "useOmarchyTheme", alias = "use_omarchy_theme")]
    use_omarchy_theme: bool,
    #[serde(rename = "windowWidth", alias = "window_width")]
    window_width: u32,
    #[serde(rename = "windowHeight", alias = "window_height")]
    window_height: u32,
    #[serde(rename = "livePreview", alias = "live_preview")]
    live_preview: bool,
}

impl Default for GuiConfig {
    fn default() -> Self {
        Self {
            default_monitor: "all".to_string(),
            preview_quality: PreviewQuality::Balanced,
            remember_last_folder: true,
            use_omarchy_theme: true,
            window_width: 1040,
            window_height: 620,
            live_preview: true,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct AppConfig {
    gui: GuiConfig,
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
    let config = load_gui_config();
    if let Some(path) = input_path
        .or_else(first_path_arg)
        .and_then(|path| existing_dir(PathBuf::from(path)))
    {
        return Ok(StartupFolder {
            path: Some(path.display().to_string()),
            source: "argument".to_string(),
        });
    }

    if config.remember_last_folder {
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
fn get_omarchy_theme_colors() -> Result<Option<OmarchyThemeColors>, String> {
    let Some(path) = omarchy_theme_colors_path() else {
        return Ok(None);
    };
    let Ok(raw) = fs::read_to_string(&path) else {
        return Ok(None);
    };
    parse_omarchy_theme_colors(&raw).map(Some)
}

#[tauri::command]
fn get_gui_config() -> Result<GuiConfig, String> {
    Ok(load_gui_config())
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

    let preview_path = match kind {
        MediaKind::Static => generate_static_preview(&path_buf).unwrap_or(path_buf),
        MediaKind::Live => generate_video_poster(&path_buf)?,
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
        generate_static_preview(&path_buf).unwrap_or(path_buf)
    };

    data_source(path, &preview_path)
}

#[tauri::command]
fn get_thumbnail_source(path: String) -> Result<MediaSource, String> {
    let path_buf = PathBuf::from(&path);
    let Some(kind) = classify_path(&path_buf) else {
        return Ok(MediaSource {
            path,
            src: None,
            reason: Some("unsupported media type".to_string()),
        });
    };

    let preview_path = match kind {
        MediaKind::Static => generate_static_thumbnail(&path_buf)?,
        MediaKind::Live => generate_video_thumbnail(&path_buf)?,
    };

    Ok(MediaSource {
        path,
        src: Some(preview_path.display().to_string()),
        reason: None,
    })
}

#[tauri::command]
fn get_thumbnail_data_source(path: String) -> Result<MediaSource, String> {
    let path_buf = PathBuf::from(&path);
    let Some(kind) = classify_path(&path_buf) else {
        return Ok(MediaSource {
            path,
            src: None,
            reason: Some("unsupported media type".to_string()),
        });
    };

    let preview_path = match kind {
        MediaKind::Static => generate_static_thumbnail(&path_buf)?,
        MediaKind::Live => generate_video_thumbnail(&path_buf)?,
    };

    data_source(path, &preview_path)
}

#[tauri::command]
fn get_live_preview_source(path: String, quality: PreviewQuality) -> Result<MediaSource, String> {
    let path_buf = PathBuf::from(&path);
    if !matches!(classify_path(&path_buf), Some(MediaKind::Live)) {
        return get_media_source(path);
    }

    let preview_path = match cached_video_animation_path(&path_buf, quality) {
        Ok(Some(path)) => path,
        _ => generate_video_animation(&path_buf, quality)?,
    };
    Ok(MediaSource {
        path,
        src: Some(preview_path.display().to_string()),
        reason: None,
    })
}

#[tauri::command]
fn get_live_preview_data_source(
    path: String,
    quality: PreviewQuality,
) -> Result<MediaSource, String> {
    let path_buf = PathBuf::from(&path);
    if !matches!(classify_path(&path_buf), Some(MediaKind::Live)) {
        return get_media_data_source(path);
    }

    let preview_path = match cached_video_animation_path(&path_buf, quality) {
        Ok(Some(path)) => path,
        _ => generate_video_animation(&path_buf, quality)?,
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
        let _ = generate_video_thumbnail(&path_buf);
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

fn generate_static_preview(path: &Path) -> Result<PathBuf, String> {
    let cache_path = static_preview_cache_path(path)?;
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

    with_generation_lock(cache_path.clone(), || {
        if cache_path.exists() {
            return Ok(cache_path);
        }

        let status = match Command::new("ffmpeg")
            .args(["-y", "-hide_banner", "-loglevel", "error", "-i"])
            .arg(path)
            .args([
                "-frames:v",
                "1",
                "-vf",
                "scale=960:-1:flags=fast_bilinear",
                "-q:v",
                "5",
            ])
            .arg(&cache_path)
            .status()
        {
            Ok(status) => status,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err("ffmpeg is required for static thumbnail generation".to_string());
            }
            Err(error) => return Err(format!("failed to start ffmpeg: {error}")),
        };

        if status.success() && cache_path.exists() {
            Ok(cache_path)
        } else {
            Err(format!(
                "failed to generate static preview for {}",
                path.display()
            ))
        }
    })
}

fn generate_static_thumbnail(path: &Path) -> Result<PathBuf, String> {
    let cache_path = static_thumbnail_cache_path(path)?;
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

    with_generation_lock(cache_path.clone(), || {
        if cache_path.exists() {
            return Ok(cache_path);
        }

        let status = match Command::new("ffmpeg")
            .args(["-y", "-hide_banner", "-loglevel", "error", "-i"])
            .arg(path)
            .args([
                "-frames:v",
                "1",
                "-vf",
                "scale=520:-1:flags=fast_bilinear",
                "-q:v",
                "5",
            ])
            .arg(&cache_path)
            .status()
        {
            Ok(status) => status,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(path.to_path_buf());
            }
            Err(error) => return Err(format!("failed to start ffmpeg: {error}")),
        };

        if status.success() && cache_path.exists() {
            Ok(cache_path)
        } else {
            Ok(path.to_path_buf())
        }
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
        Some("mp4") => "video/mp4",
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

fn generate_video_thumbnail(path: &Path) -> Result<PathBuf, String> {
    let cache_path = video_thumbnail_cache_path(path)?;
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

    if try_ffmpegthumbnailer_with_size(path, &cache_path, 520)?
        || try_ffmpeg_with_size(path, &cache_path, 520)?
    {
        return Ok(cache_path);
    }

    generate_video_poster(path)
}

fn generate_video_animation(path: &Path, quality: PreviewQuality) -> Result<PathBuf, String> {
    let token = next_live_preview_token();
    cancel_stale_live_preview(token);

    let profile = quality.profile();
    let cache_path = video_animation_cache_path(path, quality)?;
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

    let _live_generation = live_preview_generation_lock()
        .lock()
        .map_err(|_| "failed to lock live preview generation".to_string())?;

    with_generation_lock(cache_path.clone(), || {
        if cache_path.exists() {
            return Ok(cache_path);
        }

        let _ = fs::remove_file(&cache_path);
        let duration = profile.duration_secs.to_string();
        let filter = format!(
            "fps={},scale={}:-2:flags=fast_bilinear",
            profile.fps, profile.width
        );
        let crf = profile.crf.to_string();
        let child = match Command::new("ffmpeg")
            .args([
                "-y",
                "-hide_banner",
                "-loglevel",
                "error",
                "-ss",
                "00:00:00.5",
                "-t",
                &duration,
                "-i",
            ])
            .arg(path)
            .args([
                "-vf",
                &filter,
                "-an",
                "-c:v",
                "libx264",
                "-preset",
                "veryfast",
                "-crf",
                &crf,
                "-pix_fmt",
                "yuv420p",
                "-movflags",
                "+faststart",
            ])
            .arg(&cache_path)
            .spawn()
        {
            Ok(child) => child,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return generate_video_poster(path);
            }
            Err(error) => return Err(format!("failed to start ffmpeg: {error}")),
        };
        record_live_preview_child(token, child);

        let status = loop {
            if live_preview_token().load(Ordering::SeqCst) != token {
                cancel_recorded_live_preview(token);
                let _ = fs::remove_file(&cache_path);
                return Err("stale live preview request cancelled".to_string());
            }

            let Some(status) = poll_recorded_live_preview(token)? else {
                std::thread::sleep(std::time::Duration::from_millis(35));
                continue;
            };
            break status;
        };
        clear_recorded_live_preview(token);

        if status.success() && cache_path.exists() {
            Ok(cache_path)
        } else {
            let _ = fs::remove_file(&cache_path);
            generate_video_poster(path)
        }
    })
}

fn cached_video_animation_path(
    path: &Path,
    quality: PreviewQuality,
) -> Result<Option<PathBuf>, String> {
    let cache_path = video_animation_cache_path(path, quality)?;
    Ok(cache_path.exists().then_some(cache_path))
}

struct ActiveLivePreview {
    token: u64,
    child: Child,
}

fn live_preview_token() -> &'static AtomicU64 {
    static TOKEN: AtomicU64 = AtomicU64::new(0);
    &TOKEN
}

fn next_live_preview_token() -> u64 {
    live_preview_token().fetch_add(1, Ordering::SeqCst) + 1
}

fn live_preview_generation_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn active_live_preview() -> &'static Mutex<Option<ActiveLivePreview>> {
    static ACTIVE: OnceLock<Mutex<Option<ActiveLivePreview>>> = OnceLock::new();
    ACTIVE.get_or_init(|| Mutex::new(None))
}

fn cancel_stale_live_preview(token: u64) {
    let Ok(mut active) = active_live_preview().lock() else {
        return;
    };
    if let Some(active_preview) = active.as_mut() {
        if active_preview.token != token {
            let _ = active_preview.child.kill();
        }
    }
}

fn record_live_preview_child(token: u64, child: Child) {
    if let Ok(mut active) = active_live_preview().lock() {
        if let Some(active_preview) = active.as_mut() {
            let _ = active_preview.child.kill();
        }
        *active = Some(ActiveLivePreview { token, child });
    }
}

fn poll_recorded_live_preview(token: u64) -> Result<Option<std::process::ExitStatus>, String> {
    let mut active = active_live_preview()
        .lock()
        .map_err(|_| "failed to lock active live preview".to_string())?;
    let Some(active_preview) = active.as_mut() else {
        return Ok(None);
    };
    if active_preview.token != token {
        return Ok(None);
    }

    active_preview
        .child
        .try_wait()
        .map_err(|error| format!("failed to poll ffmpeg preview process: {error}"))
}

fn cancel_recorded_live_preview(token: u64) {
    let Ok(mut active) = active_live_preview().lock() else {
        return;
    };
    if let Some(active_preview) = active.as_mut() {
        if active_preview.token == token {
            let _ = active_preview.child.kill();
        }
    }
}

fn clear_recorded_live_preview(token: u64) {
    let Ok(mut active) = active_live_preview().lock() else {
        return;
    };
    if active.as_ref().is_some_and(|active| active.token == token) {
        *active = None;
    }
}

fn try_ffmpegthumbnailer(input: &Path, output: &Path) -> Result<bool, String> {
    try_ffmpegthumbnailer_with_size(input, output, 960)
}

fn try_ffmpegthumbnailer_with_size(input: &Path, output: &Path, size: u32) -> Result<bool, String> {
    let status = match Command::new("ffmpegthumbnailer")
        .args(["-i"])
        .arg(input)
        .args(["-o"])
        .arg(output)
        .args(["-s", &size.to_string(), "-q", "8", "-t", "10%"])
        .status()
    {
        Ok(status) => status,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(format!("failed to start ffmpegthumbnailer: {error}")),
    };

    Ok(status.success() && output.exists())
}

fn try_ffmpeg(input: &Path, output: &Path) -> Result<bool, String> {
    try_ffmpeg_with_size(input, output, 960)
}

fn try_ffmpeg_with_size(input: &Path, output: &Path, width: u32) -> Result<bool, String> {
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
        .args([
            "-frames:v",
            "1",
            "-vf",
            &format!("scale={width}:-1:flags=fast_bilinear"),
            "-q:v",
            "5",
        ])
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
    video_cache_path(path, "poster-v3.jpg")
}

fn video_thumbnail_cache_path(path: &Path) -> Result<PathBuf, String> {
    video_cache_path(path, "thumb-v1.jpg")
}

fn static_preview_cache_path(path: &Path) -> Result<PathBuf, String> {
    video_cache_path(path, "static-v3.jpg")
}

fn static_thumbnail_cache_path(path: &Path) -> Result<PathBuf, String> {
    video_cache_path(path, "static-thumb-v1.jpg")
}

fn video_animation_cache_path(path: &Path, quality: PreviewQuality) -> Result<PathBuf, String> {
    video_cache_path(path, quality.profile().cache_name)
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

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join("jobowalls");
            if candidate.exists() {
                return candidate;
            }
        }
    }

    if let Ok(cwd) = std::env::current_dir() {
        for base in cwd.ancestors() {
            let candidate = base.join("target").join("debug").join("jobowalls");
            if candidate.exists() {
                return candidate;
            }
        }
    }

    if let Some(home) = dirs::home_dir() {
        let candidate = home.join(".local").join("bin").join("jobowalls");
        if candidate.exists() {
            return candidate;
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

fn omarchy_theme_colors_path() -> Option<PathBuf> {
    if let Some(config_home) = std::env::var_os("XDG_CONFIG_HOME") {
        return Some(
            PathBuf::from(config_home)
                .join("omarchy")
                .join("current")
                .join("theme")
                .join("colors.toml"),
        );
    }

    dirs::home_dir().map(|home| {
        home.join(".config")
            .join("omarchy")
            .join("current")
            .join("theme")
            .join("colors.toml")
    })
}

fn default_config_path() -> Option<PathBuf> {
    if let Some(config_home) = std::env::var_os("XDG_CONFIG_HOME") {
        return Some(PathBuf::from(config_home).join("jobowalls").join("config.toml"));
    }

    dirs::home_dir().map(|home| home.join(".config").join("jobowalls").join("config.toml"))
}

fn load_gui_config() -> GuiConfig {
    let Some(path) = default_config_path() else {
        return GuiConfig::default();
    };
    let Ok(raw) = fs::read_to_string(path) else {
        return GuiConfig::default();
    };
    toml::from_str::<AppConfig>(&raw)
        .map(|config| config.gui.normalized())
        .unwrap_or_default()
}

impl GuiConfig {
    fn normalized(mut self) -> Self {
        if self.default_monitor.trim().is_empty() {
            self.default_monitor = "all".to_string();
        }
        if self.window_width == 0 {
            self.window_width = 1040;
        }
        if self.window_height == 0 {
            self.window_height = 620;
        }
        self
    }
}

fn parse_omarchy_theme_colors(raw: &str) -> Result<OmarchyThemeColors, String> {
    let colors = toml::from_str::<HashMap<String, String>>(raw)
        .map_err(|error| format!("failed to parse Omarchy theme colors: {error}"))?;

    let color = |key: &str, fallback: &str| {
        colors
            .get(key)
            .filter(|value| is_hex_color(value))
            .cloned()
            .unwrap_or_else(|| fallback.to_string())
    };

    Ok(OmarchyThemeColors {
        background: color("background", "#0c0d0f"),
        foreground: color("foreground", "#eef0f2"),
        accent: color("accent", "#7cc7b2"),
        selection_background: color("selection_background", "#7cc7b2"),
        selection_foreground: color("selection_foreground", "#0c0d0f"),
        muted: color("color8", "#8f98a2"),
        surface: color("color0", "#171a1e"),
        surface_raised: color("color8", "#2a3038"),
        warning: color("color3", "#e0a85c"),
        error: color("color1", "#d36a72"),
    })
}

fn is_hex_color(value: &str) -> bool {
    let bytes = value.as_bytes();
    matches!(bytes.len(), 7 | 9)
        && bytes[0] == b'#'
        && bytes[1..].iter().all(|byte| byte.is_ascii_hexdigit())
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            resolve_startup_folder,
            scan_folder,
            get_omarchy_theme_colors,
            get_gui_config,
            get_status,
            preview_plan,
            apply_wallpaper,
            get_media_source,
            get_media_data_source,
            get_thumbnail_source,
            get_thumbnail_data_source,
            get_live_preview_source,
            get_live_preview_data_source,
            warm_live_preview,
            save_last_folder,
            close_picker,
        ])
        .run(tauri::generate_context!())
        .expect("error while running jobowalls GUI");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_home_prefixes() {
        let Some(home) = dirs::home_dir() else {
            return;
        };

        assert_eq!(expand_home(PathBuf::from("~")), home);
        assert_eq!(
            expand_home(PathBuf::from("~/Pictures")),
            home.join("Pictures")
        );
        assert_eq!(
            expand_home(PathBuf::from("/tmp/walls")),
            PathBuf::from("/tmp/walls")
        );
    }

    #[test]
    fn scans_supported_wallpapers_sorted_case_insensitively() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("zeta.mp4"), b"video").unwrap();
        fs::write(dir.path().join("Alpha.PNG"), b"image").unwrap();
        fs::write(dir.path().join("notes.txt"), b"notes").unwrap();
        fs::create_dir(dir.path().join("nested")).unwrap();

        let items = scan_folder(dir.path().display().to_string()).unwrap();

        assert_eq!(items.len(), 2);
        assert_eq!(items[0].name, "Alpha.PNG");
        assert_eq!(items[0].kind, MediaKind::Static);
        assert_eq!(items[1].name, "zeta.mp4");
        assert_eq!(items[1].kind, MediaKind::Live);
    }

    #[test]
    fn rejects_scan_folder_for_non_directory() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("wall.png");
        fs::write(&path, b"image").unwrap();

        let error = scan_folder(path.display().to_string()).unwrap_err();

        assert!(error.contains("not a folder"));
    }

    #[test]
    fn classifies_gui_media_extensions() {
        assert_eq!(
            classify_path(Path::new("wall.JPG")),
            Some(MediaKind::Static)
        );
        assert_eq!(classify_path(Path::new("rain.WEBM")), Some(MediaKind::Live));
        assert_eq!(classify_path(Path::new("notes.txt")), None);
    }

    #[test]
    fn chooses_preview_mime_from_path_extension() {
        assert_eq!(mime_for_path(Path::new("wall.jpg")), "image/jpeg");
        assert_eq!(mime_for_path(Path::new("wall.webp")), "image/webp");
        assert_eq!(mime_for_path(Path::new("wall.gif")), "image/gif");
        assert_eq!(mime_for_path(Path::new("wall.bmp")), "image/bmp");
        assert_eq!(mime_for_path(Path::new("wall.png")), "image/png");
    }

    #[test]
    fn parses_omarchy_theme_colors_with_fallbacks() {
        let colors = parse_omarchy_theme_colors(
            r##"
background = "#181a1f"
foreground = "#b9bec6"
accent = "#ad2222"
selection_background = "#9e1a1a"
selection_foreground = "#181a1f"
color8 = "#4b515b"
color3 = "not-a-color"
color1 = "#b31414"
"##,
        )
        .unwrap();

        assert_eq!(colors.background, "#181a1f");
        assert_eq!(colors.foreground, "#b9bec6");
        assert_eq!(colors.accent, "#ad2222");
        assert_eq!(colors.selection_background, "#9e1a1a");
        assert_eq!(colors.selection_foreground, "#181a1f");
        assert_eq!(colors.muted, "#4b515b");
        assert_eq!(colors.warning, "#e0a85c");
        assert_eq!(colors.error, "#b31414");
    }

    #[test]
    fn parses_gui_config_section() {
        let config: AppConfig = toml::from_str(
            r#"
            [gui]
            default_monitor = "DP-1"
            preview_quality = "pretty"
            remember_last_folder = false
            use_omarchy_theme = false
            window_width = 900
            window_height = 540
            live_preview = false
            "#,
        )
        .unwrap();

        let gui = config.gui.normalized();
        assert_eq!(gui.default_monitor, "DP-1");
        assert_eq!(gui.preview_quality, PreviewQuality::Pretty);
        assert!(!gui.remember_last_folder);
        assert!(!gui.use_omarchy_theme);
        assert_eq!(gui.window_width, 900);
        assert_eq!(gui.window_height, 540);
        assert!(!gui.live_preview);
    }

    #[test]
    fn builds_data_source_with_base64_payload() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("wall.png");
        fs::write(&path, b"abc").unwrap();

        let source = data_source("original.png".to_string(), &path).unwrap();

        assert_eq!(source.path, "original.png");
        assert_eq!(source.reason, None);
        assert_eq!(source.src.as_deref(), Some("data:image/png;base64,YWJj"));
    }

    #[test]
    fn cache_path_is_stable_for_unchanged_file_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rain.mp4");
        fs::write(&path, b"same").unwrap();

        let first = video_poster_cache_path(&path).unwrap();
        let second = video_poster_cache_path(&path).unwrap();

        assert_eq!(first, second);
        assert!(first.to_string_lossy().ends_with(".poster-v3.jpg"));
    }

    #[test]
    fn cache_path_changes_when_file_size_changes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rain.mp4");
        fs::write(&path, b"short").unwrap();
        let first = video_poster_cache_path(&path).unwrap();

        fs::write(&path, b"longer content").unwrap();
        let second = video_poster_cache_path(&path).unwrap();

        assert_ne!(first, second);
    }
}
