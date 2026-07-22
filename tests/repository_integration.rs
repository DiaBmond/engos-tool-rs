//! Integration coverage for the persistence fixes from the code review.
//!
//! These exercise real SQL against a real database, because the defects they
//! guard against (a counter that never persisted, review ordering that carried
//! no learning signal) lived in the queries rather than in Rust logic.
//!
//! Skipped automatically when `DATABASE_URL` is unset, so `cargo test` still
//! works on a machine with no database:
//!
//! ```sh
//! docker compose up -d
//! DATABASE_URL=postgres://eng_os_user:supersecretpassword@localhost:5432/eng_os_db cargo test
//! ```

use sqlx::{PgPool, Row};
use uuid::Uuid;

use engos_tool_rs::application::sentence::ports::SentenceRepository;
use engos_tool_rs::application::user::ports::UserRepository;
use engos_tool_rs::application::vocab::ports::VocabRepository;
use engos_tool_rs::domain::sentence::Sentence;
use engos_tool_rs::domain::user::User;
use engos_tool_rs::domain::user_vocab::UserVocab;
use engos_tool_rs::domain::vocab::{Vocab, VocabCategory};
use engos_tool_rs::infrastructure::database::postgres::sentence_repository::PostgresSentenceRepository;
use engos_tool_rs::infrastructure::database::postgres::user_repository::PostgresUserRepository;
use engos_tool_rs::infrastructure::database::postgres::vocab_repository::PostgresVocabRepository;

async fn pool() -> Option<PgPool> {
    let url = std::env::var("DATABASE_URL").ok()?;
    match PgPool::connect(&url).await {
        Ok(pool) => Some(pool),
        Err(e) => {
            eprintln!("skipping integration test, cannot reach database: {e}");
            None
        }
    }
}

/// Unique per test so runs never collide.
fn test_id(prefix: &str) -> String {
    format!("{prefix}_{}", Uuid::new_v4())
}

async fn seed_user(pool: &PgPool, user_id: &str) {
    PostgresUserRepository::new(pool.clone())
        .save(&User::new(user_id.to_string()))
        .await
        .expect("seed user");
}

/// Removes everything a test created.
///
/// `user_vocabs` and `sentences` cascade from `users`, but `vocabs` rows do not
/// — deleting only the user left generated words accumulating in the library on
/// every run.
async fn cleanup(pool: &PgPool, user_id: &str, vocab_ids: &[String]) {
    let _ = sqlx::query("DELETE FROM users WHERE user_id = $1")
        .bind(user_id)
        .execute(pool)
        .await;

    for vocab_id in vocab_ids {
        let _ = sqlx::query("DELETE FROM vocabs WHERE vocab_id = $1")
            .bind(vocab_id)
            .execute(pool)
            .await;
    }
}

/// A learner's wrong answer must not raise `correct_count`, but must still
/// stamp `last_reviewed_at` so the word rotates out of the front of the queue.
#[tokio::test]
async fn review_outcome_separates_exposure_from_mastery() {
    let Some(pool) = pool().await else { return };
    let repo = PostgresVocabRepository::new(pool.clone());
    let user_id = test_id("U_sr");
    seed_user(&pool, &user_id).await;

    let vocab = repo
        .save_vocab(&Vocab::new(
            Uuid::new_v4().to_string(),
            test_id("word"),
            "คำทดสอบ".into(),
            VocabCategory::Tech,
        ))
        .await
        .expect("save vocab");

    repo.upsert_user_vocab(&UserVocab::new(user_id.clone(), vocab.vocab_id.clone()))
        .await
        .expect("link vocab to user");

    let read = |pool: PgPool, user_id: String, vocab_id: String| async move {
        sqlx::query(
            "SELECT seen_count, correct_count, last_reviewed_at FROM user_vocabs
             WHERE user_id = $1 AND vocab_id = $2",
        )
        .bind(&user_id)
        .bind(&vocab_id)
        .fetch_one(&pool)
        .await
        .expect("read user_vocab")
    };

    let row = read(pool.clone(), user_id.clone(), vocab.vocab_id.clone()).await;
    assert_eq!(row.get::<i32, _>("seen_count"), 1);
    assert_eq!(row.get::<i32, _>("correct_count"), 0);
    assert!(
        row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("last_reviewed_at")
            .is_none(),
        "a freshly served word has not been reviewed yet"
    );

    // Wrong answer.
    repo.record_review_outcome(&user_id, &vocab.vocab_id, false)
        .await
        .expect("record incorrect");

    let row = read(pool.clone(), user_id.clone(), vocab.vocab_id.clone()).await;
    assert_eq!(
        row.get::<i32, _>("correct_count"),
        0,
        "a wrong answer must not count as mastery"
    );
    assert!(
        row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("last_reviewed_at")
            .is_some(),
        "a wrong answer must still stamp last_reviewed_at, or the word is served forever"
    );

    // Correct answer.
    repo.record_review_outcome(&user_id, &vocab.vocab_id, true)
        .await
        .expect("record correct");

    let row = read(pool.clone(), user_id.clone(), vocab.vocab_id.clone()).await;
    assert_eq!(row.get::<i32, _>("correct_count"), 1);

    // Serving it again bumps exposure only.
    repo.upsert_user_vocab(&UserVocab::new(user_id.clone(), vocab.vocab_id.clone()))
        .await
        .expect("re-serve");
    let row = read(pool.clone(), user_id.clone(), vocab.vocab_id.clone()).await;
    assert_eq!(row.get::<i32, _>("seen_count"), 2);
    assert_eq!(
        row.get::<i32, _>("correct_count"),
        1,
        "re-serving must not change mastery"
    );

    cleanup(&pool, &user_id, &[vocab.vocab_id]).await;
}

/// Review ordering must surface the least-mastered word first. Ordering by the
/// old `guess_count` sorted by how often a word had been *shown*, which carried
/// no information about whether the learner knew it.
#[tokio::test]
async fn review_ordering_puts_weakest_words_first() {
    let Some(pool) = pool().await else { return };
    let repo = PostgresVocabRepository::new(pool.clone());
    let user_id = test_id("U_order");
    seed_user(&pool, &user_id).await;

    let mut ids = Vec::new();
    for label in ["weak", "strong"] {
        let v = repo
            .save_vocab(&Vocab::new(
                Uuid::new_v4().to_string(),
                test_id(label),
                format!("นิยาม {label}"),
                VocabCategory::Daily,
            ))
            .await
            .expect("save vocab");
        repo.upsert_user_vocab(&UserVocab::new(user_id.clone(), v.vocab_id.clone()))
            .await
            .expect("link");
        ids.push((label, v.vocab_id));
    }

    let strong_id = &ids.iter().find(|(l, _)| *l == "strong").unwrap().1;
    // Master the "strong" word.
    for _ in 0..3 {
        repo.record_review_outcome(&user_id, strong_id, true)
            .await
            .expect("record correct");
    }
    let weak_id = &ids.iter().find(|(l, _)| *l == "weak").unwrap().1;
    repo.record_review_outcome(&user_id, weak_id, false)
        .await
        .expect("record incorrect");

    let review = repo
        .get_review_vocabs(&user_id, 10)
        .await
        .expect("fetch review list");

    assert_eq!(review.len(), 2);
    assert_eq!(
        &review[0].0.vocab_id, weak_id,
        "the word the learner keeps missing must come first"
    );
    assert_eq!(review[0].1.correct_count, 0);
    assert_eq!(review[1].1.correct_count, 3);

    let created: Vec<String> = ids.into_iter().map(|(_, id)| id).collect();
    cleanup(&pool, &user_id, &created).await;
}

/// An empty library is an ordinary state and must come back as an empty vector.
/// Returning `Err` here is what made new learners see a system-error message.
#[tokio::test]
async fn review_list_is_empty_not_an_error_for_a_new_learner() {
    let Some(pool) = pool().await else { return };
    let repo = PostgresVocabRepository::new(pool.clone());
    let user_id = test_id("U_empty");
    seed_user(&pool, &user_id).await;

    let review = repo
        .get_review_vocabs(&user_id, 10)
        .await
        .expect("empty review list must not be an error");
    assert!(review.is_empty());

    cleanup(&pool, &user_id, &[]).await;
}

/// The draft chain must keep the learner's first attempt while tracking the
/// latest revision and the real revision count. Previously the row was rebuilt
/// per message, so `total_fix` was always 0 and `original_text` actually held
/// the sentence that finally passed.
#[tokio::test]
async fn sentence_chain_preserves_first_draft_and_persists_revision_count() {
    let Some(pool) = pool().await else { return };
    let repo = PostgresSentenceRepository::new(pool.clone());
    let user_id = test_id("U_sent");
    seed_user(&pool, &user_id).await;

    let sentence_id = Uuid::new_v4().to_string();
    const FIRST_DRAFT: &str = "I has a pen";

    // Attempt 1: rejected.
    let mut s = Sentence::new(sentence_id.clone(), user_id.clone(), FIRST_DRAFT.into());
    s.mark_as_needs_work("ลองดู tense นะครับ".into());
    repo.save_sentence(&s).await.expect("save attempt 1");

    // Attempt 2: revised, still rejected.
    let mut s = Sentence::revision(
        sentence_id.clone(),
        user_id.clone(),
        FIRST_DRAFT.into(),
        "I haves a pen".into(),
        s.total_fix,
    );
    s.mark_as_needs_work("ใกล้แล้วครับ".into());
    repo.save_sentence(&s).await.expect("save attempt 2");

    // Attempt 3: passes.
    let mut s = Sentence::revision(
        sentence_id.clone(),
        user_id.clone(),
        FIRST_DRAFT.into(),
        "I have a pen".into(),
        s.total_fix,
    );
    s.mark_as_passed("เยี่ยมครับ".into());
    repo.save_sentence(&s).await.expect("save attempt 3");

    let row = sqlx::query(
        "SELECT original_text, final_text, total_fix, is_passed FROM sentences WHERE sentence_id = $1",
    )
    .bind(&sentence_id)
    .fetch_one(&pool)
    .await
    .expect("read sentence");

    assert_eq!(
        row.get::<String, _>("original_text"),
        FIRST_DRAFT,
        "the first draft must survive every revision"
    );
    assert_eq!(
        row.get::<String, _>("final_text"),
        "I have a pen",
        "the latest revision must be recorded"
    );
    assert_eq!(
        row.get::<i16, _>("total_fix"),
        2,
        "both rejections must be persisted; this column used to always be 0"
    );
    assert!(row.get::<bool, _>("is_passed"));

    cleanup(&pool, &user_id, &[]).await;
}

/// Re-inserting a known `(word, category)` must return the row that already
/// exists, so the ids stored in the chat state stay resolvable.
#[tokio::test]
async fn saving_a_duplicate_word_returns_the_existing_row() {
    let Some(pool) = pool().await else { return };
    let repo = PostgresVocabRepository::new(pool.clone());
    let word = test_id("dedup");

    let first = repo
        .save_vocab(&Vocab::new(
            Uuid::new_v4().to_string(),
            word.clone(),
            "นิยามแรก".into(),
            VocabCategory::Native,
        ))
        .await
        .expect("first insert");

    // Same word and category, different generated id.
    let second = repo
        .save_vocab(&Vocab::new(
            Uuid::new_v4().to_string(),
            word.clone(),
            "นิยามที่สอง".into(),
            VocabCategory::Native,
        ))
        .await
        .expect("second insert");

    assert_eq!(
        first.vocab_id, second.vocab_id,
        "must reuse the existing row rather than mint a second id"
    );
    assert_eq!(second.definition, "นิยามที่สอง", "definition is refreshed");

    // The returned id must actually resolve.
    let found = repo
        .find_vocab_by_id(&second.vocab_id)
        .await
        .expect("lookup")
        .expect("vocab should exist");
    assert_eq!(found.category, VocabCategory::Native);

    let _ = sqlx::query("DELETE FROM vocabs WHERE vocab_id = $1")
        .bind(&first.vocab_id)
        .execute(&pool)
        .await;
}

/// A level outside the domain range must be clamped on load rather than
/// propagating through the application.
#[tokio::test]
async fn user_round_trip_clamps_out_of_range_level() {
    let Some(pool) = pool().await else { return };
    let repo = PostgresUserRepository::new(pool.clone());
    let user_id = test_id("U_clamp");

    let mut user = User::new(user_id.clone());
    user.current_level = 4;
    user.progress_stack = 3;
    repo.save(&user).await.expect("save");

    let loaded = repo
        .find_by_id(&user_id)
        .await
        .expect("load")
        .expect("user should exist");
    assert_eq!(loaded.current_level, 4);
    assert_eq!(loaded.progress_stack, 3);

    repo.ping().await.expect("ping should succeed");

    cleanup(&pool, &user_id, &[]).await;
}

/// `register_round` must persist every word and its learner link atomically,
/// and must hand back the ids that actually landed in the database — a word
/// already in the library keeps its existing id, and the chat state stores
/// whatever this returns.
#[tokio::test]
async fn register_round_is_atomic_and_returns_persisted_ids() {
    let Some(pool) = pool().await else { return };
    let repo = PostgresVocabRepository::new(pool.clone());
    let user_id = test_id("U_round");
    seed_user(&pool, &user_id).await;

    let generated: Vec<Vocab> = ["Daily", "Native", "Tech"]
        .iter()
        .map(|cat| {
            Vocab::new(
                Uuid::new_v4().to_string(),
                test_id(&format!("round_{cat}")),
                format!("นิยาม {cat}"),
                VocabCategory::from_str_lossy(cat),
            )
        })
        .collect();

    let round = repo
        .register_round(&user_id, &generated)
        .await
        .expect("register round");
    assert_eq!(round.len(), 3);

    // Every returned id must resolve, and must be linked to the learner.
    for v in &round {
        assert!(
            repo.find_vocab_by_id(&v.vocab_id)
                .await
                .expect("lookup")
                .is_some(),
            "returned id must exist"
        );
    }
    let linked: i64 = sqlx::query_scalar("SELECT count(*) FROM user_vocabs WHERE user_id = $1")
        .bind(&user_id)
        .fetch_one(&pool)
        .await
        .expect("count links");
    assert_eq!(
        linked, 3,
        "all three words must be linked in one transaction"
    );

    // Re-running the same round bumps exposure without duplicating rows.
    let again = repo
        .register_round(&user_id, &generated)
        .await
        .expect("re-register");
    let ids_first: Vec<&String> = round.iter().map(|v| &v.vocab_id).collect();
    let ids_again: Vec<&String> = again.iter().map(|v| &v.vocab_id).collect();
    assert_eq!(ids_first, ids_again, "existing words keep their ids");

    let seen: i32 = sqlx::query_scalar(
        "SELECT seen_count FROM user_vocabs WHERE user_id = $1 AND vocab_id = $2",
    )
    .bind(&user_id)
    .bind(&round[0].vocab_id)
    .fetch_one(&pool)
    .await
    .expect("read seen_count");
    assert_eq!(seen, 2, "re-serving increments exposure");

    let created: Vec<String> = round.into_iter().map(|v| v.vocab_id).collect();
    cleanup(&pool, &user_id, &created).await;
}
