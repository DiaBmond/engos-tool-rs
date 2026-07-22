use std::process::ExitCode;

use sqlx::postgres::PgPoolOptions;

use engos_tool_rs::domain::error::AppResult;
use engos_tool_rs::infrastructure::app_state::AppState;
use engos_tool_rs::infrastructure::config::AppConfig;
use engos_tool_rs::infrastructure::database::postgres::usage_repository::PostgresUsageRepository;
use engos_tool_rs::infrastructure::database::redis_repo::RedisSessionRepository;
use engos_tool_rs::infrastructure::external::gemini::client::GeminiClient;
use engos_tool_rs::infrastructure::external::line_api::LineClient;
use engos_tool_rs::infrastructure::server::start_server;
use engos_tool_rs::infrastructure::{telemetry, usage_writer};

#[tokio::main]
async fn main() -> ExitCode {
    if dotenvy::dotenv().is_err() {
        // Not an error: deployed environments inject variables directly.
        eprintln!("note: no .env file found, reading from the process environment");
    }

    telemetry::init();

    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            // `AppError`'s Display is log-safe: secrets are wrapped and upstream
            // errors are redacted before they reach it.
            tracing::error!(error = %error, kind = error.kind(), "fatal startup error");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> AppResult<()> {
    tracing::info!("starting EngOS server");

    // Validate the whole environment up front so a missing variable fails the
    // process immediately rather than the first request that needs it.
    let config = AppConfig::from_env()?;

    tracing::info!("connecting to PostgreSQL");
    let pg_pool = PgPoolOptions::new()
        .max_connections(config.db_max_connections)
        .acquire_timeout(config.db_acquire_timeout)
        .connect(config.database_url.expose())
        .await?;
    tracing::info!(
        max_connections = config.db_max_connections,
        "PostgreSQL connected"
    );

    tracing::info!("connecting to Redis");
    let session_repo = RedisSessionRepository::new(config.redis_url.expose()).await?;
    tracing::info!("Redis connected");

    // Token accounting is drained by a detached task, so recording a call
    // never blocks or fails a learner's turn.
    let (usage_tx, usage_rx) = tokio::sync::mpsc::unbounded_channel();
    tokio::spawn(usage_writer::run(
        usage_rx,
        PostgresUsageRepository::new(pg_pool.clone()),
    ));

    let gemini_client = GeminiClient::new(
        config.gemini_api_key.expose().to_string(),
        config.gemini_model.clone(),
    )?
    .with_usage_channel(usage_tx);
    let line_client = LineClient::new(config.line_access_token.expose().to_string())?;
    tracing::info!("Gemini and LINE clients initialised");

    let host = config.host.clone();
    let port = config.port;
    let state = AppState::new(config, pg_pool, session_repo, gemini_client, line_client);

    start_server(state, &host, port).await
}
