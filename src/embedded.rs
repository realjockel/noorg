use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "resources/default_scripts"]
pub struct DefaultScripts;
