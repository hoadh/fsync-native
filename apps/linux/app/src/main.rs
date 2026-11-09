//! fsync desktop app for Linux: engine bridge, QML views, tray, single instance.

// A SAS-bypassing control channel must never reach users: release builds refuse the feature
// unless a test pipeline sets FSYNC_ALLOW_TEST_FEATURES=1 (see build.rs).
#[cfg(all(
    feature = "e2e-control",
    not(debug_assertions),
    not(fsync_allow_test_features)
))]
compile_error!(
    "e2e-control is test-only: it must not be built in release mode \
     (set FSYNC_ALLOW_TEST_FEATURES=1 for a test-only release build)"
);

mod bridge;
mod build_info;
mod cli_args;
mod core_state;
mod engine;
mod smoke_core;

use bridge::smoke_log;
use cli_args::Args;
use engine::{boot, shutdown, startup, tray};

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use cxx_qt_lib::{
    QGuiApplication, QMap, QMapPair_QString_QVariant, QQmlApplicationEngine, QString, QUrl,
    QVariant,
};

const MAIN_QML: &str = "qrc:/qt/qml/app/fsync/qml/Main.qml";

fn init_tracing() {
    use tracing_subscriber::EnvFilter;
    let filter = EnvFilter::try_from_env("FSYNC_LOG").unwrap_or_else(|_| EnvFilter::new("info"));
    // Ignored error: a global subscriber already exists (tests), logging still works.
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init();
}

fn main() {
    let args = match Args::parse(std::env::args().skip(1)) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("fsync-linux: {e}");
            std::process::exit(2);
        }
    };
    if args.help {
        print!("{}", engine::shortcut_help::help_text());
        return;
    }
    if args.version {
        println!("fsync-linux {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    if args.build_info {
        println!("{}", build_info::json());
        return;
    }
    init_tracing();
    std::process::exit(run(&args));
}

fn run(args: &Args) -> i32 {
    if args.smoke {
        startup::inert_core();
        return run_gui(true, false);
    }
    let launch = match startup::prepare(args) {
        Ok(l) => l,
        Err(code) => return code,
    };
    boot::spawn(Arc::clone(&launch.core));
    tray::start(Arc::clone(&launch.core));
    if !args.send.is_empty() {
        // This launch became the instance: `--send` is staged once the engine is ready.
        let abs: Vec<_> = args
            .send
            .iter()
            .filter_map(|p| std::path::absolute(p).ok())
            .collect();
        engine::send_files::on_cold_start(&launch.core, abs);
    }
    watch_signals(&launch.core);
    let code = run_gui(false, args.background);
    // Normally the shutdown thread ended the loop and this is a no-op; if the loop ended any
    // other way the ordered shutdown still runs before the process leaves.
    shutdown::finish_after_event_loop(&launch.core);
    code
}

/// SIGTERM/SIGINT/SIGHUP (logout, `kill`) take the same ordered path as Quit.
fn watch_signals(core: &Arc<core_state::Core>) {
    use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};
    let Ok(mut signals) = signal_hook::iterator::Signals::new([SIGTERM, SIGINT, SIGHUP]) else {
        tracing::warn!("cannot install signal handlers");
        return;
    };
    let core = Arc::clone(core);
    let _ = std::thread::Builder::new()
        .name("fsync-signals".into())
        .spawn(move || {
            for _ in signals.forever() {
                shutdown::begin(Some(Arc::clone(&core)));
            }
        });
}

/// Starts Qt and the QML window. With `smoke`, the window quits itself after one event-loop turn
/// and the exit code reports QML warnings or load failure (packaging and container checks).
fn run_gui(smoke: bool, background: bool) -> i32 {
    if smoke {
        smoke_log::install();
        if std::env::var_os("FSYNC_SMOKE_CORE").is_some()
            && let Err(e) = smoke_core::open_and_close()
        {
            eprintln!("smoke: core failed: {e}");
            return 1;
        }
    }
    let mut app = QGuiApplication::new();
    bridge::app_identity::set_desktop_file_name();
    let mut engine = QQmlApplicationEngine::new();
    let Some(mut engine_ref) = engine.as_mut() else {
        eprintln!("cannot create QML engine");
        return 1;
    };
    let mut props = QMap::<QMapPair_QString_QVariant>::default();
    props.insert(QString::from("smoke"), QVariant::from(&smoke));
    props.insert(QString::from("startHidden"), QVariant::from(&background));
    engine_ref.as_mut().set_initial_properties(&props);
    // objectCreated(null) = QML failed to load (objectCreationFailed needs Qt >= 6.4). It fires
    // synchronously inside load(), so record it and return before entering the event loop.
    let load_failed = Arc::new(AtomicBool::new(false));
    let flag = load_failed.clone();
    engine_ref
        .as_mut()
        .on_object_created(move |_, obj, _| {
            if obj.is_null() {
                flag.store(true, Ordering::SeqCst);
            }
        })
        .release();
    engine_ref.load(&QUrl::from(MAIN_QML));
    if load_failed.load(Ordering::SeqCst) {
        eprintln!("QML root object failed to load");
        return 1;
    }
    let code = match app.as_mut() {
        Some(app) => app.exec(),
        None => return 1,
    };
    // Destroy the QML scene first: warnings raised during teardown must count too.
    drop(engine);
    if smoke && smoke_log::qml_warnings() > 0 {
        eprintln!("smoke: {} QML warning(s)", smoke_log::qml_warnings());
        return 1;
    }
    code
}
