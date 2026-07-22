-- Brings a database created from the original schema up to 001's shape.
--
-- Idempotent: safe to run against a fresh database created from 001 (every
-- step is a no-op there) and safe to re-run after a partial failure.

-- ---------------------------------------------------------------------------
-- user_vocabs: split the overloaded `guess_count` into exposure vs. mastery.
--
-- `guess_count` counted how often a word was *shown*, but review ordering read
-- it as if it meant how well the word was *known*, so ordering by it carried no
-- learning signal.
-- ---------------------------------------------------------------------------
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'user_vocabs' AND column_name = 'guess_count'
    ) AND NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'user_vocabs' AND column_name = 'seen_count'
    ) THEN
        ALTER TABLE user_vocabs RENAME COLUMN guess_count TO seen_count;
    END IF;
END $$;

ALTER TABLE user_vocabs
    ADD COLUMN IF NOT EXISTS seen_count INT NOT NULL DEFAULT 1,
    ADD COLUMN IF NOT EXISTS correct_count INT NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS last_reviewed_at TIMESTAMP WITH TIME ZONE;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'user_vocabs_correct_count_check'
    ) THEN
        ALTER TABLE user_vocabs
            ADD CONSTRAINT user_vocabs_correct_count_check CHECK (correct_count >= 0);
    END IF;
END $$;

-- ---------------------------------------------------------------------------
-- sentences: keep the first draft and the latest revision separately.
--
-- The old code rebuilt the row on every turn, so `original_text` actually held
-- the sentence that finally passed and `total_fix` was always 0.
-- ---------------------------------------------------------------------------
ALTER TABLE sentences
    ADD COLUMN IF NOT EXISTS final_text TEXT NOT NULL DEFAULT '',
    ADD COLUMN IF NOT EXISTS created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    ADD COLUMN IF NOT EXISTS updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP;

-- Existing rows only ever stored the passing sentence, so seed `final_text`
-- from it rather than leaving the column blank.
UPDATE sentences SET final_text = original_text WHERE final_text = '';

-- ---------------------------------------------------------------------------
-- users: `created_at` was nullable, forcing the loader to invent a timestamp.
-- ---------------------------------------------------------------------------
UPDATE users SET created_at = CURRENT_TIMESTAMP WHERE created_at IS NULL;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'users' AND column_name = 'created_at' AND is_nullable = 'YES'
    ) THEN
        ALTER TABLE users ALTER COLUMN created_at SET NOT NULL;
    END IF;
END $$;

-- ---------------------------------------------------------------------------
-- Indexes supporting the review query and per-user sentence history.
-- ---------------------------------------------------------------------------
CREATE INDEX IF NOT EXISTS idx_user_vocabs_review
    ON user_vocabs (user_id, correct_count ASC, last_reviewed_at ASC NULLS FIRST);

CREATE INDEX IF NOT EXISTS idx_sentences_user
    ON sentences (user_id, created_at DESC);
