use std::sync::Arc;

use sqlx::PgPool;

use crate::application::deps::AppDeps;
use crate::application::roleplay::roleplay_service::RoleplayService;
use crate::application::sentence::sentence_service::SentenceService;
use crate::application::session::ports::ChatStateRepository;
use crate::application::user::ports::UserUseCase;
use crate::application::user::user_service::UserService;
use crate::application::vocab::vocab_service::VocabService;
use crate::domain::error::AppResult;
use crate::infrastructure::config::AppConfig;
use crate::infrastructure::database::postgres::sentence_repository::PostgresSentenceRepository;
use crate::infrastructure::database::postgres::user_repository::PostgresUserRepository;
use crate::infrastructure::database::postgres::vocab_repository::PostgresVocabRepository;
use crate::infrastructure::database::redis_repo::RedisSessionRepository;
use crate::infrastructure::external::gemini::client::GeminiClient;
use crate::infrastructure::external::line_api::LineClient;

pub type AppVocabService = VocabService<PostgresVocabRepository, GeminiClient>;
pub type AppSentenceService = SentenceService<PostgresSentenceRepository, GeminiClient>;
pub type AppRoleplayService = RoleplayService<GeminiClient>;
pub type AppUserService = UserService<PostgresUserRepository>;

/// The production wiring of [`AppDeps`].
///
/// Handlers never name this type: they are generic over `AppDeps`, so tests can
/// supply an entirely in-memory implementation.
#[derive(Clone)]
pub struct AppState {
    config: Arc<AppConfig>,
    session_repo: Arc<RedisSessionRepository>,
    line_client: Arc<LineClient>,
    user_service: Arc<AppUserService>,
    vocab_service: Arc<AppVocabService>,
    sentence_service: Arc<AppSentenceService>,
    roleplay_service: Arc<AppRoleplayService>,
}

impl AppState {
    pub fn new(
        config: AppConfig,
        pg_pool: PgPool,
        session_repo: RedisSessionRepository,
        gemini_client: GeminiClient,
        line_client: LineClient,
    ) -> Self {
        // `PgPool` and `reqwest::Client` are handle types over a shared pool, so
        // cloning them here shares connections rather than duplicating them.
        let user_service = UserService::new(PostgresUserRepository::new(pg_pool.clone()));

        let vocab_service = VocabService::new(
            PostgresVocabRepository::new(pg_pool.clone()),
            gemini_client.clone(),
        );

        let sentence_service = SentenceService::new(
            PostgresSentenceRepository::new(pg_pool.clone()),
            gemini_client.clone(),
        );

        let roleplay_service = RoleplayService::new(gemini_client);

        Self {
            config: Arc::new(config),
            session_repo: Arc::new(session_repo),
            line_client: Arc::new(line_client),
            user_service: Arc::new(user_service),
            vocab_service: Arc::new(vocab_service),
            sentence_service: Arc::new(sentence_service),
            roleplay_service: Arc::new(roleplay_service),
        }
    }

    /// Verifies both backing stores are reachable. Used by `/readyz`.
    pub async fn health_check(&self) -> AppResult<()> {
        self.user_service.health_check().await?;
        self.session_repo.ping().await
    }
}

impl AppDeps for AppState {
    type Users = AppUserService;
    type Vocab = AppVocabService;
    type Sentences = AppSentenceService;
    type Roleplay = AppRoleplayService;
    type Session = RedisSessionRepository;
    type Messaging = LineClient;

    fn users(&self) -> &Self::Users {
        &self.user_service
    }

    fn vocab(&self) -> &Self::Vocab {
        &self.vocab_service
    }

    fn sentences(&self) -> &Self::Sentences {
        &self.sentence_service
    }

    fn roleplay(&self) -> &Self::Roleplay {
        &self.roleplay_service
    }

    fn session(&self) -> &Self::Session {
        &self.session_repo
    }

    fn messaging(&self) -> &Self::Messaging {
        &self.line_client
    }

    fn line_channel_secret(&self) -> &str {
        self.config.line_channel_secret.expose()
    }
}
