use crate::{
    backends::{model::Backend, mpvpaper, swaybg},
    command::{pid_is_running, signal_pid, terminate_pid},
    config::Config,
    media::MediaKind,
    orchestrator::SetPlan,
    shell::{
        apply::apply_wallpaper,
        cli::{ShellArgs, ShellPosition},
        layer,
        model::WallpaperItem,
        preview::{PreviewProfile, generate, prioritized_jobs},
        scanner::{resolve_folder, scan_folder},
        state::{ShellState, runtime_state_path, shell_state_path},
        widgets::{carousel, empty},
    },
    state::State,
};
use anyhow::Result;
use clap::Parser;
use gtk::{gdk, glib, prelude::*};
use serde_json::json;
use std::{
    cell::RefCell,
    collections::HashSet,
    fs,
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    process::Command,
    rc::Rc,
    sync::mpsc::{self, Receiver, RecvTimeoutError, Sender},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const NAV_ANIMATION_FRAMES: u8 = 10;
const DESKTOP_PREVIEW_NAV_DELAY_MS: u64 = 140;
const STATIC_DESKTOP_PREVIEW_DEBOUNCE_MS: u64 = 120;
const LIVE_DESKTOP_PREVIEW_DEBOUNCE_MS: u64 = 220;
const STATIC_PREVIEW_SETTLE_MS: u64 = 220;
const RETIRED_PREVIEW_CLEANUP_DELAY_MS: u64 = 450;

pub fn run() -> Result<()> {
    let args = ShellArgs::parse();
    let config = Config::load(&default_config_path())?;
    let args = args.apply_config_defaults(&config);
    let shell_state_path = shell_state_path();
    let shell_state = ShellState::load(&shell_state_path)?;
    let folder = resolve_folder(args.folder.clone(), Some(&shell_state))?;
    let items = scan_folder(&folder)?;
    let selected = initial_selection(&items, &folder, &shell_state);
    let active_wallpaper =
        State::load(&runtime_state_path())?.map(|state| PathBuf::from(state.wallpaper));
    let (preview_tx, preview_rx) = mpsc::channel();
    let (desktop_request_tx, desktop_request_rx) = mpsc::channel();
    let (desktop_result_tx, desktop_result_rx) = mpsc::channel();
    spawn_desktop_preview_worker(desktop_request_rx, desktop_result_tx);

    let app_state = Rc::new(RefCell::new(AppState {
        args,
        folder,
        items,
        selected,
        active_wallpaper: active_wallpaper.clone(),
        original_wallpaper: active_wallpaper.clone(),
        current_preview_wallpaper: active_wallpaper,
        shell_state,
        shell_state_path,
        config,
        status: None,
        queued_previews: HashSet::new(),
        preview_tx,
        desktop_request_tx,
        desktop_preview_generation: 0,
        animation_frame: 0,
        animation_direction: 1,
        previous_selected: None,
    }));

    let app = gtk::Application::builder()
        .application_id("dev.jobowalls.shell")
        .build();
    let preview_rx = Rc::new(RefCell::new(Some(preview_rx)));
    let desktop_result_rx = Rc::new(RefCell::new(Some(desktop_result_rx)));

    app.connect_activate(move |app| {
        let preview_rx = preview_rx.borrow_mut().take();
        let desktop_result_rx = desktop_result_rx.borrow_mut().take();
        if let Err(error) = build_ui(app, app_state.clone(), preview_rx, desktop_result_rx) {
            eprintln!("{error:#}");
            app.quit();
        }
    });

    app.run_with_args(&["jobowalls-shell"]);
    Ok(())
}

struct AppState {
    args: ShellArgs,
    folder: PathBuf,
    items: Vec<WallpaperItem>,
    selected: usize,
    active_wallpaper: Option<PathBuf>,
    original_wallpaper: Option<PathBuf>,
    current_preview_wallpaper: Option<PathBuf>,
    shell_state: ShellState,
    shell_state_path: PathBuf,
    config: Config,
    status: Option<String>,
    queued_previews: HashSet<PathBuf>,
    preview_tx: Sender<()>,
    desktop_request_tx: Sender<DesktopPreviewRequest>,
    desktop_preview_generation: u64,
    animation_frame: u8,
    animation_direction: isize,
    previous_selected: Option<usize>,
}

#[derive(Debug, Clone)]
enum DesktopPreviewRequest {
    Preview(Box<DesktopPreviewJob>),
    Stop(Sender<()>),
}

#[derive(Debug, Clone)]
struct DesktopPreviewJob {
    path: PathBuf,
    monitor: String,
    generation: u64,
    debounce_ms: u64,
    is_live: bool,
    config: Config,
}

#[derive(Debug)]
struct DesktopPreviewResult {
    path: PathBuf,
    generation: u64,
    result: std::result::Result<(), String>,
}

fn build_ui(
    app: &gtk::Application,
    state: Rc<RefCell<AppState>>,
    preview_rx: Option<mpsc::Receiver<()>>,
    desktop_result_rx: Option<mpsc::Receiver<DesktopPreviewResult>>,
) -> Result<()> {
    load_css();

    let args = state.borrow().args.clone();
    let window_width = if args.width > 0 {
        args.width
    } else {
        carousel::STAGE_WIDTH
    };
    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title("JoboWalls Shell")
        .default_width(window_width)
        .default_height(args.height())
        .focusable(true)
        .focus_on_click(true)
        .resizable(false)
        .build();
    if args.debug_window {
        window.set_default_size(window_width, args.height());
    }
    window.add_css_class("shell-window");
    layer::configure(&window, &args);

    render(&window, state.clone());
    if let Some(preview_rx) = preview_rx {
        install_preview_poll(&window, state.clone(), preview_rx);
    }
    if let Some(desktop_result_rx) = desktop_result_rx {
        install_desktop_preview_poll(&window, state.clone(), desktop_result_rx);
    }
    install_animation_tick(&window, state.clone());
    install_keys(&window, state.clone());
    install_outside_cancel(&window, state.clone());
    install_close_cleanup(&window, state.clone());
    schedule_desktop_preview(&state);
    window.present();
    window.present_with_time(0);
    request_window_focus(&window);
    Ok(())
}

fn render(window: &gtk::ApplicationWindow, state: Rc<RefCell<AppState>>) {
    let mut state_ref = state.borrow_mut();
    let root = gtk::Box::new(gtk::Orientation::Vertical, 8);
    root.add_css_class("shell-root");
    root.set_halign(gtk::Align::Center);
    root.set_valign(if state_ref.args.position() == ShellPosition::Center {
        gtk::Align::Center
    } else {
        gtk::Align::End
    });
    root.set_margin_bottom(if state_ref.args.position() == ShellPosition::Center {
        0
    } else {
        48
    });
    root.set_focusable(true);
    root.set_width_request(carousel::STAGE_WIDTH);
    root.set_height_request(state_ref.args.height());

    if state_ref.items.is_empty() {
        root.append(&empty::build("No wallpapers found"));
    } else {
        queue_preview_jobs(&mut state_ref);
        root.append(&carousel::build(
            &state_ref.items,
            state_ref.selected,
            state_ref.active_wallpaper.as_deref(),
            false,
            state_ref.previous_selected,
            animation_progress(&state_ref),
            state_ref.animation_direction,
        ));
    }

    let label = gtk::Label::new(state_ref.status.as_deref());
    label.add_css_class("status");
    label.set_halign(gtk::Align::Center);
    label.set_wrap(true);
    label.set_width_request(520);
    label.set_height_request(18);
    root.append(&label);

    install_keys_on(&root, window, state.clone());
    install_pointer_controls_on(&root, window, state.clone());
    window.set_child(Some(&root));
    request_widget_focus(&root);
}

fn request_window_focus(window: &gtk::ApplicationWindow) {
    let window_for_idle = window.clone();
    glib::idle_add_local_once(move || {
        window_for_idle.present();
        window_for_idle.grab_focus();
    });

    for delay_ms in [50, 150, 300] {
        let window_for_timeout = window.clone();
        glib::timeout_add_local_once(Duration::from_millis(delay_ms), move || {
            window_for_timeout.present();
            window_for_timeout.grab_focus();
        });
    }
}

fn request_widget_focus<W>(widget: &W)
where
    W: IsA<gtk::Widget>,
{
    let widget_for_idle = widget.clone().upcast::<gtk::Widget>();
    glib::idle_add_local_once(move || {
        widget_for_idle.grab_focus();
    });

    for delay_ms in [50, 150, 300] {
        let widget_for_timeout = widget.clone().upcast::<gtk::Widget>();
        glib::timeout_add_local_once(Duration::from_millis(delay_ms), move || {
            widget_for_timeout.grab_focus();
        });
    }
}

fn queue_preview_jobs(state: &mut AppState) {
    let jobs = prioritized_jobs(
        &state.items,
        state.selected,
        PreviewProfile::default(),
        false,
    );

    for job in jobs {
        if job.output.exists() || !state.queued_previews.insert(job.output.clone()) {
            continue;
        }

        let tx = state.preview_tx.clone();
        thread::spawn(move || {
            let _ = generate(&job);
            let _ = tx.send(());
        });
    }
}

fn spawn_desktop_preview_worker(
    request_rx: Receiver<DesktopPreviewRequest>,
    result_tx: Sender<DesktopPreviewResult>,
) {
    thread::spawn(move || {
        let mut live_preview_pid = None;
        let mut static_preview_pid = None;
        'worker: while let Ok(request) = request_rx.recv() {
            let mut request = match request {
                DesktopPreviewRequest::Preview(request) => *request,
                DesktopPreviewRequest::Stop(done_tx) => {
                    stop_live_preview(&mut live_preview_pid);
                    stop_static_preview(&mut static_preview_pid);
                    let _ = done_tx.send(());
                    continue 'worker;
                }
            };

            loop {
                match request_rx.recv_timeout(Duration::from_millis(request.debounce_ms)) {
                    Ok(DesktopPreviewRequest::Preview(next)) => {
                        request = *next;
                    }
                    Ok(DesktopPreviewRequest::Stop(done_tx)) => {
                        stop_live_preview(&mut live_preview_pid);
                        stop_static_preview(&mut static_preview_pid);
                        let _ = done_tx.send(());
                        continue 'worker;
                    }
                    Err(RecvTimeoutError::Timeout) => break,
                    Err(RecvTimeoutError::Disconnected) => break 'worker,
                }
            }

            let result = if request.is_live {
                apply_fast_live_preview(&request, &mut live_preview_pid, &mut static_preview_pid)
            } else {
                apply_static_desktop_preview(
                    &request,
                    &mut live_preview_pid,
                    &mut static_preview_pid,
                )
            };
            let _ = result_tx.send(DesktopPreviewResult {
                path: request.path,
                generation: request.generation,
                result,
            });
        }
        stop_live_preview(&mut live_preview_pid);
        stop_static_preview(&mut static_preview_pid);
    });
}

fn apply_static_desktop_preview(
    request: &DesktopPreviewJob,
    live_preview_pid: &mut Option<u32>,
    static_preview_pid: &mut Option<u32>,
) -> std::result::Result<(), String> {
    let plan = SetPlan {
        wallpaper: request.path.clone(),
        media_kind: MediaKind::Static,
        backend: Backend::Swaybg,
        monitor: request.monitor.clone(),
    };
    let command = swaybg::start_command(&plan);
    match command.spawn_detached() {
        Ok(pid) => {
            thread::sleep(Duration::from_millis(STATIC_PREVIEW_SETTLE_MS));
            if !pid_is_running(pid) {
                return Err(format!(
                    "desktop preview failed for {}: swaybg exited before settling",
                    request.path.display()
                ));
            }
            retire_live_preview(live_preview_pid);
            retire_static_preview(static_preview_pid);
            retire_blocking_live_for_desktop_preview(&request.monitor);
            retire_blocking_swaybg_for_live_preview(&request.monitor);
            *static_preview_pid = Some(pid);
            Ok(())
        }
        Err(error) => Err(format!(
            "desktop preview failed for {}: {error}",
            request.path.display()
        )),
    }
}

fn apply_fast_live_preview(
    request: &DesktopPreviewJob,
    live_preview_pid: &mut Option<u32>,
    static_preview_pid: &mut Option<u32>,
) -> std::result::Result<(), String> {
    let plan = SetPlan {
        wallpaper: request.path.clone(),
        media_kind: MediaKind::Live,
        backend: Backend::Mpvpaper,
        monitor: request.monitor.clone(),
    };
    let ipc_socket = mpvpaper_preview_ipc_socket(&plan);
    remove_stale_preview_socket(&ipc_socket)?;
    let command = mpvpaper::start_command_with_ipc(&plan, &request.config.mpvpaper, &ipc_socket);
    match command.spawn_detached() {
        Ok(pid) => {
            if let Err(error) = wait_for_preview_mpvpaper_readiness(
                pid,
                &ipc_socket,
                request.config.mpvpaper.readiness_timeout_ms,
            ) {
                let _ = terminate_pid(pid);
                return Err(format!(
                    "desktop preview failed for {}: {error}",
                    request.path.display()
                ));
            }
            stop_live_preview(live_preview_pid);
            retire_static_preview(static_preview_pid);
            retire_blocking_live_for_desktop_preview(&request.monitor);
            retire_blocking_swaybg_for_live_preview(&request.monitor);
            *live_preview_pid = Some(pid);
            Ok(())
        }
        Err(error) => Err(format!(
            "desktop preview failed for {}: {error}",
            request.path.display()
        )),
    }
}

fn mpvpaper_preview_ipc_socket(plan: &SetPlan) -> PathBuf {
    let monitor = plan
        .monitor
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);

    std::env::temp_dir().join(format!(
        "jobowalls-shell-mpvpaper-{}-{monitor}-{now}.sock",
        std::process::id()
    ))
}

fn remove_stale_preview_socket(path: &Path) -> std::result::Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!(
            "failed to remove stale mpv IPC socket {}: {error}",
            path.display()
        )),
    }
}

fn wait_for_preview_mpvpaper_readiness(
    pid: u32,
    ipc_socket: &Path,
    timeout_ms: u64,
) -> std::result::Result<(), String> {
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);

    loop {
        if !pid_is_running(pid) {
            return Err(format!(
                "mpvpaper pid {pid} exited before reporting readiness"
            ));
        }

        match query_preview_mpv_video_output(ipc_socket) {
            Ok(true) => return Ok(()),
            Ok(false) => {}
            Err(error) if Instant::now() < deadline => {
                let _ = error;
            }
            Err(error) => {
                return Err(format!(
                    "failed to query mpv IPC {}: {error}",
                    ipc_socket.display()
                ));
            }
        }

        if Instant::now() >= deadline {
            return Err(format!(
                "timed out waiting for mpv video output on pid {pid}"
            ));
        }

        thread::sleep(Duration::from_millis(50));
    }
}

fn query_preview_mpv_video_output(ipc_socket: &Path) -> std::result::Result<bool, String> {
    let mut stream = UnixStream::connect(ipc_socket).map_err(|error| error.to_string())?;
    stream
        .set_read_timeout(Some(Duration::from_millis(200)))
        .map_err(|error| error.to_string())?;
    stream
        .set_write_timeout(Some(Duration::from_millis(200)))
        .map_err(|error| error.to_string())?;
    let request = json!({
        "command": ["get_property", "video-out-params"],
        "request_id": 1,
    });
    writeln!(stream, "{request}").map_err(|error| error.to_string())?;

    let mut response = String::new();
    BufReader::new(stream)
        .read_line(&mut response)
        .map_err(|error| error.to_string())?;

    let response: serde_json::Value =
        serde_json::from_str(&response).map_err(|error| error.to_string())?;
    if response.get("error").and_then(|error| error.as_str()) != Some("success") {
        return Ok(false);
    }

    Ok(!response.get("data").is_none_or(|data| data.is_null()))
}

fn retire_blocking_swaybg_for_live_preview(monitor: &str) {
    for pid in blocking_swaybg_pids(&runtime_state_path(), monitor) {
        retire_preview_blocker(pid);
    }
}

fn retire_blocking_live_for_desktop_preview(monitor: &str) {
    for pid in blocking_live_pids(&runtime_state_path(), monitor) {
        retire_preview_blocker(pid);
    }
}

fn blocking_live_pids(state_path: &Path, monitor: &str) -> Vec<u32> {
    let Ok(Some(state)) = State::load(state_path) else {
        return Vec::new();
    };

    let mut pids = state_live_pids_for_monitor(&state, monitor);
    pids.sort_unstable();
    pids.dedup();
    pids
}

fn blocking_swaybg_pids(state_path: &Path, monitor: &str) -> Vec<u32> {
    let mut pids = Vec::new();
    if let Ok(Some(state)) = State::load(state_path) {
        pids.extend(state_swaybg_pids_for_monitor(&state, monitor));
    }

    pids.extend(omarchy_swaybg_pids());
    pids.sort_unstable();
    pids.dedup();
    pids
}

fn state_swaybg_pids_for_monitor(state: &State, monitor: &str) -> Vec<u32> {
    state
        .monitors
        .iter()
        .filter_map(|(name, monitor_state)| {
            if monitor != "all" && monitor != name {
                return None;
            }
            (monitor_state.backend == Backend::Swaybg)
                .then_some(monitor_state.pid)
                .flatten()
        })
        .collect()
}

fn state_live_pids_for_monitor(state: &State, monitor: &str) -> Vec<u32> {
    state
        .monitors
        .iter()
        .filter_map(|(name, monitor_state)| {
            if monitor != "all" && monitor != name {
                return None;
            }
            (monitor_state.backend == Backend::Mpvpaper)
                .then_some(monitor_state.pid)
                .flatten()
        })
        .collect()
}

fn omarchy_swaybg_pids() -> Vec<u32> {
    let Ok(output) = Command::new("ps")
        .args(["-eo", "pid=", "-o", "args="])
        .output()
    else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }

    let home = dirs::home_dir();
    let output = String::from_utf8_lossy(&output.stdout);
    output
        .lines()
        .filter_map(|line| parse_omarchy_swaybg_pid(line, home.as_deref()))
        .collect()
}

fn parse_omarchy_swaybg_pid(line: &str, home: Option<&Path>) -> Option<u32> {
    let line = line.trim();
    let (pid, command) = line.split_once(char::is_whitespace)?;
    let pid = pid.parse().ok()?;
    if !command
        .split_whitespace()
        .next()
        .is_some_and(|program| program.ends_with("swaybg"))
    {
        return None;
    }
    if !command.split_whitespace().any(|arg| arg == "-i") {
        return None;
    }

    let home = home?;
    let omarchy_background = home.join(".config/omarchy/current/background");
    command
        .contains(&omarchy_background.display().to_string())
        .then_some(pid)
}

fn stop_live_preview(live_preview_pid: &mut Option<u32>) {
    if let Some(pid) = live_preview_pid.take() {
        terminate_preview_blocker(pid);
    }
}

fn retire_live_preview(live_preview_pid: &mut Option<u32>) {
    if let Some(pid) = live_preview_pid.take() {
        retire_preview_blocker(pid);
    }
}

fn stop_static_preview(static_preview_pid: &mut Option<u32>) {
    if let Some(pid) = static_preview_pid.take() {
        terminate_preview_blocker(pid);
    }
}

fn retire_static_preview(static_preview_pid: &mut Option<u32>) {
    if let Some(pid) = static_preview_pid.take() {
        retire_preview_blocker(pid);
    }
}

fn retire_preview_blocker(pid: u32) {
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(RETIRED_PREVIEW_CLEANUP_DELAY_MS));
        terminate_preview_blocker(pid);
    });
}

fn terminate_preview_blocker(pid: u32) {
    let _ = terminate_pid(pid);
    let deadline = Instant::now() + Duration::from_millis(500);
    while pid_is_running(pid) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(25));
    }
    if pid_is_running(pid) {
        let _ = signal_pid(pid, "KILL");
    }
}

fn animation_progress(state: &AppState) -> f64 {
    if state.animation_frame == 0 || NAV_ANIMATION_FRAMES == 0 {
        return 1.0;
    }

    let progress = 1.0 - (f64::from(state.animation_frame) / f64::from(NAV_ANIMATION_FRAMES));
    progress * progress * (3.0 - 2.0 * progress)
}

fn install_preview_poll(
    window: &gtk::ApplicationWindow,
    state: Rc<RefCell<AppState>>,
    preview_rx: mpsc::Receiver<()>,
) {
    let window = window.clone();
    glib::timeout_add_local(Duration::from_millis(150), move || {
        let mut changed = false;
        while preview_rx.try_recv().is_ok() {
            changed = true;
        }

        if changed {
            render(&window, state.clone());
        }

        glib::ControlFlow::Continue
    });
}

fn install_desktop_preview_poll(
    window: &gtk::ApplicationWindow,
    state: Rc<RefCell<AppState>>,
    desktop_result_rx: mpsc::Receiver<DesktopPreviewResult>,
) {
    let window = window.clone();
    glib::timeout_add_local(Duration::from_millis(80), move || {
        let mut changed = false;
        while let Ok(result) = desktop_result_rx.try_recv() {
            let mut state = state.borrow_mut();
            if result.generation != state.desktop_preview_generation {
                continue;
            }

            changed = true;
            match result.result {
                Ok(()) => {
                    state.active_wallpaper = Some(result.path.clone());
                    state.current_preview_wallpaper = Some(result.path);
                }
                Err(error) => {
                    state.status = Some(error);
                }
            }
        }

        if changed {
            render(&window, state.clone());
        }

        glib::ControlFlow::Continue
    });
}

fn install_animation_tick(window: &gtk::ApplicationWindow, state: Rc<RefCell<AppState>>) {
    let window = window.clone();
    glib::timeout_add_local(Duration::from_millis(16), move || {
        let should_render = {
            let mut state = state.borrow_mut();
            if state.animation_frame == 0 {
                false
            } else {
                state.animation_frame -= 1;
                if state.animation_frame == 0 {
                    state.previous_selected = None;
                }
                true
            }
        };

        if should_render {
            render(&window, state.clone());
        }

        glib::ControlFlow::Continue
    });
}

fn install_keys(window: &gtk::ApplicationWindow, state: Rc<RefCell<AppState>>) {
    install_keys_on(window, window, state);
}

fn install_keys_on<W>(target: &W, window: &gtk::ApplicationWindow, state: Rc<RefCell<AppState>>)
where
    W: IsA<gtk::Widget>,
{
    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    let window_for_keys = window.clone();
    keys.connect_key_pressed(move |_, key, _, _| match key {
        gdk::Key::Escape => {
            restore_original_and_close(&state, &window_for_keys);
            glib::Propagation::Stop
        }
        gdk::Key::Left | gdk::Key::h | gdk::Key::H => {
            move_selection(&state, -1);
            render(&window_for_keys, state.clone());
            schedule_desktop_preview_after_navigation(state.clone());
            glib::Propagation::Stop
        }
        gdk::Key::Right | gdk::Key::l | gdk::Key::L => {
            move_selection(&state, 1);
            render(&window_for_keys, state.clone());
            schedule_desktop_preview_after_navigation(state.clone());
            glib::Propagation::Stop
        }
        gdk::Key::Return | gdk::Key::KP_Enter => {
            apply_selected(&state, &window_for_keys);
            glib::Propagation::Stop
        }
        gdk::Key::r | gdk::Key::R => {
            rescan(&state);
            render(&window_for_keys, state.clone());
            glib::Propagation::Stop
        }
        gdk::Key::s | gdk::Key::S => {
            shuffle_selection(&state);
            render(&window_for_keys, state.clone());
            schedule_desktop_preview_after_navigation(state.clone());
            glib::Propagation::Stop
        }
        _ => glib::Propagation::Proceed,
    });
    target.add_controller(keys);
}

fn install_pointer_controls_on<W>(
    target: &W,
    window: &gtk::ApplicationWindow,
    state: Rc<RefCell<AppState>>,
) where
    W: IsA<gtk::Widget>,
{
    let scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::BOTH_AXES);
    scroll.set_propagation_phase(gtk::PropagationPhase::Capture);
    let window_for_scroll = window.clone();
    let state_for_scroll = state.clone();
    scroll.connect_scroll(move |_, dx, dy| {
        if dx < 0.0 || dy < 0.0 {
            move_selection(&state_for_scroll, -1);
        } else if dx > 0.0 || dy > 0.0 {
            move_selection(&state_for_scroll, 1);
        }
        render(&window_for_scroll, state_for_scroll.clone());
        schedule_desktop_preview_after_navigation(state_for_scroll.clone());
        glib::Propagation::Stop
    });
    target.add_controller(scroll);

    let click = gtk::GestureClick::new();
    click.set_propagation_phase(gtk::PropagationPhase::Capture);
    let window_for_click = window.clone();
    click.connect_pressed(move |_, presses, x, _| {
        if presses >= 2 {
            apply_selected(&state, &window_for_click);
            return;
        }

        let width = f64::from(window_for_click.width()).max(1.0);
        if x < width / 3.0 {
            move_selection(&state, -1);
            render(&window_for_click, state.clone());
            schedule_desktop_preview_after_navigation(state.clone());
        } else if x > width * 2.0 / 3.0 {
            move_selection(&state, 1);
            render(&window_for_click, state.clone());
            schedule_desktop_preview_after_navigation(state.clone());
        }
    });
    target.add_controller(click);
}

fn install_outside_cancel(window: &gtk::ApplicationWindow, state: Rc<RefCell<AppState>>) {
    let click = gtk::GestureClick::new();
    click.set_propagation_phase(gtk::PropagationPhase::Bubble);
    let window_for_click = window.clone();
    let args = state.borrow().args.clone();
    click.connect_pressed(move |_, _, x, y| {
        if args.debug_window {
            return;
        }

        if !point_inside_shell_panel(
            f64::from(window_for_click.width()),
            f64::from(window_for_click.height()),
            &args,
            x,
            y,
        ) {
            restore_original_and_close(&state, &window_for_click);
        }
    });
    window.add_controller(click);
}

fn point_inside_shell_panel(
    window_width: f64,
    window_height: f64,
    args: &ShellArgs,
    x: f64,
    y: f64,
) -> bool {
    let panel_width = f64::from(carousel::STAGE_WIDTH);
    let panel_height = f64::from(args.height());
    let left = ((window_width - panel_width) / 2.0).max(0.0);
    let right = (left + panel_width).min(window_width.max(panel_width));
    let top = if args.position() == ShellPosition::Center {
        ((window_height - panel_height) / 2.0).max(0.0)
    } else {
        (window_height - panel_height - 48.0).max(0.0)
    };
    let bottom = (top + panel_height).min(window_height.max(panel_height));

    x >= left && x <= right && y >= top && y <= bottom
}

fn install_close_cleanup(window: &gtk::ApplicationWindow, state: Rc<RefCell<AppState>>) {
    window.connect_close_request(move |_| {
        stop_desktop_preview_worker(&state);
        glib::Propagation::Proceed
    });
}

fn move_selection(app_state: &Rc<RefCell<AppState>>, delta: isize) {
    let mut state = app_state.borrow_mut();
    let len = state.items.len();
    if len == 0 {
        return;
    }
    let previous = state.selected;
    state.selected = if delta < 0 {
        (state.selected + len - 1) % len
    } else {
        (state.selected + 1) % len
    };
    start_navigation_animation(&mut state, previous, delta);
    let folder = state.folder.clone();
    let monitor = state.args.monitor().to_string();
    let selected = state.selected;
    state.shell_state.remember(&folder, &monitor, selected);
    let _ = state.shell_state.save(&state.shell_state_path);
}

fn shuffle_selection(app_state: &Rc<RefCell<AppState>>) {
    let mut state = app_state.borrow_mut();
    let len = state.items.len();
    if len <= 1 {
        return;
    }

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.subsec_nanos() as usize)
        .unwrap_or_default();
    let mut selected = nanos % len;
    if selected == state.selected {
        selected = (selected + 1) % len;
    }
    let previous = state.selected;
    state.selected = selected;
    start_navigation_animation(&mut state, previous, 1);

    let folder = state.folder.clone();
    let monitor = state.args.monitor().to_string();
    state.shell_state.remember(&folder, &monitor, selected);
    let _ = state.shell_state.save(&state.shell_state_path);
}

fn start_navigation_animation(state: &mut AppState, previous: usize, delta: isize) {
    state.animation_frame = NAV_ANIMATION_FRAMES;
    state.animation_direction = delta.signum();
    state.previous_selected = Some(previous);
}

fn apply_selected(state: &Rc<RefCell<AppState>>, window: &gtk::ApplicationWindow) {
    let (path, monitor) = {
        let state = state.borrow();
        let Some(item) = state.items.get(state.selected) else {
            return;
        };
        (item.path.clone(), state.args.monitor().to_string())
    };

    stop_desktop_preview_worker(state);
    {
        let mut state = state.borrow_mut();
        state.status = Some("Applying...".to_string());
    }
    render(window, state.clone());

    match apply_wallpaper(&path, &monitor) {
        Ok(()) => {
            {
                let mut state = state.borrow_mut();
                state.active_wallpaper = Some(path.clone());
                state.current_preview_wallpaper = Some(path);
                state.original_wallpaper = state.current_preview_wallpaper.clone();
            }
            window.close();
        }
        Err(error) => {
            state.borrow_mut().status = Some(error.to_string());
            render(window, state.clone());
        }
    }
}

fn schedule_desktop_preview(state: &Rc<RefCell<AppState>>) {
    let request = {
        let mut state = state.borrow_mut();
        if state.args.no_live_preview {
            return;
        }

        let Some(item) = state.items.get(state.selected) else {
            return;
        };
        let path = item.path.clone();
        let is_live = item.is_live();
        let debounce_ms = if item.is_live() {
            LIVE_DESKTOP_PREVIEW_DEBOUNCE_MS
        } else {
            STATIC_DESKTOP_PREVIEW_DEBOUNCE_MS
        };
        if state.current_preview_wallpaper.as_ref() == Some(&path) {
            return;
        }

        state.desktop_preview_generation += 1;
        let generation = state.desktop_preview_generation;
        DesktopPreviewRequest::Preview(Box::new(DesktopPreviewJob {
            path,
            monitor: state.args.monitor().to_string(),
            generation,
            debounce_ms,
            is_live,
            config: state.config.clone(),
        }))
    };

    let _ = state.borrow().desktop_request_tx.send(request);
}

fn schedule_desktop_preview_after_navigation(state: Rc<RefCell<AppState>>) {
    glib::timeout_add_local_once(
        Duration::from_millis(DESKTOP_PREVIEW_NAV_DELAY_MS),
        move || {
            schedule_desktop_preview(&state);
        },
    );
}

fn restore_original_and_close(state: &Rc<RefCell<AppState>>, window: &gtk::ApplicationWindow) {
    let (original, monitor, needs_restore) = {
        let state = state.borrow();
        let original = state.original_wallpaper.clone();
        let needs_restore = original.is_some() && state.current_preview_wallpaper != original;
        (original, state.args.monitor().to_string(), needs_restore)
    };

    stop_desktop_preview_worker(state);
    if let Some(original) = original
        && needs_restore
    {
        state.borrow_mut().status = Some("Restoring...".to_string());
        render(window, state.clone());
        if let Err(error) = apply_wallpaper(&original, &monitor) {
            state.borrow_mut().status = Some(format!("restore failed: {error}"));
            render(window, state.clone());
            return;
        }
    }

    window.close();
}

fn stop_desktop_preview_worker(state: &Rc<RefCell<AppState>>) {
    let (done_tx, done_rx) = mpsc::channel();
    let _ = state
        .borrow()
        .desktop_request_tx
        .send(DesktopPreviewRequest::Stop(done_tx));
    let _ = done_rx.recv_timeout(Duration::from_millis(800));
}

fn rescan(state: &Rc<RefCell<AppState>>) {
    let folder = state.borrow().folder.clone();
    match scan_folder(&folder) {
        Ok(items) => {
            let mut state = state.borrow_mut();
            state.items = items;
            state.selected = state.selected.min(state.items.len().saturating_sub(1));
            state.status = None;
        }
        Err(error) => {
            state.borrow_mut().status = Some(error.to_string());
        }
    }
}

fn initial_selection(items: &[WallpaperItem], folder: &Path, shell_state: &ShellState) -> usize {
    if items.is_empty() {
        return 0;
    }

    if let Ok(Some(runtime)) = State::load(&runtime_state_path()) {
        let active = PathBuf::from(runtime.wallpaper);
        if let Some(index) = items.iter().position(|item| item.path == active) {
            return index;
        }
    }

    shell_state.remembered_index(folder, items.len())
}

fn load_css() {
    let provider = gtk::CssProvider::new();
    provider.load_from_data(include_str!("style.css"));
    if let Some(display) = gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}

fn default_config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("jobowalls")
        .join("config.toml")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::MonitorState;
    use std::{
        collections::BTreeMap,
        io::{BufRead, BufReader, Write},
        os::unix::net::UnixListener,
        thread,
    };

    #[test]
    fn parses_only_omarchy_current_background_swaybg_pid() {
        let home = Path::new("/home/tester");

        assert_eq!(
            parse_omarchy_swaybg_pid(
                "2106 /usr/bin/swaybg -i /home/tester/.config/omarchy/current/background -m fill",
                Some(home),
            ),
            Some(2106)
        );
        assert_eq!(
            parse_omarchy_swaybg_pid("2107 /usr/bin/swaybg -i /tmp/other.jpg -m fill", Some(home)),
            None
        );
        assert_eq!(
            parse_omarchy_swaybg_pid(
                "2108 /usr/bin/mpvpaper all /home/tester/.config/omarchy/current/background",
                Some(home),
            ),
            None
        );
    }

    #[test]
    fn state_swaybg_pids_filter_by_monitor_and_backend() {
        let state = State {
            version: 1,
            active_backend: Backend::Swaybg,
            mode: MediaKind::Static,
            wallpaper: "/tmp/a.png".to_string(),
            monitors: BTreeMap::from([
                (
                    "DP-1".to_string(),
                    MonitorState {
                        backend: Backend::Swaybg,
                        wallpaper: "/tmp/a.png".to_string(),
                        pid: Some(101),
                    },
                ),
                (
                    "DP-2".to_string(),
                    MonitorState {
                        backend: Backend::Mpvpaper,
                        wallpaper: "/tmp/b.mp4".to_string(),
                        pid: Some(202),
                    },
                ),
                (
                    "HDMI-A-1".to_string(),
                    MonitorState {
                        backend: Backend::Swaybg,
                        wallpaper: "/tmp/c.png".to_string(),
                        pid: None,
                    },
                ),
            ]),
            collections: BTreeMap::new(),
            last_command: None,
            updated_at: time::OffsetDateTime::UNIX_EPOCH,
        };

        assert_eq!(state_swaybg_pids_for_monitor(&state, "DP-1"), vec![101]);
        assert_eq!(
            state_swaybg_pids_for_monitor(&state, "DP-2"),
            Vec::<u32>::new()
        );
        assert_eq!(state_swaybg_pids_for_monitor(&state, "all"), vec![101]);
    }

    #[test]
    fn state_live_pids_filter_by_monitor_and_backend() {
        let state = State {
            version: 1,
            active_backend: Backend::Mpvpaper,
            mode: MediaKind::Live,
            wallpaper: "/tmp/a.mp4".to_string(),
            monitors: BTreeMap::from([
                (
                    "DP-1".to_string(),
                    MonitorState {
                        backend: Backend::Mpvpaper,
                        wallpaper: "/tmp/a.mp4".to_string(),
                        pid: Some(101),
                    },
                ),
                (
                    "DP-2".to_string(),
                    MonitorState {
                        backend: Backend::Swaybg,
                        wallpaper: "/tmp/b.png".to_string(),
                        pid: Some(202),
                    },
                ),
                (
                    "HDMI-A-1".to_string(),
                    MonitorState {
                        backend: Backend::Mpvpaper,
                        wallpaper: "/tmp/c.mp4".to_string(),
                        pid: None,
                    },
                ),
            ]),
            collections: BTreeMap::new(),
            last_command: None,
            updated_at: time::OffsetDateTime::UNIX_EPOCH,
        };

        assert_eq!(state_live_pids_for_monitor(&state, "DP-1"), vec![101]);
        assert_eq!(
            state_live_pids_for_monitor(&state, "DP-2"),
            Vec::<u32>::new()
        );
        assert_eq!(state_live_pids_for_monitor(&state, "all"), vec![101]);
    }

    #[test]
    fn outside_click_geometry_tracks_bottom_and_center_panel() {
        let bottom_args = ShellArgs {
            folder: None,
            monitor: None,
            position: Some(ShellPosition::Bottom),
            width: 0,
            height: Some(340),
            no_live_preview: false,
            debug_window: false,
        };
        assert!(point_inside_shell_panel(
            1920.0,
            1080.0,
            &bottom_args,
            960.0,
            860.0
        ));
        assert!(!point_inside_shell_panel(
            1920.0,
            1080.0,
            &bottom_args,
            960.0,
            500.0
        ));

        let center_args = ShellArgs {
            folder: None,
            monitor: None,
            position: Some(ShellPosition::Center),
            width: 0,
            height: Some(340),
            no_live_preview: false,
            debug_window: false,
        };
        assert!(point_inside_shell_panel(
            1920.0,
            1080.0,
            &center_args,
            960.0,
            540.0
        ));
        assert!(!point_inside_shell_panel(
            1920.0,
            1080.0,
            &center_args,
            960.0,
            900.0
        ));
    }

    #[test]
    fn mpvpaper_preview_ipc_socket_sanitizes_monitor_name() {
        let plan = SetPlan {
            wallpaper: PathBuf::from("/tmp/live.mp4"),
            media_kind: MediaKind::Live,
            backend: Backend::Mpvpaper,
            monitor: "DP-1:bad/name".to_string(),
        };

        let path = mpvpaper_preview_ipc_socket(&plan);
        let file_name = path.file_name().unwrap().to_string_lossy();

        assert!(file_name.starts_with("jobowalls-shell-mpvpaper-"));
        assert!(file_name.contains("-DP-1_bad_name-"));
        assert!(!file_name.contains(':'));
        assert!(!file_name.contains('/'));
    }

    #[test]
    fn query_preview_mpv_video_output_reports_ready_when_data_is_present() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("mpv.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = String::new();
            BufReader::new(stream.try_clone().unwrap())
                .read_line(&mut request)
                .unwrap();
            assert!(request.contains("video-out-params"));
            writeln!(
                stream,
                r#"{{"request_id":1,"error":"success","data":{{"w":1920,"h":1080}}}}"#
            )
            .unwrap();
        });

        assert_eq!(query_preview_mpv_video_output(&socket), Ok(true));
        server.join().unwrap();
    }

    #[test]
    fn query_preview_mpv_video_output_reports_not_ready_when_data_is_null() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("mpv.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = String::new();
            BufReader::new(stream.try_clone().unwrap())
                .read_line(&mut request)
                .unwrap();
            writeln!(
                stream,
                r#"{{"request_id":1,"error":"success","data":null}}"#
            )
            .unwrap();
        });

        assert_eq!(query_preview_mpv_video_output(&socket), Ok(false));
        server.join().unwrap();
    }
}
