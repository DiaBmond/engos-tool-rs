use std::sync::Arc;
use sqlx::PgPool;
use crate::infrastructure::database::postgres::vocab_repository::PostgresVocabRepository;
use crate::infrastructure::database::postgres::user_repository::PostgresUserRepository;
use crate::infrastructure::database::postgres::sentence_repository::PostgresSentenceRepository;
use crate::infrastructure::database::redis_repo::RedisChatStateRepository;
use crate::infrastructure::external::gemini::client::GeminiClient;
use crate::infrastructure::external::line_api::LineClient;
use crate::application::roleplay::roleplay_service::RoleplayService;

#[derive(Clone)]
pub struct AppState {
    pub pg_pool: PgPool,
    pub vocab_repo: Arc<PostgresVocabRepository>,
    pub user_repo: Arc<PostgresUserRepository>,
    pub sentence_repo: Arc<PostgresSentenceRepository>,
    pub chat_state_repo: Arc<RedisChatStateRepository>,
    pub gemini_client: Arc<GeminiClient>,
    pub line_client: Arc<LineClient>,
    pub roleplay_service: Arc<RoleplayService<GeminiClient>>,
}

impl AppState {
    pub fn new(
        pg_pool: PgPool,
        chat_state_repo: RedisChatStateRepository,
        gemini_client: GeminiClient,
        line_client: LineClient,
    ) -> Self {
        let vocab_repo = Arc::new(PostgresVocabRepository::new(pg_pool.clone()));
        let user_repo = Arc::new(PostgresUserRepository::new(pg_pool.clone()));
        let sentence_repo = Arc::new(PostgresSentenceRepository::new(pg_pool.clone()));
        
        let gemini_arc = Arc::new(gemini_client);
        let roleplay_service = Arc::new(RoleplayService::new(gemini_arc.as_ref().clone()));

        Self {
            pg_pool,
            vocab_repo,
            user_repo,
            sentence_repo,
            chat_state_repo: Arc::new(chat_state_repo),
            gemini_client: gemini_arc,
            line_client: Arc::new(line_client),
            roleplay_service,
        }
    }
}