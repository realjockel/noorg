use clap::Parser;
use noorg::{
    cli::Cli, handlers::handle_command, logging::init_logging, observer_registry::ObserverRegistry,
    script_loader::ScriptLoader, settings::Settings,
};
use std::{io, sync::Arc};

#[tokio::main]
async fn main() -> io::Result<()> {
    // Parse CLI args first to get debug flag
    let cli = Cli::parse();

    // Initialize logging before any other operations
    init_logging(cli.debug);

    let settings = Settings::new();
    let script_loader = ScriptLoader::new(settings.scripts_dir.clone(), settings.clone());

    // Load observers asynchronously
    let observers = script_loader.load_observers(&settings.enabled_observers)?;
    let observer_registry = Arc::new(ObserverRegistry::new());

    // Register observers
    for observer in observers {
        observer_registry.register(observer).await;
    }

    handle_command(cli.command, settings, observer_registry, None).await?;

    Ok(())
}
