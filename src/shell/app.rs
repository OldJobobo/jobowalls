use crate::{
    shell::{
        apply::apply_wallpaper,
        cli::ShellArgs,
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
use std::{
    cell::RefCell,
    collections::HashSet,
    path::PathBuf,
    rc::Rc,
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const NAV_ANIMATION_FRAMES: u8 = 10;
const DESKTOP_PREVIEW_DEBOUNCE_MS: u64 = 120;

pub fn run() -> Result<()> {
    let args = ShellArgs::parse();
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
struct DesktopPreviewRequest {
    path: PathBuf,
    monitor: String,
    generation: u64,
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
    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title("JoboWalls Shell")
        .default_width(args.width)
        .default_height(args.height)
        .resizable(false)
        .build();
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
    install_pointer_controls(&window, state.clone());
    schedule_desktop_preview(&state);
    window.present();
    Ok(())
}

fn render(window: &gtk::ApplicationWindow, state: Rc<RefCell<AppState>>) {
    let mut state_ref = state.borrow_mut();
    let root = gtk::Box::new(gtk::Orientation::Vertical, 8);
    root.add_css_class("shell-root");
    root.set_halign(gtk::Align::Center);
    root.set_valign(gtk::Align::Center);

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

    if let Some(status) = &state_ref.status {
        let label = gtk::Label::new(Some(status));
        label.add_css_class("status");
        label.set_wrap(true);
        label.set_width_request(520);
        root.append(&label);
    }

    window.set_child(Some(&root));
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
        while let Ok(mut request) = request_rx.recv() {
            while let Ok(next) =
                request_rx.recv_timeout(Duration::from_millis(DESKTOP_PREVIEW_DEBOUNCE_MS))
            {
                request = next;
            }

            let result = apply_wallpaper(&request.path, &request.monitor).map_err(|error| {
                format!(
                    "desktop preview failed for {}: {error}",
                    request.path.display()
                )
            });
            let _ = result_tx.send(DesktopPreviewResult {
                path: request.path,
                generation: request.generation,
                result,
            });
        }
    });
}

fn animation_progress(state: &AppState) -> f64 {
    if state.animation_frame == 0 {
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
                    if state.status.as_deref() == Some("Previewing...") {
                        state.status = None;
                    }
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
    let keys = gtk::EventControllerKey::new();
    let window_for_keys = window.clone();
    keys.connect_key_pressed(move |_, key, _, _| match key {
        gdk::Key::Escape => {
            restore_original_and_close(&state, &window_for_keys);
            glib::Propagation::Stop
        }
        gdk::Key::Left | gdk::Key::h | gdk::Key::H => {
            move_selection(&state, -1);
            render(&window_for_keys, state.clone());
            glib::Propagation::Stop
        }
        gdk::Key::Right | gdk::Key::l | gdk::Key::L => {
            move_selection(&state, 1);
            render(&window_for_keys, state.clone());
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
            glib::Propagation::Stop
        }
        _ => glib::Propagation::Proceed,
    });
    window.add_controller(keys);
}

fn install_pointer_controls(window: &gtk::ApplicationWindow, state: Rc<RefCell<AppState>>) {
    let scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::BOTH_AXES);
    let window_for_scroll = window.clone();
    let state_for_scroll = state.clone();
    scroll.connect_scroll(move |_, dx, dy| {
        if dx < 0.0 || dy < 0.0 {
            move_selection(&state_for_scroll, -1);
        } else if dx > 0.0 || dy > 0.0 {
            move_selection(&state_for_scroll, 1);
        }
        render(&window_for_scroll, state_for_scroll.clone());
        glib::Propagation::Stop
    });
    window.add_controller(scroll);

    let click = gtk::GestureClick::new();
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
        } else if x > width * 2.0 / 3.0 {
            move_selection(&state, 1);
            render(&window_for_click, state.clone());
        }
    });
    window.add_controller(click);
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
    let monitor = state.args.monitor.clone();
    let selected = state.selected;
    state.shell_state.remember(&folder, &monitor, selected);
    let _ = state.shell_state.save(&state.shell_state_path);
    drop(state);
    schedule_desktop_preview(app_state);
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
    let monitor = state.args.monitor.clone();
    state.shell_state.remember(&folder, &monitor, selected);
    let _ = state.shell_state.save(&state.shell_state_path);
    drop(state);
    schedule_desktop_preview(app_state);
}

fn start_navigation_animation(state: &mut AppState, previous: usize, delta: isize) {
    state.animation_frame = NAV_ANIMATION_FRAMES;
    state.animation_direction = if delta < 0 { -1 } else { 1 };
    state.previous_selected = Some(previous);
}

fn apply_selected(state: &Rc<RefCell<AppState>>, window: &gtk::ApplicationWindow) {
    let (path, monitor) = {
        let state = state.borrow();
        let Some(item) = state.items.get(state.selected) else {
            return;
        };
        (item.path.clone(), state.args.monitor.clone())
    };

    {
        let mut state = state.borrow_mut();
        state.status = Some("Applying...".to_string());
    }
    render(window, state.clone());

    match apply_wallpaper(&path, &monitor) {
        Ok(()) => {
            let mut state = state.borrow_mut();
            state.active_wallpaper = Some(path.clone());
            state.current_preview_wallpaper = Some(path);
            state.original_wallpaper = state.current_preview_wallpaper.clone();
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
        if state.current_preview_wallpaper.as_ref() == Some(&path) {
            return;
        }

        state.desktop_preview_generation += 1;
        state.status = Some("Previewing...".to_string());
        DesktopPreviewRequest {
            path,
            monitor: state.args.monitor.clone(),
            generation: state.desktop_preview_generation,
        }
    };

    let _ = state.borrow().desktop_request_tx.send(request);
}

fn restore_original_and_close(state: &Rc<RefCell<AppState>>, window: &gtk::ApplicationWindow) {
    let (original, monitor, needs_restore) = {
        let state = state.borrow();
        let original = state.original_wallpaper.clone();
        let needs_restore = original.is_some() && state.current_preview_wallpaper != original;
        (original, state.args.monitor.clone(), needs_restore)
    };

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

fn initial_selection(items: &[WallpaperItem], folder: &PathBuf, shell_state: &ShellState) -> usize {
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
