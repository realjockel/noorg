use image::io::Reader as ImageReader;
use noorg::{
    cli::Command, handlers::handle_command, logging::init_logging,
    observer_registry::ObserverRegistry, script_loader::ScriptLoader, settings::Settings,
    window_manager,
};
use std::env;
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::{io, sync::Arc};
use tao::event_loop::{ControlFlow, EventLoop};
use tokio::sync::mpsc;
use tokio::sync::Mutex;
use tracing::{error, info};
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem},
    TrayIconBuilder,
};

#[derive(Debug)]
enum TrayCommand {
    ToggleWatch,
    AddNote,
    Quit,
    UpdateWatchStatus(bool),
    OpenSettings,
    ShowInfo,
}

struct MenuItems {
    watch_item: MenuItem,
    add_note_item: MenuItem,
    settings_item: MenuItem,
    info_item: MenuItem,
    quit_item: MenuItem,
}

impl MenuItems {
    fn update_watch_status(&self, is_watching: bool) {
        self.watch_item.set_text(if is_watching {
            "🟢 Stop Watching"
        } else {
            "🔴 Start Watching"
        });
    }
}

fn show_error(title: &str, message: &str) {
    rfd::MessageDialog::new()
        .set_title(title)
        .set_description(message)
        .set_level(rfd::MessageLevel::Error)
        .show();
}

fn show_input(title: &str, message: &str) -> Option<String> {
    rfd::FileDialog::new()
        .set_title(title)
        .set_file_name(message)
        .save_file()
        .map(|path| {
            path.file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string()
        })
}

#[cfg(not(target_os = "windows"))]
const BASE_PATH: &str = "/usr/local/share/noorg";
#[cfg(target_os = "windows")]
const BASE_PATH: &str = "C:\\Program Files\\noorg";

fn get_base_path() -> PathBuf {
    PathBuf::from(BASE_PATH)
}

fn get_bin_path() -> PathBuf {
    let mut path = get_base_path();
    path.push("bin");
    path
}

fn get_cli_path() -> PathBuf {
    let mut path = get_bin_path();
    #[cfg(target_os = "windows")]
    path.push("note_cli.exe");
    #[cfg(not(target_os = "windows"))]
    path.push("note_cli");
    path
}

fn get_resources_path() -> PathBuf {
    let mut path = get_base_path();
    path.push("resources");
    path
}

#[tokio::main]
async fn main() -> io::Result<()> {
    init_logging(true);

    // Create channels for menu events
    let (tx_watch, mut rx) = mpsc::unbounded_channel();
    let tx_add = tx_watch.clone();
    let tx_quit = tx_watch.clone();

    let event_loop = EventLoop::new();
    let menu = Menu::new();
    let menu_items = MenuItems {
        watch_item: MenuItem::new("🔴 Start Watching", true, None),
        add_note_item: MenuItem::new("Add Note", true, None),
        settings_item: MenuItem::new("⚙️ Settings", true, None),
        info_item: MenuItem::new("ℹ️ Show Info", true, None),
        quit_item: MenuItem::new("Quit", true, None),
    };

    // Set up menu event handlers
    let watch_id = menu_items.watch_item.id().clone();
    let add_id = menu_items.add_note_item.id().clone();
    let settings_id = menu_items.settings_item.id().clone();
    let info_id = menu_items.info_item.id().clone();
    let quit_id = menu_items.quit_item.id().clone();

    menu.append(&menu_items.watch_item).unwrap();
    menu.append(&menu_items.add_note_item).unwrap();
    menu.append(&menu_items.settings_item).unwrap();
    menu.append(&menu_items.info_item).unwrap();
    menu.append(&menu_items.quit_item).unwrap();

    // Register menu event handlers
    let tx_watch_clone = tx_watch.clone();
    let tx_settings = tx_watch.clone();
    let tx_info = tx_watch.clone();

    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        let menu_id = event.id();
        let _ = if *menu_id == watch_id {
            tx_watch_clone.send(TrayCommand::ToggleWatch)
        } else if *menu_id == add_id {
            tx_add.send(TrayCommand::AddNote)
        } else if *menu_id == settings_id {
            tx_settings.send(TrayCommand::OpenSettings)
        } else if *menu_id == info_id {
            tx_info.send(TrayCommand::ShowInfo)
        } else if *menu_id == quit_id {
            tx_quit.send(TrayCommand::Quit)
        } else {
            Ok(())
        };
    }));

    // Create tray icon
    let icon = include_bytes!("../../resources/icon.png");
    let image = ImageReader::new(Cursor::new(icon))
        .with_guessed_format()
        .expect("Failed to guess image format")
        .decode()
        .expect("Failed to decode image");

    let rgba = image.into_rgba8();
    let (width, height) = (rgba.width() as u32, rgba.height() as u32);
    let rgba = rgba.into_raw();

    let icon = match tray_icon::Icon::from_rgba(rgba, width, height) {
        Ok(icon) => icon,
        Err(err) => {
            error!("Failed to create tray icon: {}", err);
            return Err(io::Error::new(io::ErrorKind::Other, err));
        }
    };

    let _tray = TrayIconBuilder::new()
        .with_icon(icon)
        .with_menu(Box::new(menu))
        .with_tooltip("Note CLI")
        .build()
        .unwrap();

    // Create settings wrapped in Arc<Mutex>
    let settings = Arc::new(Mutex::new(Settings::new()));

    // Create script loader with settings
    let settings_guard = settings.lock().await;
    let script_loader =
        ScriptLoader::new(settings_guard.scripts_dir.clone(), settings_guard.clone());

    // Load observers
    let observers = script_loader.load_observers(&settings_guard.enabled_observers)?;
    drop(settings_guard); // Release the lock

    // Create observer registry
    let observer_registry = Arc::new(ObserverRegistry::new());

    // Load and register observers
    for observer in observers {
        observer_registry.register(observer).await;
    }

    // Command handler
    let settings_clone = Arc::clone(&settings);
    let observer_registry_clone = Arc::clone(&observer_registry);
    let is_watching = Arc::new(AtomicBool::new(false));
    let stop_signal = Arc::new(AtomicBool::new(false));

    event_loop.run(move |_event, _event_loop, control_flow| {
        *control_flow = ControlFlow::Wait;

        while let Ok(cmd) = rx.try_recv() {
            match cmd {
                TrayCommand::ToggleWatch => {
                    if !is_watching.load(Ordering::SeqCst) {
                        info!("Starting file watcher...");

                        // Get the path to the note_cli binary using our new path functions
                        let note_cli = get_cli_path();

                        info!("Using note_cli binary at: {:?}", note_cli);
                        if !note_cli.exists() {
                            error!("note_cli binary not found at {:?}", note_cli);
                            show_error(
                                "Failed to start watcher",
                                &format!("note_cli binary not found at {:?}", note_cli),
                            );
                            return;
                        }

                        let settings = settings_clone.clone();
                        let observer_registry = Arc::clone(&observer_registry_clone);
                        let is_watching_clone = Arc::clone(&is_watching);
                        let tx = tx_watch.clone();
                        let stop_signal = Arc::clone(&stop_signal);

                        stop_signal.store(false, Ordering::SeqCst);

                        std::thread::spawn(move || {
                            if let Err(e) =
                                tokio::runtime::Runtime::new().unwrap().block_on(async {
                                    let settings = settings.lock().await;
                                    handle_command(
                                        Command::Watch,
                                        settings.clone(),
                                        observer_registry,
                                        Some(Arc::clone(&stop_signal)),
                                    )
                                    .await
                                })
                            {
                                error!("Failed to start watcher: {}", e);
                                show_error("Failed to start watcher", &e.to_string());
                                is_watching_clone.store(false, Ordering::SeqCst);
                                let _ = tx.send(TrayCommand::UpdateWatchStatus(false));
                                return;
                            }
                            is_watching_clone.store(false, Ordering::SeqCst);
                            let _ = tx.send(TrayCommand::UpdateWatchStatus(false));
                        });

                        // Update UI immediately when starting
                        is_watching.store(true, Ordering::SeqCst);
                        menu_items.update_watch_status(true);
                    } else {
                        info!("Stopping file watcher...");
                        stop_signal.store(true, Ordering::SeqCst);
                        is_watching.store(false, Ordering::SeqCst);
                        menu_items.update_watch_status(false);
                    }
                }
                TrayCommand::UpdateWatchStatus(watching) => {
                    menu_items.update_watch_status(watching);
                }
                TrayCommand::AddNote => {
                    if let Some(title) = show_input("New Note", "Enter note title") {
                        let settings = settings_clone.clone();
                        let observer_registry = Arc::clone(&observer_registry_clone);
                        let title_clone = title.clone();

                        std::thread::spawn(move || {
                            if let Err(e) =
                                tokio::runtime::Runtime::new().unwrap().block_on(async {
                                    let settings = settings.lock().await;
                                    handle_command(
                                        Command::Add {
                                            title: title_clone,
                                            body: None,
                                            frontmatter: vec![],
                                        },
                                        settings.clone(),
                                        observer_registry,
                                        None,
                                    )
                                    .await
                                })
                            {
                                error!("Failed to create note: {}", e);
                                show_error("Failed to create note", &e.to_string());
                            }
                        });
                    }
                }
                TrayCommand::Quit => {
                    info!("Quitting...");
                    std::process::exit(0);
                }
                TrayCommand::OpenSettings => {
                    let settings = Arc::clone(&settings_clone);
                    window_manager::open_settings(settings);
                }
                TrayCommand::ShowInfo => {
                    let settings = settings_clone.clone();
                    std::thread::spawn(move || {
                        let rt = tokio::runtime::Runtime::new().unwrap();
                        let settings_guard = rt.block_on(async {
                            let settings = settings.lock().await;
                            settings.clone()
                        });

                        let message = format!(
                            "Watched Directory: {}\n\
                             File Type: {}\n\
                             Active Observers: {}",
                            settings_guard.note_dir,
                            settings_guard.file_type,
                            settings_guard.enabled_observers.join(", ")
                        );

                        rfd::MessageDialog::new()
                            .set_title("Note Watcher Info")
                            .set_description(&message)
                            .set_level(rfd::MessageLevel::Info)
                            .show();
                    });
                }
            }
        }
    });
}
