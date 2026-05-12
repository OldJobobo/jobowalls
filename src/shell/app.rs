use crate::{
    backends::{model::Backend, mpvpaper, swaybg},
    command::{pid_is_running, signal_pid, terminate_pid},
    config::Config,
    media::MediaKind,
    orchestrator::SetPlan,
    shell::{
        apply::apply_wallpaper,
        cli::{ShellArgs, ShellLayout, ShellPosition},
        layer::{self, SurfaceDimensions},
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
    env, fs,
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
const LIVE_DESKTOP_PREVIEW_DEBOUNCE_MS: u64 = 700;
const STATIC_PREVIEW_SETTLE_MS: u64 = 220;
const RETIRED_PREVIEW_CLEANUP_DELAY_MS: u64 = 450;
const PICKER_SPACING: i32 = 8;
const STATUS_HEIGHT: i32 = 18;
const NAV_REPEAT_SNAP_MS: u64 = 110;

fn shell_debug_enabled() -> bool {
    env::var_os("JOBOWALLS_SHELL_DEBUG").is_some()
}

fn shell_debug(message: impl AsRef<str>) {
    if shell_debug_enabled() {
        eprintln!(
            "[jobowalls-shell {:?}] {}",
            Instant::now(),
            message.as_ref()
        );
    }
}

pub fn run() -> Result<()> {
    let args = ShellArgs::parse();
    let config = Config::load(&default_config_path())?;
    let args = args.apply_config_defaults(&config);
    shell_debug(format!(
        "startup monitor={} position={:?} layout={:?} debug_window={} live_preview={}",
        args.monitor(),
        args.position(),
        args.layout(),
        args.debug_window,
        !args.no_live_preview
    ));
    let shell_state_path = shell_state_path();
    let shell_state = ShellState::load(&shell_state_path)?;
    let folder = resolve_folder(args.folder.clone(), Some(&shell_state))?;
    let items = scan_folder(&folder)?;
    shell_debug(format!(
        "loaded folder={} items={}",
        folder.display(),
        items.len()
    ));
    let selected = initial_selection(&items, &folder, &shell_state);
    let current_wallpaper =
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
        original_wallpaper: current_wallpaper.clone(),
        current_preview_wallpaper: current_wallpaper,
        shell_state,
        shell_state_path,
        config,
        status: None,
        queued_previews: HashSet::new(),
        preview_tx,
        desktop_request_tx,
        desktop_preview_generation: 0,
        pending_desktop_preview_wallpaper: None,
        animation_frame: 0,
        animation_direction: 1,
        previous_selected: None,
        last_navigation: None,
        dismiss_suppressed_until: None,
        control_modifier_down: false,
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
    original_wallpaper: Option<PathBuf>,
    current_preview_wallpaper: Option<PathBuf>,
    shell_state: ShellState,
    shell_state_path: PathBuf,
    config: Config,
    status: Option<String>,
    queued_previews: HashSet<PathBuf>,
    preview_tx: Sender<PreviewCompletion>,
    desktop_request_tx: Sender<DesktopPreviewRequest>,
    desktop_preview_generation: u64,
    pending_desktop_preview_wallpaper: Option<PathBuf>,
    animation_frame: u8,
    animation_direction: isize,
    previous_selected: Option<usize>,
    last_navigation: Option<NavigationInput>,
    dismiss_suppressed_until: Option<Instant>,
    control_modifier_down: bool,
}

#[derive(Debug, Clone, Copy)]
struct NavigationInput {
    direction: isize,
    at: Instant,
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

#[derive(Debug)]
struct LivePreviewProcess {
    pid: u32,
    ipc_socket: PathBuf,
}

#[derive(Debug)]
struct PreviewCompletion {
    source: PathBuf,
    output: PathBuf,
    result: std::result::Result<(), String>,
}

fn build_ui(
    app: &gtk::Application,
    state: Rc<RefCell<AppState>>,
    preview_rx: Option<mpsc::Receiver<PreviewCompletion>>,
    desktop_result_rx: Option<mpsc::Receiver<DesktopPreviewResult>>,
) -> Result<()> {
    load_css();

    let args = state.borrow().args.clone();
    let target_monitor = shell_target_monitor(&args);
    let (window_width, window_height) = shell_panel_dimensions(&args);
    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title("JoboWalls Shell")
        .default_width(window_width)
        .default_height(window_height)
        .focusable(true)
        .focus_on_click(true)
        .resizable(false)
        .build();
    if args.debug_window {
        window.set_default_size(window_width, window_height);
    }
    window.add_css_class("shell-window");
    configure_shell_panel(&window, &args, target_monitor.as_ref());
    let click_catcher = if args.debug_window {
        None
    } else {
        Some(build_click_catcher(
            app,
            &args,
            target_monitor.as_ref(),
            state.clone(),
            &window,
        ))
    };

    render(&window, state.clone());
    if let Some(preview_rx) = preview_rx {
        install_preview_poll(&window, state.clone(), preview_rx);
    }
    if let Some(desktop_result_rx) = desktop_result_rx {
        install_desktop_preview_poll(&window, state.clone(), desktop_result_rx);
    }
    install_animation_tick(&window, state.clone());
    install_keys(&window, state.clone());
    install_close_cleanup(&window, state.clone(), click_catcher.clone());
    schedule_desktop_preview(&state);
    if let Some(click_catcher) = &click_catcher {
        click_catcher.present();
    }
    window.present();
    window.present_with_time(0);
    request_window_focus(&window);
    Ok(())
}

fn shell_panel_dimensions(args: &ShellArgs) -> (i32, i32) {
    let (stage_width, stage_height) = carousel::stage_dimensions(args.layout());
    let width = if args.width > 0 {
        args.width
    } else if args.layout() == ShellLayout::Vertical {
        carousel::STAGE_WIDTH
    } else {
        stage_width
    };
    let height = if args.layout() == ShellLayout::Vertical {
        stage_height + PICKER_SPACING + STATUS_HEIGHT
    } else {
        args.height()
    };
    (width, height)
}

fn configure_shell_panel(
    window: &gtk::ApplicationWindow,
    args: &ShellArgs,
    target_monitor: Option<&gdk::Monitor>,
) {
    let panel_dimensions = shell_panel_dimensions(args);
    let output_dimensions = monitor_dimensions_for(target_monitor)
        .or_else(monitor_dimensions)
        .unwrap_or(panel_dimensions);

    window.set_size_request(panel_dimensions.0, panel_dimensions.1);

    layer::configure_panel(
        window,
        args,
        target_monitor,
        SurfaceDimensions {
            width: panel_dimensions.0,
            height: panel_dimensions.1,
        },
        SurfaceDimensions {
            width: output_dimensions.0,
            height: output_dimensions.1,
        },
    );
}

fn build_click_catcher(
    app: &gtk::Application,
    args: &ShellArgs,
    monitor: Option<&gdk::Monitor>,
    state: Rc<RefCell<AppState>>,
    panel_window: &gtk::ApplicationWindow,
) -> gtk::ApplicationWindow {
    let (width, height) = monitor_dimensions_for(monitor)
        .or_else(monitor_dimensions)
        .unwrap_or_else(|| shell_panel_dimensions(args));
    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title("JoboWalls Shell Dismiss")
        .default_width(width)
        .default_height(height)
        .focusable(false)
        .resizable(false)
        .build();
    window.add_css_class("shell-window");
    window.set_size_request(width, height);
    layer::configure_click_catcher(&window, monitor);

    let surface = gtk::Box::new(gtk::Orientation::Vertical, 0);
    surface.add_css_class("shell-dismiss-surface");
    surface.set_hexpand(true);
    surface.set_vexpand(true);
    install_focus_on_pointer_motion(&surface, panel_window);
    install_cancel_on_click(&surface, panel_window, state);
    window.set_child(Some(&surface));
    window
}

fn shell_target_monitor(args: &ShellArgs) -> Option<gdk::Monitor> {
    let target = args.monitor();
    if target == "all" {
        return None;
    }

    shell_debug(format!("shell monitor target={target}"));
    monitor_by_connector(target)
}

fn monitor_dimensions() -> Option<(i32, i32)> {
    let display = gdk::Display::default()?;
    let monitors = display.monitors();
    let monitor = monitors.item(0)?.downcast::<gdk::Monitor>().ok()?;
    monitor_dimensions_for(Some(&monitor))
}

fn monitor_dimensions_for(monitor: Option<&gdk::Monitor>) -> Option<(i32, i32)> {
    let monitor = monitor?;
    let geometry = monitor.geometry();
    Some((geometry.width(), geometry.height()))
}

fn monitor_by_connector(target: &str) -> Option<gdk::Monitor> {
    let display = gdk::Display::default()?;
    let monitors = display.monitors();
    for index in 0..monitors.n_items() {
        let monitor = monitors.item(index)?.downcast::<gdk::Monitor>().ok()?;
        if monitor
            .connector()
            .is_some_and(|connector| connector.as_str() == target)
        {
            return Some(monitor);
        }
    }
    None
}

fn render(window: &gtk::ApplicationWindow, state: Rc<RefCell<AppState>>) {
    let mut state_ref = state.borrow_mut();
    shell_debug(format!(
        "render selected={} position={:?} layout={:?} animation_frame={} previous={:?}",
        state_ref.selected,
        state_ref.args.position(),
        state_ref.args.layout(),
        state_ref.animation_frame,
        state_ref.previous_selected
    ));
    let panel = gtk::Fixed::new();
    panel.add_css_class("shell-root");
    panel.set_focusable(true);
    let (panel_width, panel_height) = shell_panel_dimensions(&state_ref.args);
    let (content_width, content_height) = shell_content_dimensions(&state_ref.args);
    let should_request_focus = state_ref.animation_frame == 0;
    let (panel_halign, panel_valign) = panel_alignment(state_ref.args.position());
    panel.set_halign(panel_halign);
    panel.set_valign(panel_valign);
    panel.set_width_request(panel_width);
    panel.set_height_request(panel_height);
    panel.set_size_request(panel_width, panel_height);
    if state_ref.args.layout() == ShellLayout::Vertical {
        panel.add_css_class("vertical");
    } else {
        panel.remove_css_class("vertical");
    }

    let content = gtk::Box::new(gtk::Orientation::Vertical, PICKER_SPACING);
    content.set_halign(gtk::Align::Start);
    content.set_valign(gtk::Align::Start);
    content.set_hexpand(false);
    content.set_vexpand(false);
    let (stage_width, _) = carousel::stage_dimensions(state_ref.args.layout());
    content.set_width_request(stage_width);
    content.set_height_request(content_height);
    content.set_size_request(content_width, content_height);
    if state_ref.args.layout() == ShellLayout::Vertical {
        content.add_css_class("vertical");
    }

    if state_ref.items.is_empty() {
        content.append(&empty::build("No wallpapers found"));
    } else {
        if state_ref.animation_frame == 0 {
            queue_preview_jobs(&mut state_ref);
        }
        let animation_progress = animation_progress(&state_ref);
        if shell_debug_enabled() {
            shell_debug(format!(
                "carousel geometry selected={} previous={:?} frame={} progress={animation_progress:.3} direction={}",
                state_ref.selected,
                state_ref.previous_selected,
                state_ref.animation_frame,
                state_ref.animation_direction,
            ));
            for line in carousel::debug_geometry(
                &state_ref.items,
                state_ref.selected,
                state_ref.original_wallpaper.as_deref(),
                false,
                state_ref.previous_selected,
                animation_progress,
                state_ref.animation_direction,
                state_ref.args.layout(),
            ) {
                shell_debug(line);
            }
        }
        let carousel_widget = carousel::build(
            &state_ref.items,
            state_ref.selected,
            state_ref.original_wallpaper.as_deref(),
            false,
            state_ref.previous_selected,
            animation_progress,
            state_ref.animation_direction,
            state_ref.args.layout(),
        );
        content.append(&carousel_widget);
    }

    let label = gtk::Label::new(state_ref.status.as_deref());
    label.add_css_class("status");
    label.set_halign(gtk::Align::Center);
    label.set_wrap(true);
    label.set_width_request(status_label_width(state_ref.args.layout()));
    label.set_height_request(STATUS_HEIGHT);
    label.set_size_request(status_label_width(state_ref.args.layout()), STATUS_HEIGHT);
    content.append(&label);

    let (content_x, content_y) = content_position(
        state_ref.args.position(),
        state_ref.args.layout(),
        (panel_width, panel_height),
        (content_width, content_height),
    );
    panel.put(&content, content_x, content_y);

    install_focus_on_pointer_motion(&panel, window);
    install_pointer_controls_on(&panel, window, state.clone());
    drop(state_ref);
    window.set_child(Some(&panel));
    if should_request_focus {
        request_widget_focus(&panel);
    }
    log_render_allocations(window, &panel, &content);
}

fn log_render_allocations(window: &gtk::ApplicationWindow, panel: &gtk::Fixed, content: &gtk::Box) {
    if !shell_debug_enabled() {
        return;
    }

    shell_debug(format!(
        "alloc immediate window={}x{} panel={}x{} content={}x{}",
        window.width(),
        window.height(),
        panel.width(),
        panel.height(),
        content.width(),
        content.height()
    ));

    let window = window.clone();
    let panel = panel.clone();
    let content = content.clone();
    glib::idle_add_local_once(move || {
        shell_debug(format!(
            "alloc idle window={}x{} panel={}x{} content={}x{}",
            window.width(),
            window.height(),
            panel.width(),
            panel.height(),
            content.width(),
            content.height()
        ));
    });
}

fn status_label_width(layout: ShellLayout) -> i32 {
    match layout {
        ShellLayout::Horizontal => 520,
        ShellLayout::Vertical => carousel::VERTICAL_STAGE_WIDTH,
    }
}

fn shell_content_dimensions(args: &ShellArgs) -> (i32, i32) {
    let (stage_width, stage_height) = carousel::stage_dimensions(args.layout());
    (stage_width, stage_height + PICKER_SPACING + STATUS_HEIGHT)
}

fn panel_alignment(position: ShellPosition) -> (gtk::Align, gtk::Align) {
    match position {
        ShellPosition::Top => (gtk::Align::Center, gtk::Align::Start),
        ShellPosition::Bottom => (gtk::Align::Center, gtk::Align::End),
        ShellPosition::Center => (gtk::Align::Center, gtk::Align::Center),
        ShellPosition::Left => (gtk::Align::Start, gtk::Align::Center),
        ShellPosition::Right => (gtk::Align::End, gtk::Align::Center),
    }
}

fn content_position(
    position: ShellPosition,
    layout: ShellLayout,
    panel: (i32, i32),
    content: (i32, i32),
) -> (f64, f64) {
    let available_x = (panel.0 - content.0).max(0);
    let available_y = (panel.1 - content.1).max(0);
    let x = match position {
        ShellPosition::Left => 0,
        ShellPosition::Right => available_x,
        ShellPosition::Top | ShellPosition::Bottom | ShellPosition::Center => available_x / 2,
    };
    let y = match layout {
        ShellLayout::Horizontal => available_y / 2,
        ShellLayout::Vertical => 0,
    };
    (f64::from(x), f64::from(y))
}

#[cfg(test)]
fn picker_position(surface: (i32, i32), panel: (i32, i32), position: ShellPosition) -> (i32, i32) {
    let max_x = (surface.0 - panel.0).max(0);
    let max_y = (surface.1 - panel.1).max(0);
    let center_x = max_x / 2;
    let center_y = max_y / 2;

    match position {
        ShellPosition::Top => (center_x, 0),
        ShellPosition::Bottom => (center_x, max_y),
        ShellPosition::Center => (center_x, center_y),
        ShellPosition::Left => (0, center_y),
        ShellPosition::Right => (max_x, center_y),
    }
}

fn request_window_focus(window: &gtk::ApplicationWindow) {
    shell_debug("request window focus scheduled");
    let window_for_idle = window.clone();
    glib::idle_add_local_once(move || {
        shell_debug("request window focus idle");
        window_for_idle.present();
        window_for_idle.grab_focus();
    });

    for delay_ms in [50, 150, 300] {
        let window_for_timeout = window.clone();
        glib::timeout_add_local_once(Duration::from_millis(delay_ms), move || {
            shell_debug(format!("request window focus timeout {delay_ms}ms"));
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

fn request_window_focus_now(window: &gtk::ApplicationWindow) {
    shell_debug("request window focus now");
    if !window.has_focus() {
        window.grab_focus();
    }
}

fn install_focus_on_pointer_motion<W>(target: &W, window: &gtk::ApplicationWindow)
where
    W: IsA<gtk::Widget>,
{
    let motion = gtk::EventControllerMotion::new();
    motion.set_propagation_phase(gtk::PropagationPhase::Capture);
    let last_focus = Rc::new(RefCell::new(Instant::now() - Duration::from_secs(1)));

    let window_for_enter = window.clone();
    let last_focus_for_enter = last_focus.clone();
    motion.connect_enter(move |_, _, _| {
        request_window_focus_throttled(&window_for_enter, &last_focus_for_enter);
    });

    let window_for_motion = window.clone();
    let last_focus_for_motion = last_focus;
    motion.connect_motion(move |_, _, _| {
        request_window_focus_throttled(&window_for_motion, &last_focus_for_motion);
    });

    target.add_controller(motion);
}

fn request_window_focus_throttled(
    window: &gtk::ApplicationWindow,
    last_focus: &Rc<RefCell<Instant>>,
) {
    let now = Instant::now();
    let mut last_focus = last_focus.borrow_mut();
    if now.duration_since(*last_focus) < Duration::from_millis(150) {
        return;
    }
    *last_focus = now;
    request_window_focus_now(window);
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
        shell_debug(format!(
            "queue thumbnail kind={:?} output={}",
            job.kind,
            job.output.display()
        ));
        thread::spawn(move || {
            let started = Instant::now();
            let result = generate(&job).map_err(|error| error.to_string());
            if shell_debug_enabled() {
                eprintln!(
                    "[jobowalls-shell {:?}] thumbnail done kind={:?} output={} elapsed_ms={} ok={}",
                    Instant::now(),
                    job.kind,
                    job.output.display(),
                    started.elapsed().as_millis(),
                    result.is_ok()
                );
            }
            let _ = tx.send(PreviewCompletion {
                source: job.source,
                output: job.output,
                result,
            });
        });
    }
}

fn spawn_desktop_preview_worker(
    request_rx: Receiver<DesktopPreviewRequest>,
    result_tx: Sender<DesktopPreviewResult>,
) {
    thread::spawn(move || {
        let mut live_preview = None;
        let mut static_preview_pid = None;
        'worker: while let Ok(request) = request_rx.recv() {
            let mut request = match request {
                DesktopPreviewRequest::Preview(request) => *request,
                DesktopPreviewRequest::Stop(done_tx) => {
                    stop_live_preview(&mut live_preview);
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
                        stop_live_preview(&mut live_preview);
                        stop_static_preview(&mut static_preview_pid);
                        let _ = done_tx.send(());
                        continue 'worker;
                    }
                    Err(RecvTimeoutError::Timeout) => break,
                    Err(RecvTimeoutError::Disconnected) => break 'worker,
                }
            }

            let result = if request.is_live {
                shell_debug(format!(
                    "desktop preview worker apply live gen={} path={} monitor={}",
                    request.generation,
                    request.path.display(),
                    request.monitor
                ));
                apply_fast_live_preview(&request, &mut live_preview, &mut static_preview_pid)
            } else {
                shell_debug(format!(
                    "desktop preview worker apply static gen={} path={} monitor={}",
                    request.generation,
                    request.path.display(),
                    request.monitor
                ));
                apply_static_desktop_preview(&request, &mut live_preview, &mut static_preview_pid)
            };
            let _ = result_tx.send(DesktopPreviewResult {
                path: request.path,
                generation: request.generation,
                result,
            });
        }
        stop_live_preview(&mut live_preview);
        stop_static_preview(&mut static_preview_pid);
    });
}

fn apply_static_desktop_preview(
    request: &DesktopPreviewJob,
    live_preview: &mut Option<LivePreviewProcess>,
    static_preview_pid: &mut Option<u32>,
) -> std::result::Result<(), String> {
    let plan = SetPlan {
        wallpaper: request.path.clone(),
        media_kind: MediaKind::Static,
        backend: Backend::Swaybg,
        monitor: request.monitor.clone(),
    };
    let command = swaybg::start_command(&plan);
    shell_debug(format!("static preview spawn command={command}"));
    match command.spawn_detached() {
        Ok(pid) => {
            shell_debug(format!("static preview spawned pid={pid}"));
            thread::sleep(Duration::from_millis(STATIC_PREVIEW_SETTLE_MS));
            if !pid_is_running(pid) {
                return Err(format!(
                    "desktop preview failed for {}: swaybg exited before settling",
                    request.path.display()
                ));
            }
            retire_live_preview(live_preview);
            retire_static_preview(static_preview_pid);
            retire_blocking_live_for_desktop_preview(&request.monitor);
            retire_blocking_swaybg_for_live_preview(&request.monitor);
            *static_preview_pid = Some(pid);
            shell_debug(format!("static preview ready pid={pid}"));
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
    live_preview: &mut Option<LivePreviewProcess>,
    static_preview_pid: &mut Option<u32>,
) -> std::result::Result<(), String> {
    if let Some(preview) = live_preview.as_ref() {
        if live_preview_is_usable(preview) {
            match reuse_live_preview(preview, request) {
                Ok(()) => {
                    retire_static_preview(static_preview_pid);
                    retire_blocking_live_for_desktop_preview(&request.monitor);
                    retire_blocking_swaybg_for_live_preview(&request.monitor);
                    shell_debug(format!("live preview reused pid={}", preview.pid));
                    return Ok(());
                }
                Err(error) => {
                    shell_debug(format!(
                        "live preview reuse failed pid={} error={error}",
                        preview.pid
                    ));
                    stop_live_preview(live_preview);
                }
            }
        } else {
            stop_live_preview(live_preview);
        }
    }

    let plan = SetPlan {
        wallpaper: request.path.clone(),
        media_kind: MediaKind::Live,
        backend: Backend::Mpvpaper,
        monitor: request.monitor.clone(),
    };
    let ipc_socket = mpvpaper_preview_ipc_socket(&plan);
    remove_stale_preview_socket(&ipc_socket)?;
    let command = mpvpaper::start_command_with_ipc(&plan, &request.config.mpvpaper, &ipc_socket);
    shell_debug(format!("live preview spawn command={command}"));
    match command.spawn_detached() {
        Ok(pid) => {
            shell_debug(format!("live preview spawned pid={pid}"));
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
            retire_live_preview(live_preview);
            retire_static_preview(static_preview_pid);
            retire_blocking_live_for_desktop_preview(&request.monitor);
            retire_blocking_swaybg_for_live_preview(&request.monitor);
            *live_preview = Some(LivePreviewProcess { pid, ipc_socket });
            shell_debug(format!("live preview ready pid={pid}"));
            Ok(())
        }
        Err(error) => Err(format!(
            "desktop preview failed for {}: {error}",
            request.path.display()
        )),
    }
}

fn live_preview_is_usable(preview: &LivePreviewProcess) -> bool {
    pid_is_running(preview.pid) && preview.ipc_socket.exists()
}

fn reuse_live_preview(
    preview: &LivePreviewProcess,
    request: &DesktopPreviewJob,
) -> std::result::Result<(), String> {
    let path = request.path.to_string_lossy().to_string();
    shell_debug(format!(
        "live preview reuse pid={} path={}",
        preview.pid,
        request.path.display()
    ));
    mpv_ipc_request(&preview.ipc_socket, json!(["loadfile", path]))?;
    wait_for_preview_mpvpaper_path(
        preview.pid,
        &preview.ipc_socket,
        &request.path,
        request.config.mpvpaper.readiness_timeout_ms,
    )
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

fn wait_for_preview_mpvpaper_path(
    pid: u32,
    ipc_socket: &Path,
    path: &Path,
    timeout_ms: u64,
) -> std::result::Result<(), String> {
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    let expected_path = path.to_string_lossy();

    loop {
        if !pid_is_running(pid) {
            return Err(format!(
                "mpvpaper pid {pid} exited before loading {}",
                path.display()
            ));
        }

        let current_path = query_preview_mpv_string_property(ipc_socket, "path");
        let has_video = query_preview_mpv_video_output(ipc_socket);
        match (current_path, has_video) {
            (Ok(Some(current_path)), Ok(true)) if current_path == expected_path => return Ok(()),
            (Err(error), _) | (_, Err(error)) if Instant::now() >= deadline => {
                return Err(format!(
                    "failed to query mpv IPC {}: {error}",
                    ipc_socket.display()
                ));
            }
            _ => {}
        }

        if Instant::now() >= deadline {
            return Err(format!(
                "timed out waiting for mpvpaper pid {pid} to load {}",
                path.display()
            ));
        }

        thread::sleep(Duration::from_millis(50));
    }
}

fn query_preview_mpv_video_output(ipc_socket: &Path) -> std::result::Result<bool, String> {
    let response = mpv_ipc_request(ipc_socket, json!(["get_property", "video-out-params"]))?;
    if response.get("error").and_then(|error| error.as_str()) != Some("success") {
        return Ok(false);
    }

    Ok(!response.get("data").is_none_or(|data| data.is_null()))
}

fn query_preview_mpv_string_property(
    ipc_socket: &Path,
    property: &str,
) -> std::result::Result<Option<String>, String> {
    let response = mpv_ipc_request(ipc_socket, json!(["get_property", property]))?;
    if response.get("error").and_then(|error| error.as_str()) != Some("success") {
        return Ok(None);
    }

    Ok(response
        .get("data")
        .and_then(|data| data.as_str())
        .map(str::to_string))
}

fn mpv_ipc_request(
    ipc_socket: &Path,
    command: serde_json::Value,
) -> std::result::Result<serde_json::Value, String> {
    let mut stream = UnixStream::connect(ipc_socket).map_err(|error| error.to_string())?;
    stream
        .set_read_timeout(Some(Duration::from_millis(200)))
        .map_err(|error| error.to_string())?;
    stream
        .set_write_timeout(Some(Duration::from_millis(200)))
        .map_err(|error| error.to_string())?;
    let request = json!({ "command": command, "request_id": 1 });
    writeln!(stream, "{request}").map_err(|error| error.to_string())?;

    let mut response = String::new();
    BufReader::new(stream)
        .read_line(&mut response)
        .map_err(|error| error.to_string())?;

    serde_json::from_str(&response).map_err(|error| error.to_string())
}

fn retire_blocking_swaybg_for_live_preview(monitor: &str) {
    for pid in blocking_swaybg_pids(&runtime_state_path(), monitor) {
        retire_preview_blocker(pid, None);
    }
}

fn retire_blocking_live_for_desktop_preview(monitor: &str) {
    for pid in blocking_live_pids(&runtime_state_path(), monitor) {
        retire_preview_blocker(pid, None);
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

fn stop_live_preview(live_preview: &mut Option<LivePreviewProcess>) {
    if let Some(preview) = live_preview.take() {
        terminate_preview_blocker(preview.pid);
        cleanup_preview_socket(&preview.ipc_socket);
    }
}

fn retire_live_preview(live_preview: &mut Option<LivePreviewProcess>) {
    if let Some(preview) = live_preview.take() {
        retire_preview_blocker(preview.pid, Some(preview.ipc_socket));
    }
}

fn stop_static_preview(static_preview_pid: &mut Option<u32>) {
    if let Some(pid) = static_preview_pid.take() {
        terminate_preview_blocker(pid);
    }
}

fn retire_static_preview(static_preview_pid: &mut Option<u32>) {
    if let Some(pid) = static_preview_pid.take() {
        retire_preview_blocker(pid, None);
    }
}

fn retire_preview_blocker(pid: u32, ipc_socket: Option<PathBuf>) {
    shell_debug(format!(
        "retire preview pid={pid} delay_ms={RETIRED_PREVIEW_CLEANUP_DELAY_MS}"
    ));
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(RETIRED_PREVIEW_CLEANUP_DELAY_MS));
        terminate_preview_blocker(pid);
        if let Some(ipc_socket) = ipc_socket {
            cleanup_preview_socket(&ipc_socket);
        }
    });
}

fn cleanup_preview_socket(path: &Path) {
    match fs::remove_file(path) {
        Ok(()) => shell_debug(format!("removed preview ipc socket={}", path.display())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => shell_debug(format!(
            "failed to remove preview ipc socket={} error={error}",
            path.display()
        )),
    }
}

fn terminate_preview_blocker(pid: u32) {
    shell_debug(format!("terminate preview pid={pid}"));
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

    let remaining_frames = state.animation_frame.saturating_sub(1);
    let progress = 1.0 - (f64::from(remaining_frames) / f64::from(NAV_ANIMATION_FRAMES));
    progress * progress * (3.0 - 2.0 * progress)
}

fn install_preview_poll(
    window: &gtk::ApplicationWindow,
    state: Rc<RefCell<AppState>>,
    preview_rx: mpsc::Receiver<PreviewCompletion>,
) {
    let window = window.clone();
    glib::timeout_add_local(Duration::from_millis(150), move || {
        let mut should_render = false;
        while let Ok(completion) = preview_rx.try_recv() {
            let mut state_ref = state.borrow_mut();
            if completion.result.is_err() {
                state_ref.queued_previews.remove(&completion.output);
            }

            let selected_source = state_ref
                .items
                .get(state_ref.selected)
                .map(|item| item.path.as_path());
            if completion.result.is_ok()
                && selected_source == Some(completion.source.as_path())
                && state_ref.animation_frame == 0
            {
                should_render = true;
            }

            shell_debug(format!(
                "thumbnail completion source={} output={} ok={} render={should_render}",
                completion.source.display(),
                completion.output.display(),
                completion.result.is_ok()
            ));
        }

        if should_render {
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
        let mut should_render = false;
        while let Ok(result) = desktop_result_rx.try_recv() {
            let mut state = state.borrow_mut();
            if result.generation != state.desktop_preview_generation {
                shell_debug(format!(
                    "desktop preview stale result gen={} current={}",
                    result.generation, state.desktop_preview_generation
                ));
                continue;
            }

            match result.result {
                Ok(()) => {
                    shell_debug(format!(
                        "desktop preview success gen={} path={}",
                        result.generation,
                        result.path.display()
                    ));
                    state.current_preview_wallpaper = Some(result.path.clone());
                    if state.pending_desktop_preview_wallpaper.as_ref() == Some(&result.path) {
                        state.pending_desktop_preview_wallpaper = None;
                    }
                }
                Err(error) => {
                    shell_debug(format!(
                        "desktop preview error gen={} error={error}",
                        result.generation
                    ));
                    state.status = Some(error);
                    if state.pending_desktop_preview_wallpaper.as_ref() == Some(&result.path) {
                        state.pending_desktop_preview_wallpaper = None;
                    }
                    should_render = true;
                }
            }
        }

        if should_render {
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
    let state_for_press = state.clone();
    keys.connect_key_pressed(move |_, key, _, modifier| match key {
        gdk::Key::Control_L | gdk::Key::Control_R => {
            state_for_press.borrow_mut().control_modifier_down = true;
            glib::Propagation::Proceed
        }
        gdk::Key::Escape => {
            restore_original_and_close(&state_for_press, &window_for_keys);
            glib::Propagation::Stop
        }
        gdk::Key::Left | gdk::Key::Up | gdk::Key::h | gdk::Key::H | gdk::Key::k | gdk::Key::K => {
            move_selection(&state_for_press, -1);
            render(&window_for_keys, state_for_press.clone());
            schedule_desktop_preview_after_navigation(state_for_press.clone());
            glib::Propagation::Stop
        }
        gdk::Key::Right
        | gdk::Key::Down
        | gdk::Key::l
        | gdk::Key::L
        | gdk::Key::j
        | gdk::Key::J => {
            move_selection(&state_for_press, 1);
            render(&window_for_keys, state_for_press.clone());
            schedule_desktop_preview_after_navigation(state_for_press.clone());
            glib::Propagation::Stop
        }
        gdk::Key::Return | gdk::Key::KP_Enter => {
            apply_selected(&state_for_press, &window_for_keys);
            glib::Propagation::Stop
        }
        gdk::Key::p | gdk::Key::P
            if modifier.contains(gdk::ModifierType::CONTROL_MASK)
                || state_for_press.borrow().control_modifier_down =>
        {
            cycle_picker_placement(&state_for_press, &window_for_keys);
            glib::Propagation::Stop
        }
        gdk::Key::r | gdk::Key::R => {
            rescan(&state_for_press);
            render(&window_for_keys, state_for_press.clone());
            glib::Propagation::Stop
        }
        gdk::Key::s | gdk::Key::S => {
            shuffle_selection(&state_for_press);
            render(&window_for_keys, state_for_press.clone());
            schedule_desktop_preview_after_navigation(state_for_press.clone());
            glib::Propagation::Stop
        }
        _ => glib::Propagation::Proceed,
    });
    keys.connect_key_released({
        let state = state.clone();
        move |_, key, _, _| {
            if matches!(key, gdk::Key::Control_L | gdk::Key::Control_R) {
                state.borrow_mut().control_modifier_down = false;
            }
        }
    });
    target.add_controller(keys);
}

fn cycle_picker_placement(state: &Rc<RefCell<AppState>>, window: &gtk::ApplicationWindow) {
    let args = {
        let mut state = state.borrow_mut();
        let (position, layout) = next_picker_placement(state.args.position());
        state.args.position = Some(position);
        state.args.layout = Some(layout);
        state.animation_frame = 0;
        state.previous_selected = None;
        suppress_dismiss_for(&mut state, Duration::from_millis(500));
        state.args.clone()
    };
    let target_monitor = shell_target_monitor(&args);
    configure_shell_panel(window, &args, target_monitor.as_ref());
    render(window, state.clone());
    window.present();
    request_window_focus(window);
}

fn next_picker_placement(position: ShellPosition) -> (ShellPosition, ShellLayout) {
    match position {
        ShellPosition::Bottom => (ShellPosition::Left, ShellLayout::Vertical),
        ShellPosition::Left => (ShellPosition::Top, ShellLayout::Horizontal),
        ShellPosition::Top => (ShellPosition::Right, ShellLayout::Vertical),
        ShellPosition::Right => (ShellPosition::Bottom, ShellLayout::Horizontal),
        ShellPosition::Center => (ShellPosition::Bottom, ShellLayout::Horizontal),
    }
}

fn suppress_dismiss_for(state: &mut AppState, duration: Duration) {
    state.dismiss_suppressed_until = Some(Instant::now() + duration);
}

fn dismiss_is_suppressed(state: &mut AppState) -> bool {
    match state.dismiss_suppressed_until {
        Some(until) if Instant::now() < until => true,
        Some(_) => {
            state.dismiss_suppressed_until = None;
            false
        }
        None => false,
    }
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
        } else {
            return glib::Propagation::Proceed;
        }
        render(&window_for_scroll, state_for_scroll.clone());
        schedule_desktop_preview_after_navigation(state_for_scroll.clone());
        glib::Propagation::Stop
    });
    target.add_controller(scroll);

    let click = gtk::GestureClick::new();
    click.set_propagation_phase(gtk::PropagationPhase::Capture);
    let window_for_click = window.clone();
    let target_for_click = target.clone().upcast::<gtk::Widget>();
    click.connect_pressed(move |_, presses, x, y| {
        if presses >= 2 {
            apply_selected(&state, &window_for_click);
            return;
        }

        let (position, extent) = if state.borrow().args.layout() == ShellLayout::Vertical {
            (y, f64::from(target_for_click.height()).max(1.0))
        } else {
            (x, f64::from(target_for_click.width()).max(1.0))
        };

        if position < extent / 3.0 {
            move_selection(&state, -1);
            render(&window_for_click, state.clone());
            schedule_desktop_preview_after_navigation(state.clone());
        } else if position > extent * 2.0 / 3.0 {
            move_selection(&state, 1);
            render(&window_for_click, state.clone());
            schedule_desktop_preview_after_navigation(state.clone());
        }
    });
    target.add_controller(click);
}

fn install_cancel_on_click<W>(
    target: &W,
    window: &gtk::ApplicationWindow,
    state: Rc<RefCell<AppState>>,
) where
    W: IsA<gtk::Widget>,
{
    let click = gtk::GestureClick::new();
    click.set_propagation_phase(gtk::PropagationPhase::Capture);
    let window_for_click = window.clone();
    click.connect_pressed(move |_, _, _, _| {
        if dismiss_is_suppressed(&mut state.borrow_mut()) {
            return;
        }
        restore_original_and_close(&state, &window_for_click);
    });
    target.add_controller(click);
}

fn install_close_cleanup(
    window: &gtk::ApplicationWindow,
    state: Rc<RefCell<AppState>>,
    click_catcher: Option<gtk::ApplicationWindow>,
) {
    window.connect_close_request(move |_| {
        if let Some(click_catcher) = &click_catcher {
            click_catcher.close();
        }
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
    let direction = delta.signum();
    let now = Instant::now();
    if should_snap_repeated_navigation(state.last_navigation, direction, now) {
        state.animation_frame = 0;
        state.animation_direction = direction;
        state.previous_selected = None;
    } else {
        state.animation_frame = NAV_ANIMATION_FRAMES;
        state.animation_direction = direction;
        state.previous_selected = Some(previous);
    }
    state.last_navigation = Some(NavigationInput { direction, at: now });
}

fn should_snap_repeated_navigation(
    last_navigation: Option<NavigationInput>,
    direction: isize,
    now: Instant,
) -> bool {
    let Some(last_navigation) = last_navigation else {
        return false;
    };

    last_navigation.direction == direction
        && now.duration_since(last_navigation.at) <= Duration::from_millis(NAV_REPEAT_SNAP_MS)
}

fn apply_selected(state: &Rc<RefCell<AppState>>, window: &gtk::ApplicationWindow) {
    let (path, monitor) = {
        let state = state.borrow();
        let Some(item) = state.items.get(state.selected) else {
            window.close();
            return;
        };
        (item.path.clone(), state.args.monitor().to_string())
    };

    window.close();
    if let Err(error) = apply_wallpaper(&path, &monitor) {
        eprintln!("apply failed: {error:#}");
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
        if !should_request_desktop_preview(
            state.current_preview_wallpaper.as_deref(),
            state.pending_desktop_preview_wallpaper.as_deref(),
            &path,
        ) {
            return;
        }

        state.desktop_preview_generation += 1;
        let generation = state.desktop_preview_generation;
        state.pending_desktop_preview_wallpaper = Some(path.clone());
        shell_debug(format!(
            "schedule desktop preview gen={generation} live={is_live} debounce_ms={debounce_ms} path={}",
            path.display()
        ));
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

fn should_request_desktop_preview(
    current_preview: Option<&Path>,
    pending_preview: Option<&Path>,
    target: &Path,
) -> bool {
    if pending_preview == Some(target) {
        return false;
    }

    current_preview != Some(target) || pending_preview.is_some()
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

    window.close();
    if let Some(original) = original
        && needs_restore
    {
        if let Err(error) = apply_wallpaper(&original, &monitor) {
            eprintln!("restore failed: {error:#}");
        }
    }
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
    fn ctrl_p_cycle_moves_clockwise_and_switches_layout() {
        assert_eq!(
            next_picker_placement(ShellPosition::Bottom),
            (ShellPosition::Left, ShellLayout::Vertical)
        );
        assert_eq!(
            next_picker_placement(ShellPosition::Left),
            (ShellPosition::Top, ShellLayout::Horizontal)
        );
        assert_eq!(
            next_picker_placement(ShellPosition::Top),
            (ShellPosition::Right, ShellLayout::Vertical)
        );
        assert_eq!(
            next_picker_placement(ShellPosition::Right),
            (ShellPosition::Bottom, ShellLayout::Horizontal)
        );
    }

    #[test]
    fn computes_picker_positions_from_surface_and_panel_size() {
        let surface = (2560, 1440);
        let horizontal_panel = (860, 340);
        let vertical_panel = (340, 886);

        assert_eq!(
            picker_position(surface, horizontal_panel, ShellPosition::Bottom),
            (850, 1100)
        );
        assert_eq!(
            picker_position(surface, horizontal_panel, ShellPosition::Top),
            (850, 0)
        );
        assert_eq!(
            picker_position(surface, vertical_panel, ShellPosition::Left),
            (0, 277)
        );
        assert_eq!(
            picker_position(surface, vertical_panel, ShellPosition::Right),
            (2220, 277)
        );
    }

    #[test]
    fn side_panels_align_to_their_anchored_edge() {
        assert_eq!(
            panel_alignment(ShellPosition::Left),
            (gtk::Align::Start, gtk::Align::Center)
        );
        assert_eq!(
            panel_alignment(ShellPosition::Right),
            (gtk::Align::End, gtk::Align::Center)
        );
    }

    #[test]
    fn side_content_is_positioned_against_anchored_edge_when_surface_is_wide() {
        assert_eq!(
            content_position(
                ShellPosition::Left,
                ShellLayout::Vertical,
                (860, 886),
                (340, 886)
            ),
            (0.0, 0.0)
        );
        assert_eq!(
            content_position(
                ShellPosition::Right,
                ShellLayout::Vertical,
                (860, 886),
                (340, 886)
            ),
            (520.0, 0.0)
        );
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

    #[test]
    fn mpv_ipc_request_sends_loadfile_command() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("mpv.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = String::new();
            BufReader::new(stream.try_clone().unwrap())
                .read_line(&mut request)
                .unwrap();
            let request: serde_json::Value = serde_json::from_str(&request).unwrap();
            assert_eq!(
                request.get("command"),
                Some(&json!(["loadfile", "/wall/live.mp4"]))
            );
            writeln!(stream, r#"{{"request_id":1,"error":"success"}}"#).unwrap();
        });

        assert_eq!(
            mpv_ipc_request(&socket, json!(["loadfile", "/wall/live.mp4"]))
                .unwrap()
                .get("error")
                .and_then(|error| error.as_str()),
            Some("success")
        );
        server.join().unwrap();
    }

    #[test]
    fn query_preview_mpv_string_property_returns_path() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("mpv.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = String::new();
            BufReader::new(stream.try_clone().unwrap())
                .read_line(&mut request)
                .unwrap();
            assert!(request.contains("\"path\""));
            writeln!(
                stream,
                r#"{{"request_id":1,"error":"success","data":"/wall/live.mp4"}}"#
            )
            .unwrap();
        });

        assert_eq!(
            query_preview_mpv_string_property(&socket, "path"),
            Ok(Some("/wall/live.mp4".to_string()))
        );
        server.join().unwrap();
    }

    #[test]
    fn desktop_preview_skips_current_wallpaper_without_pending_preview() {
        assert!(!should_request_desktop_preview(
            Some(Path::new("/wall/a.png")),
            None,
            Path::new("/wall/a.png")
        ));
    }

    #[test]
    fn desktop_preview_replaces_different_pending_preview_even_when_target_is_current() {
        assert!(should_request_desktop_preview(
            Some(Path::new("/wall/a.png")),
            Some(Path::new("/wall/b.png")),
            Path::new("/wall/a.png")
        ));
    }

    #[test]
    fn desktop_preview_skips_duplicate_pending_preview() {
        assert!(!should_request_desktop_preview(
            Some(Path::new("/wall/a.png")),
            Some(Path::new("/wall/b.png")),
            Path::new("/wall/b.png")
        ));
    }

    #[test]
    fn rapid_same_direction_navigation_snaps_instead_of_restarting_animation() {
        let now = Instant::now();
        let last = NavigationInput {
            direction: 1,
            at: now - Duration::from_millis(40),
        };

        assert!(should_snap_repeated_navigation(Some(last), 1, now));
    }

    #[test]
    fn slow_navigation_keeps_full_animation() {
        let now = Instant::now();
        let last = NavigationInput {
            direction: 1,
            at: now - Duration::from_millis(NAV_REPEAT_SNAP_MS + 1),
        };

        assert!(!should_snap_repeated_navigation(Some(last), 1, now));
    }

    #[test]
    fn direction_change_keeps_full_animation() {
        let now = Instant::now();
        let last = NavigationInput {
            direction: 1,
            at: now - Duration::from_millis(40),
        };

        assert!(!should_snap_repeated_navigation(Some(last), -1, now));
    }

    #[test]
    fn repeat_burst_stays_snapped_after_animation_has_been_cleared() {
        let now = Instant::now();
        let last = NavigationInput {
            direction: 1,
            at: now - Duration::from_millis(40),
        };

        assert!(should_snap_repeated_navigation(Some(last), 1, now));
    }
}
