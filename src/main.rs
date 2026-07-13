use sqlx::postgres::PgPoolOptions;
use std::env;
use std::time::Duration;

use engos_tool_rs::infrastructure::database::redis_repo::RedisChatStateRepository;
use engos_tool_rs::infrastructure::external::gemini::client::GeminiClient;
use engos_tool_rs::infrastructure::external::line_api::LineClient;
use engos_tool_rs::infrastructure::app_state::AppState;
use engos_tool_rs::infrastructure::server::start_server;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    if let Err(_) = dotenvy::dotenv() {
        println!("⚠️  Note: .env file not found, reading from system environment variables.");
    }

    println!("🛠️  Starting EngOS Server...");

    let db_url = env::var("DATABASE_URL").expect("❌ Missing DATABASE_URL in .env");
    let redis_url = env::var("REDIS_URL").expect("❌ Missing REDIS_URL in .env");
    let host = env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port: u16 = env::var("PORT")
        .unwrap_or_else(|_| "8080".to_string())
        .parse()
        .expect("❌ PORT must be a valid number");

    println!("🗄️  Connecting to PostgreSQL...");
    let pg_pool = PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&db_url)
        .await
        .expect("❌ Failed to connect to PostgreSQL");
    println!("✅ PostgreSQL Connected!");

    println!("🧠 Connecting to Redis...");
    let chat_state_repo = RedisChatStateRepository::new(&redis_url)
        .await
        .expect("❌ Failed to connect to Redis");
    println!("✅ Redis Connected!");

    println!("🤖 Initializing Gemini AI & LINE Clients...");
    let gemini_client = GeminiClient::from_env();
    let line_client = LineClient::from_env();

    let app_state = AppState::new(pg_pool, chat_state_repo, gemini_client, line_client);

    start_server(app_state, &host, port).await?;

    Ok(())
}