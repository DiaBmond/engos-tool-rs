use redis::aio::ConnectionManager;
use redis::AsyncCommands;
use crate::domain::chat_state::ChatState;
use crate::application::user::ports::ChatStateRepository;

#[derive(Clone)]
pub struct RedisChatStateRepository {
    client: ConnectionManager,
}

impl RedisChatStateRepository {
    pub async fn new(redis_url: &str) -> Result<Self, String> {
        let client = redis::Client::open(redis_url)
            .map_err(|e| format!("Invalid Redis URL: {}", e))?;
            
        let connection_manager = ConnectionManager::new(client)
            .await
            .map_err(|e| format!("Failed to connect to Redis: {}", e))?;

        Ok(Self {
            client: connection_manager,
        })
    }

    fn get_key(&self, user_id: &str) -> String {
        format!("eng_os:chat_state:{}", user_id)
    }
}

impl ChatStateRepository for RedisChatStateRepository {

    async fn get_state(&self, user_id: &str) -> Result<ChatState, String> {
        let mut conn = self.client.clone();
        let key = self.get_key(user_id);

        let json_str: Option<String> = conn
            .get(&key)
            .await
            .map_err(|e| format!("Redis get error: {}", e))?;

        if let Some(data) = json_str {
            let state: ChatState = serde_json::from_str(&data)
                .map_err(|e| format!("Failed to parse ChatState JSON: {}", e))?;
            Ok(state)
        } else {
            Ok(ChatState::Idle)
        }
    }

    async fn set_state(&self, user_id: &str, state: &ChatState, ttl_seconds: u64) -> Result<(), String> {
        let mut conn = self.client.clone();
        let key = self.get_key(user_id);

        let json_str = serde_json::to_string(state)
            .map_err(|e| format!("Failed to serialize ChatState: {}", e))?;

        let _: () = conn
            .set_ex(&key, json_str, ttl_seconds)
            .await
            .map_err(|e| format!("Redis set_ex error: {}", e))?;

        Ok(())
    }

    async fn clear_state(&self, user_id: &str) -> Result<(), String> {
        let mut conn = self.client.clone();
        let key = self.get_key(user_id);

        let _: () = conn
            .del(&key)
            .await
            .map_err(|e| format!("Redis del error: {}", e))?;

        Ok(())
    }
}