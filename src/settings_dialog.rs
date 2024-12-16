use crate::settings::Settings;
use iced::widget::{button, checkbox, column, container, row, text, text_input, Space};
use iced::{
    executor, theme, window, Alignment, Application, Command, Element, Length,
    Settings as IcedSettings, Theme,
};
use rfd::FileDialog;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::error;

pub struct SettingsDialog {
    settings: Arc<Mutex<Settings>>,
    temp_settings: Settings,
    save_message: Option<(String, bool)>, // (message, is_success)
}

#[derive(Debug, Clone)]
pub enum Message {
    FileTypeChanged(String),
    TimestampsToggled(bool),
    SelectNoteDir,
    SelectScriptsDir,
    SelectObsidianVault,
    ObserverToggled(String, bool),
    SaveSettings,
    DismissMessage,
}

impl Application for SettingsDialog {
    type Message = Message;
    type Theme = Theme;
    type Executor = executor::Default;
    type Flags = Settings;

    fn new(settings: Settings) -> (Self, Command<Message>) {
        (
            Self {
                settings: Arc::new(Mutex::new(settings.clone())),
                temp_settings: settings,
                save_message: None,
            },
            Command::none(),
        )
    }

    fn title(&self) -> String {
        String::from("Note CLI Settings")
    }

    fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::FileTypeChanged(value) => {
                self.temp_settings.file_type = value;
            }
            Message::TimestampsToggled(value) => {
                self.temp_settings.timestamps = value;
            }
            Message::SelectNoteDir => {
                if let Some(path) = FileDialog::new().pick_folder() {
                    self.temp_settings.note_dir = path.to_string_lossy().to_string();
                }
            }
            Message::SelectScriptsDir => {
                if let Some(path) = FileDialog::new().pick_folder() {
                    self.temp_settings.scripts_dir = path.to_string_lossy().to_string();
                }
            }
            Message::SelectObsidianVault => {
                if let Some(path) = FileDialog::new().pick_folder() {
                    self.temp_settings.obsidian_vault_path =
                        Some(path.to_string_lossy().to_string());
                }
            }
            Message::ObserverToggled(observer, enabled) => {
                if enabled {
                    self.temp_settings.enabled_observers.push(observer);
                } else {
                    self.temp_settings
                        .enabled_observers
                        .retain(|x| x != &observer);
                }
            }
            Message::SaveSettings => match self.save_settings() {
                Ok(_) => {
                    self.save_message = Some(("Settings saved successfully!".into(), true));
                }
                Err(e) => {
                    self.save_message = Some((format!("Error saving settings: {}", e), false));
                }
            },
            Message::DismissMessage => {
                self.save_message = None;
            }
        }
        Command::none()
    }

    fn view(&self) -> Element<Message> {
        let title = text("Note CLI Settings")
            .size(24)
            .style(theme::Text::Default);

        let section_title_style = |title: &str| text(title).size(16).style(theme::Text::Default);

        let label_style = |label: &str| {
            text(label)
                .style(theme::Text::Default)
                .width(Length::Fixed(120.0))
        };

        let mut content = column![
            title,
            Space::with_height(20),
            // Basic Settings
            section_title_style("Basic Settings"),
            container(
                column![
                    row![
                        label_style("File Type"),
                        text_input("md", &self.temp_settings.file_type)
                            .padding(6)
                            .on_input(Message::FileTypeChanged)
                    ]
                    .spacing(10)
                    .align_items(Alignment::Center),
                    Space::with_height(10),
                    checkbox(
                        "Enable Timestamps",
                        self.temp_settings.timestamps,
                        Message::TimestampsToggled
                    )
                    .text_size(14),
                ]
                .spacing(5)
            )
            .padding(10),
            Space::with_height(20),
            // Directory Settings
            section_title_style("Directories"),
            container(
                column![
                    row![
                        label_style("Note Directory"),
                        text(&self.temp_settings.note_dir).width(Length::Fill),
                        button(text("Choose").size(14))
                            .padding([5, 10])
                            .on_press(Message::SelectNoteDir)
                    ]
                    .spacing(10)
                    .align_items(Alignment::Center),
                    Space::with_height(10),
                    row![
                        label_style("Scripts Directory"),
                        text(&self.temp_settings.scripts_dir).width(Length::Fill),
                        button(text("Choose").size(14))
                            .padding([5, 10])
                            .on_press(Message::SelectScriptsDir)
                    ]
                    .spacing(10)
                    .align_items(Alignment::Center),
                ]
                .spacing(5)
            )
            .padding(10),
            Space::with_height(20),
            // Obsidian Settings
            section_title_style("Obsidian Integration"),
            container(
                row![
                    label_style("Vault Path"),
                    text(
                        self.temp_settings
                            .obsidian_vault_path
                            .as_deref()
                            .unwrap_or("")
                    )
                    .width(Length::Fill),
                    button(text("Choose").size(14))
                        .padding([5, 10])
                        .on_press(Message::SelectObsidianVault)
                ]
                .spacing(10)
                .align_items(Alignment::Center)
            )
            .padding(10),
            Space::with_height(20),
            // Observers
            section_title_style("Enabled Observers"),
            container(
                column(
                    vec!["timestamp", "sqlite", "tag_index", "toc"]
                        .into_iter()
                        .map(|observer| {
                            checkbox(
                                observer,
                                self.temp_settings
                                    .enabled_observers
                                    .contains(&observer.to_string()),
                                move |checked| {
                                    Message::ObserverToggled(observer.to_string(), checked)
                                },
                            )
                            .text_size(14)
                            .spacing(10)
                            .into()
                        })
                        .collect()
                )
                .spacing(10)
            )
            .padding(10),
            Space::with_height(20),
            // Save Button
            button(
                row![text("Save Settings").size(14),]
                    .spacing(10)
                    .align_items(Alignment::Center)
            )
            .padding([8, 16])
            .style(theme::Button::Primary)
            .on_press(Message::SaveSettings),
        ]
        .spacing(5)
        .padding(20);

        // Add save message if present
        if let Some((message, is_success)) = &self.save_message {
            content = content.push(Space::with_height(10)).push(
                container(
                    row![
                        text(if *is_success { "✅ " } else { "❌ " }),
                        text(message),
                        Space::with_width(Length::Fill),
                        button(text("×").size(16))
                            .style(theme::Button::Text)
                            .on_press(Message::DismissMessage)
                    ]
                    .spacing(10)
                    .align_items(Alignment::Center),
                )
                .padding(10)
                .style(if *is_success {
                    theme::Container::Custom(Box::new(SuccessStyle))
                } else {
                    theme::Container::Custom(Box::new(ErrorStyle))
                }),
            );
        }

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x()
            .into()
    }
}

// Add custom styles for success/error messages
struct SuccessStyle;
struct ErrorStyle;

impl container::StyleSheet for SuccessStyle {
    type Style = Theme;

    fn appearance(&self, _style: &Self::Style) -> container::Appearance {
        container::Appearance {
            background: Some(iced::Color::from_rgb(0.0, 0.8, 0.0).into()),
            text_color: Some(iced::Color::WHITE),
            ..Default::default()
        }
    }
}

impl container::StyleSheet for ErrorStyle {
    type Style = Theme;

    fn appearance(&self, _style: &Self::Style) -> container::Appearance {
        container::Appearance {
            background: Some(iced::Color::from_rgb(0.8, 0.0, 0.0).into()),
            text_color: Some(iced::Color::WHITE),
            ..Default::default()
        }
    }
}

impl SettingsDialog {
    pub fn show(settings: Settings) {
        let iced_settings = IcedSettings {
            flags: settings,
            window: window::Settings {
                size: (600, 800),
                position: window::Position::Centered,
                resizable: false,
                decorations: true,
                transparent: false,
                ..Default::default()
            },
            default_text_size: default_text_size(),
            ..Default::default()
        };

        // Run the application
        if let Err(e) = <SettingsDialog as Application>::run(iced_settings) {
            error!("Failed to start settings window: {}", e);
            rfd::MessageDialog::new()
                .set_title("Error")
                .set_description(&format!("Failed to start settings window: {}", e))
                .set_level(rfd::MessageLevel::Error)
                .show();
        }
    }

    fn save_settings(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Get the config file path
        let config_path =
            if let Some(proj_dirs) = directories::ProjectDirs::from("", "norg", "norg") {
                proj_dirs.config_dir().join("config.toml")
            } else {
                return Err("Could not determine config directory".into());
            };

        // Serialize and save the settings
        let config_str = toml::to_string_pretty(&self.temp_settings)?;
        std::fs::write(config_path, config_str)?;

        // Update the shared settings
        let mut settings = self.settings.blocking_lock();
        *settings = self.temp_settings.clone();

        Ok(())
    }
}

#[cfg(target_os = "macos")]
fn default_text_size() -> f32 {
    13.0
}

#[cfg(not(target_os = "macos"))]
fn default_text_size() -> f32 {
    14.0
}
