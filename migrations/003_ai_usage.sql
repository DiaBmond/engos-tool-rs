-- Token accounting for AI calls.
--
-- Deliberately not linked to a user: attribution is not needed for a personal
-- tool, and leaving it out means the usage history is not personal data and
-- survives an account erasure.

CREATE TABLE IF NOT EXISTS ai_usage (
    id BIGSERIAL PRIMARY KEY,
    model VARCHAR(100) NOT NULL,
    -- Which part of the app spent the tokens, e.g. 'roleplay_turn'.
    feature VARCHAR(50) NOT NULL,
    prompt_tokens INT NOT NULL DEFAULT 0 CHECK (prompt_tokens >= 0),
    output_tokens INT NOT NULL DEFAULT 0 CHECK (output_tokens >= 0),
    total_tokens INT NOT NULL DEFAULT 0 CHECK (total_tokens >= 0),
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- The usage report always filters by a time window and groups by feature.
CREATE INDEX IF NOT EXISTS idx_ai_usage_created_at
    ON ai_usage (created_at DESC);

CREATE INDEX IF NOT EXISTS idx_ai_usage_feature
    ON ai_usage (feature, created_at DESC);
