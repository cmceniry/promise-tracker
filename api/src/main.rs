use clap::Parser;
use std::path::PathBuf;
use tracing::{info, warn};

mod server;
mod static_files;
mod storage;
mod validation;

use server::{create_router, AppState};
use storage::Storage;

#[derive(Parser)]
#[command(name = "api")]
#[command(about = "REST API server for promise tracker contracts")]
struct Cli {
    /// Base directory to scan/store contracts
    #[arg(long = "base-dir", default_value = ".")]
    base_dir: PathBuf,

    /// Port to listen on
    #[arg(long, default_value = "8080")]
    port: u16,

    /// Host to bind to
    #[arg(long, default_value = "0.0.0.0")]
    host: String,

    /// Enable dev mode (proxy frontend requests to Trunk dev server)
    #[arg(long)]
    dev: bool,

    /// Dev server URL for frontend proxy (default: http://localhost:3000)
    #[arg(long, default_value = "http://localhost:3000")]
    dev_server_url: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    // Validate base directory
    if !cli.base_dir.exists() {
        anyhow::bail!("Base directory does not exist: {:?}", cli.base_dir);
    }

    if !cli.base_dir.is_dir() {
        anyhow::bail!("Base directory is not a directory: {:?}", cli.base_dir);
    }

    // Create storage and scan directory
    info!(
        "Initializing storage from base directory: {:?}",
        cli.base_dir
    );
    let storage = Storage::new(&cli.base_dir)?;

    // Validate all found contracts
    let contracts = storage.list_contracts();
    info!("Found {} contract(s) in base directory", contracts.len());

    let mut valid_count = 0;
    let mut invalid_count = 0;

    for contract_id in &contracts {
        match storage.load_contract(contract_id) {
            Ok(content) => match validation::validate_contract(&content) {
                Ok(_) => {
                    valid_count += 1;
                }
                Err(e) => {
                    invalid_count += 1;
                    warn!(
                        "Contract {} is invalid and will not be served: {}",
                        contract_id, e
                    );
                }
            },
            Err(e) => {
                invalid_count += 1;
                warn!("Failed to load contract {}: {}", contract_id, e);
            }
        }
    }

    if invalid_count > 0 {
        warn!(
            "Found {} invalid contract(s) that will not be served",
            invalid_count
        );
    }

    info!("Serving {} valid contract(s)", valid_count);

    // Create app state
    let app_state = AppState::new(storage, cli.dev, cli.dev_server_url.clone());

    // Create router
    let app = create_router(app_state);

    // Log dev mode status
    if cli.dev {
        info!(
            "Running in DEV mode - proxying frontend to {}",
            cli.dev_server_url
        );
        info!(
            "Note: For full hot reload support, access Trunk dev server directly at {}",
            cli.dev_server_url
        );
    } else {
        info!("Running in PRODUCTION mode - serving embedded frontend");
    }

    // Start server
    let addr = format!("{}:{}", cli.host, cli.port);
    info!("Starting server on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
