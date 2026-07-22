-- Baseline schema. Applied to a fresh database; existing deployments are
-- brought forward by the numbered migrations that follow.

CREATE TABLE IF NOT EXISTS users (
    user_id VARCHAR(255) PRIMARY KEY,
    progress_stack INT NOT NULL DEFAULT 0 CHECK (progress_stack >= 0),
    current_level SMALLINT NOT NULL DEFAULT 1 CHECK (current_level BETWEEN 1 AND 4),
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS vocabs (
    vocab_id VARCHAR(36) PRIMARY KEY,
    word VARCHAR(255) NOT NULL,
    definition TEXT NOT NULL,
    category VARCHAR(50) NOT NULL CHECK (category IN ('Daily', 'Native', 'Tech')),
    UNIQUE (word, category)
);

CREATE TABLE IF NOT EXISTS user_vocabs (
    user_id VARCHAR(255) NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    vocab_id VARCHAR(36) NOT NULL REFERENCES vocabs(vocab_id) ON DELETE CASCADE,
    -- Times the word has been served to the learner.
    seen_count INT NOT NULL DEFAULT 1 CHECK (seen_count >= 0),
    -- Times the learner recalled it correctly. Drives review ordering.
    correct_count INT NOT NULL DEFAULT 0 CHECK (correct_count >= 0),
    last_reviewed_at TIMESTAMP WITH TIME ZONE,
    PRIMARY KEY (user_id, vocab_id)
);

CREATE TABLE IF NOT EXISTS sentences (
    sentence_id VARCHAR(36) PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    -- The learner's first draft; never overwritten by later revisions.
    original_text TEXT NOT NULL,
    -- The most recent revision, including the one that finally passed.
    final_text TEXT NOT NULL DEFAULT '',
    total_fix SMALLINT NOT NULL DEFAULT 0 CHECK (total_fix >= 0),
    final_feedback TEXT NOT NULL DEFAULT '',
    is_passed BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Review ordering scans a learner's whole vocabulary, weakest words first.
CREATE INDEX IF NOT EXISTS idx_user_vocabs_review
    ON user_vocabs (user_id, correct_count ASC, last_reviewed_at ASC NULLS FIRST);

CREATE INDEX IF NOT EXISTS idx_sentences_user
    ON sentences (user_id, created_at DESC);
