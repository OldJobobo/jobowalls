use crate::{
    backends::{
        awww, hyprpaper,
        model::{Backend, BackendOverride, WallpaperBackend},
        mpvpaper, swaybg,
    },
    collection::{
        scan_collection, select_next_persistent, select_previous_persistent,
        select_shuffle_persistent,
    },
    command::{CommandSpec, pid_is_running, program_available, run_all, signal_pid, terminate_pid},
    config::{BackendPreference, Config, StaticBackendPreference},
    monitors,
    orchestrator::{SetPlan, plan_set},
    state::State,
};
use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use serde::Serialize;
use serde_json::json;
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

#[derive(Debug, Parser)]
#[command(name = "jobowalls", version, about)]
struct Cli {
    #[arg(long, value_name = "PATH")]
    config: Option<PathBuf>,

    #[arg(long, value_name = "PATH")]
    state: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Set {
        path: PathBuf,

        #[arg(long)]
        monitor: Option<String>,

        #[arg(long, value_enum, default_value_t = BackendArg::Auto)]
        backend: BackendArg,

        #[arg(long)]
        dry_run: bool,

        #[arg(long)]
        json: bool,
    },
    Status {
        #[arg(long)]
        json: bool,
    },
    #[command(alias = "stop-live")]
    Stop,
    Restore {
        #[arg(long)]
        dry_run: bool,
    },
    Next(CollectionArgs),
    Previous(CollectionArgs),
    Shuffle(CollectionArgs),
    Daemon {
        #[arg(long)]
        once: bool,
    },
    ListMonitors,
    Doctor,
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
}

#[derive(Debug, Subcommand)]
enum ConfigCommand {
    Init {
        #[arg(long)]
        force: bool,
    },
    PrintDefault,
}

#[derive(Debug, clap::Args)]
struct CollectionArgs {
    collection: PathBuf,

    #[arg(long)]
    monitor: Option<String>,

    #[arg(long, value_enum, default_value_t = BackendArg::Auto)]
    backend: BackendArg,

    #[arg(long)]
    dry_run: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum BackendArg {
    Auto,
    Hyprpaper,
    Mpvpaper,
    Awww,
    Swaybg,
}

impl From<BackendArg> for BackendOverride {
    fn from(value: BackendArg) -> Self {
        match value {
            BackendArg::Auto => BackendOverride::Auto,
            BackendArg::Hyprpaper => BackendOverride::Backend(Backend::Hyprpaper),
            BackendArg::Mpvpaper => BackendOverride::Backend(Backend::Mpvpaper),
            BackendArg::Awww => BackendOverride::Backend(Backend::Awww),
            BackendArg::Swaybg => BackendOverride::Backend(Backend::Swaybg),
        }
    }
}

pub fn run() -> Result<()> {
    let cli = Cli::parse();
    let paths = RuntimePaths::resolve(cli.config, cli.state)?;
    let config = Config::load(&paths.config)?;

    match cli.command {
        Command::Set {
            path,
            monitor,
            backend,
            dry_run,
            json,
        } => {
            if !path.exists() {
                bail!("wallpaper path does not exist: {}", path.display());
            }

            let path = fs::canonicalize(&path)
                .with_context(|| format!("failed to resolve wallpaper path {}", path.display()))?;
            let mut plan = plan_set(&config, &path, monitor, backend.into())?;
            apply_runtime_auto_backend(&mut plan, &config, backend);

            if dry_run {
                if json {
                    print_set_plan_json(&plan)?;
                } else {
                    print_set_plan(&plan, &config)?;
                }
                return Ok(());
            }

            execute_set_plan(&plan, &config, &paths.state)?;
        }
        Command::Status { json } => match State::load(&paths.state)? {
            Some(state) => {
                if json {
                    print_status_json(Some(&state))?;
                } else {
                    print_state(&state);
                }
            }
            None => {
                if json {
                    print_status_json(None)?;
                } else {
                    println!("no jobowalls state found at {}", paths.state.display());
                }
            }
        },
        Command::Stop => {
            let stopped = stop_owned_live(&paths.state)?;
            record_last_command_if_state_exists(&paths.state, "stop")?;
            if stopped == 0 {
                println!("no owned live wallpaper processes found");
            } else {
                println!("stopped {stopped} owned live wallpaper process(es)");
            }
        }
        Command::Restore { dry_run } => {
            let Some(state) = State::load(&paths.state)? else {
                bail!("no jobowalls state found at {}", paths.state.display());
            };
            if dry_run {
                print_restore_plan(&state, &config)?;
                return Ok(());
            }
            execute_restore(&state, &config, &paths.state)?;
        }
        Command::Next(args) => {
            execute_collection_step(CollectionStep::Next, args, &config, &paths.state)?;
        }
        Command::Previous(args) => {
            execute_collection_step(CollectionStep::Previous, args, &config, &paths.state)?;
        }
        Command::Shuffle(args) => {
            execute_collection_step(CollectionStep::Shuffle, args, &config, &paths.state)?;
        }
        Command::Daemon { once } => {
            run_daemon(&config, &paths.state, once)?;
        }
        Command::ListMonitors => {
            print!("{}", monitors::list()?);
        }
        Command::Doctor => print_doctor(&paths, &config),
        Command::Config { command } => match command {
            ConfigCommand::Init { force } => {
                Config::default().save(&paths.config, force)?;
                println!("wrote config {}", paths.config.display());
            }
            ConfigCommand::PrintDefault => {
                print!("{}", Config::default().to_toml_string()?);
            }
        },
    }

    Ok(())
}

#[derive(Debug, Clone, Copy)]
enum CollectionStep {
    Next,
    Previous,
    Shuffle,
}

fn execute_collection_step(
    step: CollectionStep,
    args: CollectionArgs,
    config: &Config,
    state_path: &std::path::Path,
) -> Result<()> {
    let collection = fs::canonicalize(&args.collection)
        .with_context(|| format!("failed to resolve collection {}", args.collection.display()))?;
    let wallpapers = scan_collection(&collection)?;
    let state = State::load(state_path)?;
    let collection_key = collection.display().to_string();
    let collection_state = state
        .as_ref()
        .and_then(|state| state.collections.get(&collection_key));
    let current = state
        .as_ref()
        .map(|state| std::path::Path::new(&state.wallpaper));
    let (index, wallpaper) = match step {
        CollectionStep::Next => select_next_persistent(&wallpapers, collection_state, current),
        CollectionStep::Previous => {
            select_previous_persistent(&wallpapers, collection_state, current)
        }
        CollectionStep::Shuffle => {
            select_shuffle_persistent(&wallpapers, collection_state, current, shuffle_seed())
        }
    };
    let monitor = args
        .monitor
        .or_else(|| state.as_ref().map(default_monitor_from_state));
    let mut plan = plan_set(config, &wallpaper, monitor, args.backend.into())?;
    apply_runtime_auto_backend(&mut plan, config, args.backend);

    println!("collection: {}", collection.display());
    println!("selected: {}", plan.wallpaper.display());

    if args.dry_run {
        print_set_plan(&plan, config)?;
        return Ok(());
    }

    execute_set_plan(&plan, config, state_path)?;
    record_collection_progress(
        state_path,
        &collection,
        index,
        matches!(step, CollectionStep::Shuffle),
    )?;
    record_last_command_if_state_exists(
        state_path,
        last_collection_command(step, &collection, &plan),
    )
}

fn default_monitor_from_state(state: &State) -> String {
    match state.monitors.len() {
        0 => "all".to_string(),
        1 => state
            .monitors
            .keys()
            .next()
            .cloned()
            .unwrap_or_else(|| "all".to_string()),
        _ => "all".to_string(),
    }
}

fn shuffle_seed() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as u64)
        .unwrap_or(0)
}

fn print_state(state: &State) {
    println!("backend: {}", state.active_backend);
    println!("mode: {:?}", state.mode);
    println!("wallpaper: {}", state.wallpaper);
    println!("updated_at: {}", state.updated_at);
    if let Some(command) = &state.last_command {
        println!("last_command: {command}");
    }
    if state.monitors.is_empty() {
        println!("monitors: none recorded");
    } else {
        println!("monitors:");
        for (name, monitor) in &state.monitors {
            let pid = monitor
                .pid
                .map(|pid| pid.to_string())
                .unwrap_or_else(|| "none".to_string());
            println!(
                "  {name}: backend={}, pid={}, pid_status={}, wallpaper={}",
                monitor.backend,
                pid,
                pid_status(monitor.pid),
                monitor.wallpaper
            );
        }
    }

    if !state.collections.is_empty() {
        println!("collections:");
        for (path, collection) in &state.collections {
            let index = collection
                .last_index
                .map(|index| index.to_string())
                .unwrap_or_else(|| "none".to_string());
            let wallpaper = collection.last_wallpaper.as_deref().unwrap_or("none");
            println!(
                "  {path}: last_index={}, last_wallpaper={}, shuffle_seen={}",
                index,
                wallpaper,
                collection.shuffle_history.len()
            );
        }
    }
}

#[derive(Debug, Serialize)]
struct StatusJson<'a> {
    state_exists: bool,
    #[serde(flatten)]
    state: Option<&'a State>,
}

fn print_status_json(state: Option<&State>) -> Result<()> {
    let payload = StatusJson {
        state_exists: state.is_some(),
        state,
    };
    println!("{}", serde_json::to_string_pretty(&payload)?);
    Ok(())
}

fn print_doctor(paths: &RuntimePaths, config: &Config) {
    println!("config: {}", paths.config.display());
    println!("config exists: {}", paths.config.exists());
    println!("state: {}", paths.state.display());
    println!("state exists: {}", paths.state.exists());
    println!("hyprland session detected: {}", hyprland_session_detected());
    println!(
        "default static backend: {:?}",
        config.general.static_backend
    );
    println!("default live backend: {}", config.general.live_backend);
    println!("hyprctl available: {}", command_available("hyprctl"));
    println!(
        "{} available: {}",
        backend_adapter(Backend::Hyprpaper).name(),
        backend_adapter(Backend::Hyprpaper).is_available()
    );
    println!(
        "hyprpaper daemon reachable: {}",
        hyprpaper_daemon_reachable()
    );
    println!(
        "{} available: {}",
        backend_adapter(Backend::Mpvpaper).name(),
        backend_adapter(Backend::Mpvpaper).is_available()
    );
    println!(
        "{} available: {}",
        backend_adapter(Backend::Awww).name(),
        backend_adapter(Backend::Awww).is_available()
    );
    println!(
        "{} available: {}",
        backend_adapter(Backend::Swaybg).name(),
        backend_adapter(Backend::Swaybg).is_available()
    );
    println!(
        "awww-daemon available: {}",
        command_available("awww-daemon")
    );
    println!("awww daemon reachable: {}", awww_daemon_reachable());
    println!(
        "static auto backend: {}",
        detected_static_auto_backend(config)
    );

    match monitors::names() {
        Ok(monitors) => {
            println!("active monitors: {}", monitors.len());
            for monitor in monitors {
                println!("  {monitor}");
            }
        }
        Err(error) => println!("active monitors: unavailable ({error})"),
    }

    match State::load(&paths.state) {
        Ok(Some(state)) => print_doctor_state(&state),
        Ok(None) => {
            println!("state readable: true");
            println!("saved state: none");
        }
        Err(error) => {
            println!("state readable: false");
            println!("state error: {error}");
        }
    }
}

fn print_doctor_state(state: &State) {
    println!("state readable: true");
    println!("saved backend: {}", state.active_backend);
    println!("saved wallpaper: {}", state.wallpaper);
    println!("saved mode: {:?}", state.mode);
    if let Some(command) = &state.last_command {
        println!("saved last command: {command}");
    }

    let mut stale_pids = 0;
    for (name, monitor) in &state.monitors {
        let status = pid_status(monitor.pid);
        if status == "stale" {
            stale_pids += 1;
        }
        println!(
            "saved monitor {name}: backend={}, pid_status={}, wallpaper={}",
            monitor.backend, status, monitor.wallpaper
        );
    }
    println!("stale owned live pids: {stale_pids}");
}

fn hyprland_session_detected() -> bool {
    std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some()
        || std::env::var("XDG_CURRENT_DESKTOP")
            .map(|desktop| {
                desktop
                    .split(':')
                    .any(|part| part.eq_ignore_ascii_case("hyprland"))
            })
            .unwrap_or(false)
}

fn apply_runtime_auto_backend(plan: &mut SetPlan, config: &Config, backend_arg: BackendArg) {
    if backend_arg != BackendArg::Auto {
        return;
    }

    if config.general.static_backend != StaticBackendPreference::Auto {
        return;
    }

    if plan.media_kind == crate::media::MediaKind::Static {
        plan.backend = select_static_auto_backend(config);
    }
}

fn execute_set_plan(plan: &SetPlan, config: &Config, state_path: &std::path::Path) -> Result<()> {
    let existing_state = State::load(state_path)?;
    let existing_collections = existing_state
        .as_ref()
        .map(|state| state.collections.clone())
        .unwrap_or_default();

    match plan.backend {
        Backend::Hyprpaper => {
            let monitors = if plan.monitor == "all" {
                monitors::names()?
            } else {
                vec![plan.monitor.clone()]
            };
            stop_owned_swaybg_for_monitors(state_path, &target_monitor_names(plan, &monitors)?)?;
            if let Some(pid) = ensure_hyprpaper_daemon()? {
                println!("started hyprpaper with pid {pid}");
            }
            let commands = hyprpaper::apply_commands(plan, &monitors, &config.hyprpaper);
            run_all(&commands)?;
            stop_owned_live_for_monitors(state_path, &target_monitor_names(plan, &monitors)?)?;
            let mut state = State::merged_with_monitor_entries(
                existing_state.as_ref(),
                plan,
                static_entries(plan, &monitors),
            );
            state.collections = existing_collections;
            state.record_last_command(last_set_command(plan));
            state.save(state_path)?;
            println!(
                "set {} on {} using {}",
                plan.wallpaper.display(),
                plan.monitor,
                plan.backend
            );
        }
        Backend::Mpvpaper => {
            let plans = target_monitor_plans(plan)?;
            let target_monitors = plans
                .iter()
                .map(|plan| plan.monitor.clone())
                .collect::<Vec<_>>();
            stop_owned_swaybg_for_monitors(state_path, &target_monitors)?;
            let previous_live_pids = existing_state
                .as_ref()
                .map(|state| owned_live_pids_for_monitors(state, &target_monitors))
                .unwrap_or_default();
            let started = start_mpvpaper_plans(&plans, config)?;
            if let Err(error) =
                wait_for_mpvpaper_readiness(&started, config.mpvpaper.readiness_timeout_ms)
            {
                let new_pids = started_mpvpaper_pids(&started);
                let _ = terminate_pids(&new_pids);
                bail!("new live wallpaper did not become ready: {error}");
            }
            let entries = started_mpvpaper_entries(&started);
            terminate_pids(&previous_live_pids)?;
            let mut state =
                State::merged_with_monitor_entries(existing_state.as_ref(), plan, entries.clone());
            state.collections = existing_collections;
            state.record_last_command(last_set_command(plan));
            state.save(state_path)?;
            print_live_started(plan, &entries);
        }
        Backend::Awww => {
            if let Some(pid) = ensure_awww_daemon()? {
                println!("started awww-daemon with pid {pid}");
            }
            let monitors = if plan.monitor == "all" {
                monitors::names()?
            } else {
                vec![plan.monitor.clone()]
            };
            let target_monitors = target_monitor_names(plan, &monitors)?;
            stop_owned_swaybg_for_monitors(state_path, &target_monitors)?;
            let live_to_static = existing_state
                .as_ref()
                .map(|state| !owned_live_pids_for_monitors(state, &target_monitors).is_empty())
                .unwrap_or(false);
            let command = if live_to_static {
                awww::apply_instant_command(plan)
            } else {
                awww::apply_command(plan, &config.awww)
            };
            command.run()?;
            stop_owned_live_for_monitors(state_path, &target_monitors)?;
            let mut state = State::merged_with_monitor_entries(
                existing_state.as_ref(),
                plan,
                static_entries(plan, &monitors),
            );
            state.collections = existing_collections;
            state.record_last_command(last_set_command(plan));
            state.save(state_path)?;
            println!(
                "set {} on {} using {}",
                plan.wallpaper.display(),
                plan.monitor,
                plan.backend
            );
        }
        Backend::Swaybg => {
            let monitors = if plan.monitor == "all" {
                monitors::names()?
            } else {
                vec![plan.monitor.clone()]
            };
            let target_monitors = target_monitor_names(plan, &monitors)?;
            stop_owned_swaybg_for_monitors(state_path, &target_monitors)?;
            stop_owned_live_for_monitors(state_path, &target_monitors)?;
            terminate_pids(&omarchy_swaybg_pids())?;

            let pid = swaybg::start_command(plan).spawn_detached()?;
            let mut state = State::merged_with_monitor_entries(
                existing_state.as_ref(),
                plan,
                static_entries_with_pid(plan, &monitors, Some(pid)),
            );
            state.collections = existing_collections;
            state.record_last_command(last_set_command(plan));
            state.save(state_path)?;
            println!(
                "set {} on {} using {}",
                plan.wallpaper.display(),
                plan.monitor,
                plan.backend
            );
        }
    }

    Ok(())
}

fn last_set_command(plan: &SetPlan) -> String {
    format!(
        "set {} --monitor {} --backend {}",
        plan.wallpaper.display(),
        plan.monitor,
        plan.backend
    )
}

fn last_collection_command(step: CollectionStep, collection: &Path, plan: &SetPlan) -> String {
    let command = match step {
        CollectionStep::Next => "next",
        CollectionStep::Previous => "previous",
        CollectionStep::Shuffle => "shuffle",
    };

    format!(
        "{command} {} --monitor {} --backend {}",
        collection.display(),
        plan.monitor,
        plan.backend
    )
}

fn owned_live_pids(state: &State) -> Vec<u32> {
    state
        .monitors
        .values()
        .filter(|monitor| monitor.backend == Backend::Mpvpaper)
        .filter_map(|monitor| monitor.pid)
        .collect()
}

fn owned_live_pids_for_monitors(state: &State, monitors: &[String]) -> Vec<u32> {
    if monitors.iter().any(|monitor| monitor == "all") {
        return owned_live_pids(state);
    }

    monitors
        .iter()
        .filter_map(|monitor| state.monitors.get(monitor))
        .filter(|monitor| monitor.backend == Backend::Mpvpaper)
        .filter_map(|monitor| monitor.pid)
        .collect()
}

fn owned_swaybg_pids_for_monitors(state: &State, monitors: &[String]) -> Vec<u32> {
    let mut pids = if monitors.iter().any(|monitor| monitor == "all") {
        state
            .monitors
            .values()
            .filter(|monitor| monitor.backend == Backend::Swaybg)
            .filter_map(|monitor| monitor.pid)
            .collect::<Vec<_>>()
    } else {
        monitors
            .iter()
            .filter_map(|monitor| state.monitors.get(monitor))
            .filter(|monitor| monitor.backend == Backend::Swaybg)
            .filter_map(|monitor| monitor.pid)
            .collect::<Vec<_>>()
    };
    pids.sort_unstable();
    pids.dedup();
    pids
}

fn terminate_pids(pids: &[u32]) -> Result<usize> {
    let mut stopped = 0;

    for pid in pids {
        if terminate_pid(*pid)? {
            stopped += 1;
        }
    }

    Ok(stopped)
}

fn record_collection_progress(
    state_path: &std::path::Path,
    collection: &std::path::Path,
    index: usize,
    shuffled: bool,
) -> Result<()> {
    let Some(mut state) = State::load(state_path)? else {
        return Ok(());
    };

    let wallpaper = state.wallpaper.clone();
    state.record_collection(collection, Path::new(&wallpaper), index, shuffled);
    state.save(state_path)
}

fn record_last_command_if_state_exists(
    state_path: &std::path::Path,
    command: impl Into<String>,
) -> Result<()> {
    let Some(mut state) = State::load(state_path)? else {
        return Ok(());
    };

    state.record_last_command(command);
    state.save(state_path)
}

fn execute_restore(state: &State, config: &Config, state_path: &std::path::Path) -> Result<()> {
    let profile_plans = restore_profile_plans(config)?;
    if !profile_plans.is_empty() {
        for mut plan in profile_plans {
            apply_runtime_auto_backend(&mut plan, config, BackendArg::Auto);
            execute_set_plan(&plan, config, state_path)?;
        }
        if let Some(mut restored_state) = State::load(state_path)? {
            restored_state.record_last_command("restore");
            restored_state.save(state_path)?;
        }
        return Ok(());
    }

    if state.active_backend != Backend::Mpvpaper {
        let plan = SetPlan::from_state(state)?;
        execute_set_plan(&plan, config, state_path)?;
        if let Some(mut restored_state) = State::load(state_path)? {
            restored_state.record_last_command("restore");
            restored_state.save(state_path)?;
        }
        return Ok(());
    }

    let previous_live_pids = owned_live_pids(state);
    let plans = state.monitor_plans();
    let started = start_mpvpaper_plans(&plans, config)?;
    if let Err(error) = wait_for_mpvpaper_readiness(&started, config.mpvpaper.readiness_timeout_ms)
    {
        let new_pids = started_mpvpaper_pids(&started);
        let _ = terminate_pids(&new_pids);
        bail!("restored live wallpaper did not become ready: {error}");
    }
    let entries = started_mpvpaper_entries(&started);
    terminate_pids(&previous_live_pids)?;
    let mut restored_state = State::from_restored_entries(
        Backend::Mpvpaper,
        state.mode,
        state.wallpaper.clone(),
        entries.clone(),
    )
    .with_last_command("restore");
    restored_state.collections = state.collections.clone();
    restored_state.save(state_path)?;

    if let Some(plan) = plans.first() {
        print_live_started(plan, &entries);
    }

    Ok(())
}

fn restore_profile_plans(config: &Config) -> Result<Vec<SetPlan>> {
    restore_profile_plans_for_monitors(config, monitors::names()?)
}

fn restore_profile_plans_for_monitors(
    config: &Config,
    active_monitors: Vec<String>,
) -> Result<Vec<SetPlan>> {
    if config.monitors.profiles.is_empty() {
        return Ok(Vec::new());
    }

    let mut plans = Vec::new();

    for monitor in active_monitors {
        let Some(profile) = config.monitors.profiles.get(&monitor) else {
            continue;
        };
        let Some(wallpaper) = profile.wallpaper.as_ref() else {
            continue;
        };
        if !wallpaper.exists() {
            bail!(
                "profile wallpaper for {monitor} does not exist: {}",
                wallpaper.display()
            );
        }
        let path = fs::canonicalize(wallpaper).with_context(|| {
            format!(
                "failed to resolve profile wallpaper for {monitor}: {}",
                wallpaper.display()
            )
        })?;
        let backend = match profile.backend {
            BackendPreference::Auto => BackendArg::Auto,
            BackendPreference::Hyprpaper => BackendArg::Hyprpaper,
            BackendPreference::Mpvpaper => BackendArg::Mpvpaper,
            BackendPreference::Awww => BackendArg::Awww,
            BackendPreference::Swaybg => BackendArg::Swaybg,
        };
        plans.push(plan_set(config, &path, Some(monitor), backend.into())?);
    }

    Ok(plans)
}

fn print_restore_plan(state: &State, config: &Config) -> Result<()> {
    println!("restore backend: {}", state.active_backend);
    println!("restore mode: {:?}", state.mode);
    println!("restore wallpaper: {}", state.wallpaper);

    if state.active_backend != Backend::Mpvpaper {
        let plan = SetPlan::from_state(state)?;
        print_set_plan(&plan, config)?;
        return Ok(());
    }

    for plan in state.monitor_plans() {
        println!(
            "command: {}",
            mpvpaper::start_command(&plan, &config.mpvpaper)
        );
    }

    Ok(())
}

fn target_monitor_plans(plan: &SetPlan) -> Result<Vec<SetPlan>> {
    if plan.monitor == "all" {
        return Ok(monitors::names()?
            .into_iter()
            .map(|monitor| plan.for_monitor(monitor))
            .collect());
    }

    Ok(vec![plan.clone()])
}

fn target_monitor_names(plan: &SetPlan, expanded_monitors: &[String]) -> Result<Vec<String>> {
    if plan.monitor == "all" {
        if expanded_monitors.is_empty() {
            return monitors::names();
        }
        return Ok(expanded_monitors.to_vec());
    }

    Ok(vec![plan.monitor.clone()])
}

fn static_entries(plan: &SetPlan, expanded_monitors: &[String]) -> Vec<(String, Option<u32>)> {
    static_entries_with_pid(plan, expanded_monitors, None)
}

fn static_entries_with_pid(
    plan: &SetPlan,
    expanded_monitors: &[String],
    pid: Option<u32>,
) -> Vec<(String, Option<u32>)> {
    if plan.monitor == "all" {
        return expanded_monitors
            .iter()
            .map(|monitor| (monitor.clone(), pid))
            .collect();
    }

    vec![(plan.monitor.clone(), pid)]
}

#[derive(Debug)]
struct StartedMpvpaper {
    monitor: String,
    pid: u32,
    ipc_socket: PathBuf,
}

fn start_mpvpaper_plans(plans: &[SetPlan], config: &Config) -> Result<Vec<StartedMpvpaper>> {
    let mut started = Vec::new();

    for plan in plans {
        let ipc_socket = mpvpaper_ipc_socket(plan);
        remove_stale_socket(&ipc_socket)?;
        let command = mpvpaper::start_command_with_ipc(plan, &config.mpvpaper, &ipc_socket);
        let pid = match command.spawn_detached() {
            Ok(pid) => pid,
            Err(error) => {
                let pids = started_mpvpaper_pids(&started);
                let _ = terminate_pids(&pids);
                return Err(error);
            }
        };
        started.push(StartedMpvpaper {
            monitor: plan.monitor.clone(),
            pid,
            ipc_socket,
        });
    }

    Ok(started)
}

fn started_mpvpaper_entries(started: &[StartedMpvpaper]) -> Vec<(String, Option<u32>)> {
    started
        .iter()
        .map(|entry| (entry.monitor.clone(), Some(entry.pid)))
        .collect()
}

fn started_mpvpaper_pids(started: &[StartedMpvpaper]) -> Vec<u32> {
    started.iter().map(|entry| entry.pid).collect()
}

fn wait_for_mpvpaper_readiness(started: &[StartedMpvpaper], timeout_ms: u64) -> Result<()> {
    let timeout = Duration::from_millis(timeout_ms);
    let deadline = Instant::now() + timeout;
    let mut ready = vec![false; started.len()];

    loop {
        let mut remaining = 0;

        for (index, entry) in started.iter().enumerate() {
            if ready[index] {
                continue;
            }

            if !pid_is_running(entry.pid) {
                bail!(
                    "{} on pid {} exited before reporting readiness",
                    entry.monitor,
                    entry.pid
                );
            }

            match query_mpv_video_output(&entry.ipc_socket) {
                Ok(true) => ready[index] = true,
                Ok(false) => remaining += 1,
                Err(error) if Instant::now() < deadline => {
                    let _ = error;
                    remaining += 1;
                }
                Err(error) => {
                    return Err(error).with_context(|| {
                        format!(
                            "{} on pid {} using {}",
                            entry.monitor,
                            entry.pid,
                            entry.ipc_socket.display()
                        )
                    });
                }
            }
        }

        if remaining == 0 {
            return Ok(());
        }

        if Instant::now() >= deadline {
            let monitors = started
                .iter()
                .enumerate()
                .filter(|(index, _)| !ready[*index])
                .map(|(_, entry)| entry.monitor.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            bail!("timed out waiting for mpv video output on {monitors}");
        }

        std::thread::sleep(Duration::from_millis(50));
    }
}

fn query_mpv_video_output(ipc_socket: &Path) -> Result<bool> {
    let mut stream = UnixStream::connect(ipc_socket)
        .with_context(|| format!("failed to connect to mpv IPC {}", ipc_socket.display()))?;
    stream
        .set_read_timeout(Some(Duration::from_millis(200)))
        .context("failed to set mpv IPC read timeout")?;
    stream
        .set_write_timeout(Some(Duration::from_millis(200)))
        .context("failed to set mpv IPC write timeout")?;
    let request = json!({
        "command": ["get_property", "video-out-params"],
        "request_id": 1,
    });
    writeln!(stream, "{request}").context("failed to write mpv IPC readiness request")?;

    let mut response = String::new();
    BufReader::new(stream)
        .read_line(&mut response)
        .context("failed to read mpv IPC readiness response")?;

    let response: serde_json::Value =
        serde_json::from_str(&response).context("failed to parse mpv IPC readiness response")?;
    if response.get("error").and_then(|error| error.as_str()) != Some("success") {
        return Ok(false);
    }

    Ok(!response.get("data").is_none_or(|data| data.is_null()))
}

fn mpvpaper_ipc_socket(plan: &SetPlan) -> PathBuf {
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
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);

    std::env::temp_dir().join(format!(
        "jobowalls-mpvpaper-{}-{monitor}-{now}.sock",
        std::process::id()
    ))
}

fn remove_stale_socket(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error)
            .with_context(|| format!("failed to remove stale mpv IPC socket {}", path.display())),
    }
}

fn print_live_started(plan: &SetPlan, entries: &[(String, Option<u32>)]) {
    for (monitor, pid) in entries {
        let pid = pid
            .map(|pid| pid.to_string())
            .unwrap_or_else(|| "unknown".to_string());
        println!(
            "started {} on {} using {} with pid {}",
            plan.wallpaper.display(),
            monitor,
            plan.backend,
            pid
        );
    }
}

fn stop_owned_live(state_path: &std::path::Path) -> Result<usize> {
    let Some(mut state) = State::load(state_path)? else {
        return Ok(0);
    };

    let stopped = terminate_pids(&owned_live_pids(&state))?;

    state.clear_live_pids();
    state.save(state_path)?;

    Ok(stopped)
}

fn stop_owned_live_for_monitors(
    state_path: &std::path::Path,
    monitors: &[String],
) -> Result<usize> {
    let Some(mut state) = State::load(state_path)? else {
        return Ok(0);
    };

    let stopped = terminate_pids(&owned_live_pids_for_monitors(&state, monitors))?;

    state.clear_live_pids_for_monitors(monitors);
    state.save(state_path)?;

    Ok(stopped)
}

fn stop_owned_swaybg_for_monitors(
    state_path: &std::path::Path,
    monitors: &[String],
) -> Result<usize> {
    let Some(mut state) = State::load(state_path)? else {
        return Ok(0);
    };

    let stopped = terminate_pids(&owned_swaybg_pids_for_monitors(&state, monitors))?;

    state.clear_backend_pids_for_monitors(Backend::Swaybg, monitors);
    state.save(state_path)?;

    Ok(stopped)
}

fn omarchy_swaybg_pids() -> Vec<u32> {
    let Ok(output) = CommandSpec::new(
        "ps",
        ["-eo".into(), "pid=".into(), "-o".into(), "args=".into()],
    )
    .output_text() else {
        return Vec::new();
    };

    let home = dirs::home_dir();
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

fn run_daemon(config: &Config, state_path: &std::path::Path, once: bool) -> Result<()> {
    loop {
        let decision = live_pause_decision(config);
        let changed = apply_live_pause_decision(state_path, decision.pause)?;
        if decision.pause {
            println!(
                "live pause active: {} ({changed} process signal(s) sent)",
                decision.reasons.join(", ")
            );
        } else {
            println!("live pause inactive ({changed} process signal(s) sent)");
        }

        if once {
            return Ok(());
        }

        std::thread::sleep(Duration::from_secs(2));
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LivePauseDecision {
    pause: bool,
    reasons: Vec<&'static str>,
}

fn live_pause_decision(config: &Config) -> LivePauseDecision {
    live_pause_decision_from_status(
        config,
        battery_power_active(),
        fullscreen_window_active(),
        idle_hint_active(),
    )
}

fn live_pause_decision_from_status(
    config: &Config,
    on_battery: bool,
    fullscreen: bool,
    idle: bool,
) -> LivePauseDecision {
    let mut reasons = Vec::new();
    let pause = &config.live.pause;

    if pause.on_battery && on_battery {
        reasons.push("battery");
    }
    if pause.on_fullscreen && fullscreen {
        reasons.push("fullscreen");
    }
    if pause.on_idle && idle {
        reasons.push("idle");
    }

    LivePauseDecision {
        pause: !reasons.is_empty(),
        reasons,
    }
}

fn apply_live_pause_decision(state_path: &std::path::Path, pause: bool) -> Result<usize> {
    let Some(state) = State::load(state_path)? else {
        return Ok(0);
    };
    let signal = if pause { "STOP" } else { "CONT" };
    let mut signaled = 0;

    for pid in owned_live_pids(&state) {
        if signal_pid(pid, signal)? {
            signaled += 1;
        }
    }

    Ok(signaled)
}

fn battery_power_active() -> bool {
    let Ok(entries) = fs::read_dir("/sys/class/power_supply") else {
        return false;
    };

    for entry in entries.flatten() {
        let status_path = entry.path().join("status");
        let Ok(status) = fs::read_to_string(status_path) else {
            continue;
        };
        if status.trim().eq_ignore_ascii_case("discharging") {
            return true;
        }
    }

    false
}

fn fullscreen_window_active() -> bool {
    let command = CommandSpec::new("hyprctl", ["-j".into(), "activewindow".into()]);
    let Ok(output) = command.output_text() else {
        return false;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&output) else {
        return false;
    };

    value
        .get("fullscreen")
        .and_then(|fullscreen| {
            fullscreen
                .as_bool()
                .or_else(|| fullscreen.as_i64().map(|value| value != 0))
        })
        .unwrap_or(false)
}

fn idle_hint_active() -> bool {
    let Some(session_id) = std::env::var_os("XDG_SESSION_ID") else {
        return false;
    };
    let command = CommandSpec::new(
        "loginctl",
        [
            "show-session".into(),
            session_id,
            "-p".into(),
            "IdleHint".into(),
            "--value".into(),
        ],
    );
    command
        .output_text()
        .map(|output| output.trim().eq_ignore_ascii_case("yes"))
        .unwrap_or(false)
}

fn pid_status(pid: Option<u32>) -> &'static str {
    match pid {
        Some(pid) if pid_is_running(pid) => "running",
        Some(_) => "stale",
        None => "none",
    }
}

fn print_set_plan(plan: &crate::orchestrator::SetPlan, config: &Config) -> Result<()> {
    println!("planned backend: {}", plan.backend);
    println!("media kind: {:?}", plan.media_kind);
    println!("monitor: {}", plan.monitor);
    println!("wallpaper: {}", plan.wallpaper.display());

    match plan.backend {
        Backend::Hyprpaper => {
            let monitors = if plan.monitor == "all" {
                monitors::names()?
            } else {
                Vec::new()
            };

            println!("ensure: {}", hyprpaper::query_command());
            println!("fallback: {}", hyprpaper::daemon_command());
            for command in hyprpaper::apply_commands(plan, &monitors, &config.hyprpaper) {
                println!("command: {command}");
            }
        }
        Backend::Mpvpaper => {
            for plan in target_monitor_plans(plan)? {
                println!(
                    "command: {}",
                    mpvpaper::start_command(&plan, &config.mpvpaper)
                );
            }
        }
        Backend::Awww => {
            println!("ensure: {}", awww::query_command());
            println!("fallback: {}", awww::daemon_command());
            println!("command: {}", awww::apply_command(plan, &config.awww));
        }
        Backend::Swaybg => {
            println!("command: {}", swaybg::start_command(plan));
        }
    }

    Ok(())
}

#[derive(Debug, Serialize)]
struct SetPlanJson<'a> {
    wallpaper: &'a Path,
    media_kind: crate::media::MediaKind,
    backend: Backend,
    monitor: &'a str,
}

fn print_set_plan_json(plan: &crate::orchestrator::SetPlan) -> Result<()> {
    let payload = SetPlanJson {
        wallpaper: &plan.wallpaper,
        media_kind: plan.media_kind,
        backend: plan.backend,
        monitor: &plan.monitor,
    };
    println!("{}", serde_json::to_string_pretty(&payload)?);
    Ok(())
}

fn hyprpaper_daemon_reachable() -> bool {
    hyprpaper::query_command().run().is_ok()
}

fn ensure_hyprpaper_daemon() -> Result<Option<u32>> {
    if hyprpaper_daemon_reachable() {
        return Ok(None);
    }

    let pid = hyprpaper::daemon_command().spawn_detached()?;
    wait_until_reachable(
        hyprpaper_daemon_reachable,
        Duration::from_millis(2_000),
        "hyprpaper daemon",
    )?;
    Ok(Some(pid))
}

fn awww_daemon_reachable() -> bool {
    awww::query_command().run().is_ok()
}

fn ensure_awww_daemon() -> Result<Option<u32>> {
    if awww_daemon_reachable() {
        return Ok(None);
    }

    let pid = awww::daemon_command().spawn_detached()?;
    wait_until_reachable(
        awww_daemon_reachable,
        Duration::from_millis(2_000),
        "awww daemon",
    )?;
    Ok(Some(pid))
}

fn wait_until_reachable(
    mut reachable: impl FnMut() -> bool,
    timeout: Duration,
    description: &str,
) -> Result<()> {
    let deadline = Instant::now() + timeout;

    loop {
        if reachable() {
            return Ok(());
        }

        if Instant::now() >= deadline {
            bail!("{description} did not become reachable");
        }

        std::thread::sleep(Duration::from_millis(50));
    }
}

fn command_available(program: &str) -> bool {
    program_available(program)
}

fn detected_static_auto_backend(config: &Config) -> Backend {
    match config.general.static_backend {
        StaticBackendPreference::Auto => select_static_auto_backend(config),
        StaticBackendPreference::Hyprpaper => Backend::Hyprpaper,
        StaticBackendPreference::Awww => Backend::Awww,
        StaticBackendPreference::Swaybg => Backend::Swaybg,
    }
}

fn select_static_auto_backend(config: &Config) -> Backend {
    select_static_auto_backend_with_availability(
        config,
        backend_adapter(Backend::Hyprpaper).is_available(),
        backend_adapter(Backend::Awww).is_available(),
        backend_adapter(Backend::Swaybg).is_available(),
    )
}

fn select_static_auto_backend_with_availability(
    config: &Config,
    hyprpaper_available: bool,
    awww_available: bool,
    swaybg_available: bool,
) -> Backend {
    if config.awww.enabled && awww_available {
        return Backend::Awww;
    }

    if swaybg_available {
        return Backend::Swaybg;
    }

    if hyprpaper_available {
        return Backend::Hyprpaper;
    }

    if awww_available {
        return Backend::Awww;
    }

    Backend::Swaybg
}

fn backend_adapter(backend: Backend) -> &'static dyn WallpaperBackend {
    match backend {
        Backend::Hyprpaper => &hyprpaper::HyprpaperBackend,
        Backend::Mpvpaper => &mpvpaper::MpvpaperBackend,
        Backend::Awww => &awww::AwwwBackend,
        Backend::Swaybg => &swaybg::SwaybgBackend,
    }
}

struct RuntimePaths {
    config: PathBuf,
    state: PathBuf,
}

impl RuntimePaths {
    fn resolve(config: Option<PathBuf>, state: Option<PathBuf>) -> Result<Self> {
        Ok(Self {
            config: config.unwrap_or_else(default_config_path),
            state: state.unwrap_or_else(default_state_path),
        })
    }
}

fn default_config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("jobowalls")
        .join("config.toml")
}

fn default_state_path() -> PathBuf {
    if let Some(state_home) = std::env::var_os("XDG_STATE_HOME") {
        return PathBuf::from(state_home)
            .join("jobowalls")
            .join("state.json");
    }

    dirs::home_dir()
        .context("failed to resolve home directory")
        .map(|home| {
            home.join(".local")
                .join("state")
                .join("jobowalls")
                .join("state.json")
        })
        .unwrap_or_else(|_| PathBuf::from("state.json"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::MediaKind;

    #[test]
    fn runtime_auto_backend_prefers_swaybg_over_hyprpaper() {
        let config = Config::default();

        assert_eq!(
            select_static_auto_backend_with_availability(&config, true, true, true),
            Backend::Swaybg
        );
    }

    #[test]
    fn runtime_auto_backend_keeps_awww_opt_in() {
        let mut config = Config::default();
        config.awww.enabled = true;

        assert_eq!(
            select_static_auto_backend_with_availability(&config, true, true, true),
            Backend::Awww
        );
    }

    #[test]
    fn runtime_auto_backend_uses_swaybg_on_omarchy_systems() {
        let config = Config::default();

        assert_eq!(
            select_static_auto_backend_with_availability(&config, false, false, true),
            Backend::Swaybg
        );
    }

    #[test]
    fn runtime_auto_backend_falls_back_to_awww_when_swaybg_and_hyprpaper_are_missing() {
        let config = Config::default();

        assert_eq!(
            select_static_auto_backend_with_availability(&config, false, true, false),
            Backend::Awww
        );
    }

    #[test]
    fn runtime_auto_backend_respects_explicit_backend_argument() {
        let mut config = Config::default();
        config.awww.enabled = true;
        let mut plan = SetPlan {
            wallpaper: PathBuf::from("/tmp/wall.png"),
            media_kind: MediaKind::Static,
            backend: Backend::Hyprpaper,
            monitor: "all".to_string(),
        };

        apply_runtime_auto_backend(&mut plan, &config, BackendArg::Hyprpaper);

        assert_eq!(plan.backend, Backend::Hyprpaper);
    }

    #[test]
    fn doctor_static_auto_backend_keeps_awww_opt_in() {
        let config = Config::default();

        assert_eq!(
            select_static_auto_backend_with_availability(&config, true, true, false),
            Backend::Hyprpaper
        );
    }

    #[test]
    fn records_collection_command_shape() {
        let plan = SetPlan {
            wallpaper: PathBuf::from("/tmp/walls/b.mp4"),
            media_kind: MediaKind::Live,
            backend: Backend::Mpvpaper,
            monitor: "DP-1".to_string(),
        };

        assert_eq!(
            last_collection_command(CollectionStep::Next, Path::new("/tmp/walls"), &plan),
            "next /tmp/walls --monitor DP-1 --backend mpvpaper"
        );
    }

    #[test]
    fn serializes_set_plan_json_for_gui() {
        let plan = SetPlan {
            wallpaper: PathBuf::from("/tmp/wall.png"),
            media_kind: MediaKind::Static,
            backend: Backend::Hyprpaper,
            monitor: "all".to_string(),
        };
        let payload = SetPlanJson {
            wallpaper: &plan.wallpaper,
            media_kind: plan.media_kind,
            backend: plan.backend,
            monitor: &plan.monitor,
        };
        let value = serde_json::to_value(payload).unwrap();

        assert_eq!(value["wallpaper"], "/tmp/wall.png");
        assert_eq!(value["media_kind"], "static");
        assert_eq!(value["backend"], "hyprpaper");
        assert_eq!(value["monitor"], "all");
    }

    #[test]
    fn serializes_empty_status_json_for_gui() {
        let value = serde_json::to_value(StatusJson {
            state_exists: false,
            state: None,
        })
        .unwrap();

        assert_eq!(value["state_exists"], false);
        assert!(value.as_object().unwrap().get("wallpaper").is_none());
    }

    #[test]
    fn wait_until_reachable_returns_when_ready() {
        wait_until_reachable(|| true, Duration::from_millis(0), "test daemon").unwrap();
    }

    #[test]
    fn live_pause_decision_respects_enabled_triggers() {
        let mut config = Config::default();
        config.live.pause.on_fullscreen = false;

        let decision = live_pause_decision_from_status(&config, false, true, true);

        assert!(decision.pause);
        assert_eq!(decision.reasons, ["idle"]);
    }

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
    }

    #[test]
    fn restore_profile_plans_use_active_monitor_profiles() {
        let dir = tempfile::tempdir().unwrap();
        let wallpaper = dir.path().join("wall.png");
        fs::write(&wallpaper, b"\x89PNG\r\n\x1a\nrest").unwrap();

        let raw = format!(
            r#"
            [monitors.profiles.DP-1]
            wallpaper = "{}"
            backend = "hyprpaper"

            [monitors.profiles.HDMI-A-1]
            wallpaper = "/tmp/missing.png"
            backend = "hyprpaper"
            "#,
            wallpaper.display()
        );
        let config: Config = toml::from_str(&raw).unwrap();

        let plans = restore_profile_plans_for_monitors(&config, vec!["DP-1".to_string()]).unwrap();

        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].monitor, "DP-1");
        assert_eq!(plans[0].backend, Backend::Hyprpaper);
    }
}
