use iced::{Application, Settings as IcedSettings};
use noorg::settings::Settings;
use noorg::settings_dialog::SettingsDialog;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: note_settings <settings_file>");
        std::process::exit(1);
    }

    let settings_path = &args[1];
    let settings = match std::fs::read_to_string(settings_path) {
        Ok(content) => match toml::from_str::<Settings>(&content) {
            Ok(settings) => settings,
            Err(e) => {
                eprintln!("Failed to parse settings: {}", e);
                std::process::exit(1);
            }
        },
        Err(e) => {
            eprintln!("Failed to read settings file: {}", e);
            std::process::exit(1);
        }
    };

    let iced_settings = IcedSettings {
        flags: settings,
        window: iced::window::Settings {
            size: (600, 800),
            ..Default::default()
        },
        ..Default::default()
    };

    if let Err(e) = SettingsDialog::run(iced_settings) {
        eprintln!("Failed to run settings dialog: {}", e);
        std::process::exit(1);
    }
}
