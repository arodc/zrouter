mod auth;
mod config;
mod error_map;
mod fallback;
mod logging;
mod provider;
mod probe;
mod proxy;
mod router;
mod server;
mod tls;

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

    // 1. Create probe notify first
    let probe_notify = Arc::new(tokio::sync::Notify::new());

    // 2. Create registry with notify injected
    let providers = Arc::new(
        provider::Registry::new(&config, Some(probe_notify.clone())).unwrap_or_else(|e| {
            tracing::error!("Failed to initialize providers: {}", e);
            std::process::exit(1);
        }),
    );

    // 3. Build shared HTTP client (reused by server and probe loop)
    let http_client = Arc::new(server::build_http_client());

    // 4. Assemble app state
    let state = Arc::new(server::AppState {
        config,
        providers,
    });

    // 5. Setup shutdown signal (before spawning probe loop)
    let shutdown_tx = setup_shutdown_signal();

    // 6. Spawn probe loop
    {
        let providers = state.providers.clone();
        let client = http_client.clone();
        let notify = probe_notify.clone();
        let fallback_config = state.config.fallback.clone();
        let shutdown_rx = shutdown_tx.subscribe();

        tokio::spawn(async move {
            probe::run_probe_loop(providers, client, notify, fallback_config, shutdown_rx).await;
        });
    }

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

    let tls_acceptor = match tls::build_server_config(&state.config.server, std::path::Path::new(&args.config)) {
        Ok(Some(server_config)) => {
            tracing::info!("TLS enabled (HTTPS, HTTP/1.1 + HTTP/2)");
            Some(tokio_rustls::TlsAcceptor::from(server_config))
        }
        Ok(None) => {
            tracing::info!("TLS disabled (plain HTTP)");
            None
        }
        Err(e) => {
            tracing::error!("Failed to configure TLS: {}", e);
            std::process::exit(1);
        }
    };

    server::serve(listener, state, tls_acceptor, http_client, shutdown_tx).await;

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
