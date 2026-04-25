mod auth;
mod config;
mod fallback;
mod logging;
mod provider;
mod proxy;
mod router;
mod server;

use std::sync::Arc;

use clap::Parser;

#[derive(Parser)]
#[command(name = "zrouter", about = "Anthropic API routing daemon")]
struct Args {
    #[arg(short, long)]
    config: String,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    let config = config::load(&args.config).unwrap_or_else(|e| {
        eprintln!("Failed to load config from {}: {}", args.config, e);
        std::process::exit(1);
    });

    logging::init(&config.logging);

    tracing::info!("zrouter starting");

    let providers = provider::Registry::new(&config).unwrap_or_else(|e| {
        tracing::error!("Failed to initialize providers: {}", e);
        std::process::exit(1);
    });

    let state = Arc::new(server::AppState {
        config,
        providers,
    });

    let addr = format!(
        "{}:{}",
        state.config.server.bind, state.config.server.port
    );

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .unwrap_or_else(|e| {
            tracing::error!("Failed to bind to {}: {}", addr, e);
            std::process::exit(1);
        });

    tracing::info!("Listening on {}", addr);

    let shutdown = setup_shutdown_signal();
    server::serve(listener, state, shutdown).await;

    tracing::info!("zrouter shut down");
}

fn setup_shutdown_signal() -> tokio::sync::broadcast::Sender<()> {
    let (tx, _) = tokio::sync::broadcast::channel(1);

    let tx_sigint = tx.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to listen for ctrl-c");
        let _ = tx_sigint.send(());
    });

    #[cfg(unix)]
    {
        let tx_sigterm = tx.clone();
        tokio::spawn(async move {
            let mut sigterm = tokio::signal::unix::signal(
                tokio::signal::unix::SignalKind::terminate(),
            )
            .expect("Failed to listen for SIGTERM");
            sigterm.recv().await;
            let _ = tx_sigterm.send(());
        });
    }

    tx
}
