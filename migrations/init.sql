CREATE TABLE IF NOT EXISTS users (
    user_id VARCHAR(255) PRIMARY KEY,
    progress_stack SMALLINT NOT NULL DEFAULT 0 CHECK (progress_stack >= 0),
    current_level SMALLINT NOT NULL DEFAULT 1 CHECK (current_level BETWEEN 1 AND 4),
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS vocabs (
    vocab_id VARCHAR(36) PRIMARY KEY,
    word VARCHAR(255) NOT NULL,
    definition TEXT NOT NULL,
    category VARCHAR(50) NOT NULL CHECK (category IN ('Daily', 'Native', 'Tech'))
);

CREATE TABLE IF NOT EXISTS user_vocabs (
    user_id VARCHAR(255) NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    vocab_id VARCHAR(36) NOT NULL REFERENCES vocabs(vocab_id) ON DELETE CASCADE,
    guess_count INT NOT NULL DEFAULT 1 CHECK (guess_count >= 0), 
    PRIMARY KEY (user_id, vocab_id)
);

CREATE TABLE IF NOT EXISTS sentences (
    sentence_id VARCHAR(36) PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    original_text TEXT NOT NULL,
    total_fix SMALLINT NOT NULL DEFAULT 0 CHECK (total_fix >= 0),
    final_feedback TEXT NOT NULL DEFAULT '',
    is_passed BOOLEAN NOT NULL DEFAULT FALSE
);