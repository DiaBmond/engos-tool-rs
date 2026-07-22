# EngOS — English Practice Bot for Developers

A LINE chatbot that helps Thai software developers practise English through four
AI-driven modes: vocabulary drilling, spaced-repetition review, sentence
coaching, and scenario roleplay.

Built in Rust with a hexagonal (ports & adapters) architecture. Axum for HTTP,
PostgreSQL for durable data, Redis for conversation state, and Google Gemini for
the AI.

---

## Table of Contents

- [Features](#features)
- [Architecture](#architecture)
- [Request Flow](#request-flow)
- [Getting Started](#getting-started)
- [Configuration](#configuration)
- [Running](#running)
- [Testing](#testing)
- [Deployment](#deployment)
- [Database](#database)
- [API Endpoints](#api-endpoints)
- [Operational Notes](#operational-notes)
- [Project Layout](#project-layout)
- [Troubleshooting](#troubleshooting)

---

## Features

| Mode | Trigger | What it does |
|------|---------|--------------|
| **Vocab** | `1`, `vocab`, `ทายศัพท์` | Gemini generates 3 words (Daily / Native / Tech). The learner recalls each from its Thai definition. Three attempts per word, then the answer is revealed. |
| **Review** | `2`, `review`, `ทบทวนศัพท์` | Replays previously seen words, weakest first, using spaced-repetition ordering. |
| **Sentence** | `3`, `sentence`, `แต่งประโยค` | The learner drafts an English sentence; the AI coach gives hints in Thai without revealing the answer, and the draft chain is tracked until it passes. |
| **Roleplay** | `4`, `roleplay`, `โรลเพลย์` | A 5-turn scenario matched to the learner's level. The session is graded at the end and awards or removes progress. |

Type `ยกเลิก`, `ออก`, `exit`, or `cancel` at any point to return to the menu.

**Progression.** Learners start at level 1. Five successful sessions advance one
level, up to level 4. A failed roleplay costs one point. All progression flows
through a single domain rule (`User::award_progress`), so every mode shares the
same definition of levelling up.

---

## Architecture

Three layers with a strictly inward dependency direction — `domain` imports
nothing from `infrastructure`.

```
┌──────────────────────────────────────────────────────────────┐
│ infrastructure/          adapters: HTTP, DB, external APIs    │
│   http/       Axum handlers, LINE signature verification      │
│   database/   PostgreSQL repositories, Redis session store    │
│   external/   Gemini client, LINE Messaging client            │
└───────────────────────────┬──────────────────────────────────┘
                            │ implements
┌───────────────────────────▼──────────────────────────────────┐
│ application/             services + port traits               │
│   user/ vocab/ sentence/ roleplay/ session/ messaging/        │
│   deps.rs — AppDeps, the seam handlers are generic over       │
└───────────────────────────┬──────────────────────────────────┘
                            │ uses
┌───────────────────────────▼──────────────────────────────────┐
│ domain/                  entities and business rules          │
│   User, Vocab, UserVocab, Sentence, ChatState, AppError       │
│   No I/O, no framework types.                                 │
└──────────────────────────────────────────────────────────────┘
```

Ports use return-position `impl Future` in traits (RPITIT) rather than
`async_trait`, so there is no `Box::pin` allocation per call. Services are
generic over their ports (`VocabService<R: VocabRepository, A: VocabAiPort>`),
which means static dispatch in production.

Each slice has ports in both directions: **driven** ports the infrastructure
implements (`VocabRepository`, `VocabAiPort`, `MessagingPort`) and **driving**
ports the transport layer consumes (`VocabUseCase`, `UserUseCase`, …).

`application::deps::AppDeps` bundles every driving port behind one associated-type
trait, so HTTP handlers are written as `fn handle<D: AppDeps>(…)` rather than
against concrete Postgres, Redis, Gemini and LINE types. That seam is what makes
the conversation state machine — the most defect-prone code in the project —
testable with in-memory fakes.

**Storage split.** Conversation state is ephemeral by nature and lives in Redis
under a one-hour TTL. Everything worth keeping — users, vocabulary, progress,
sentence history — lives in PostgreSQL.

**Progression has one owner.** Every mode grants progress through
`UserUseCase::award_progress`, which delegates to the domain rule
`User::award_progress`. Grading a roleplay session is deliberately free of side
effects so it cannot become a second, divergent path.

---

## Request Flow

```
LINE Platform
     │  POST /webhook
     ▼
┌─────────────────────────────────────────────────────────┐
│ 1. Verify x-line-signature (HMAC-SHA256, raw bytes)     │
│    invalid ──────────────────────────────► 401          │
│ 2. Parse JSON; malformed ────────────────► 400          │
│ 3. Spawn a background task per text event               │
│ 4. Return 200 immediately  (~0.6 ms)                    │
└──────────────────────┬──────────────────────────────────┘
                       │ tokio::spawn
                       ▼
┌─────────────────────────────────────────────────────────┐
│ 5. Claim webhookEventId in Redis (SET NX EX 600)        │
│    already claimed → drop, it is a LINE retry           │
│ 6. Acquire per-user lock (SET NX EX 90), retrying ~2s   │
│    still held → ask the learner to resend, stop         │
│ 7. Load user (Postgres) + chat state (Redis)            │
│ 8. Dispatch on ChatState → service → Gemini             │
│    (whole turn bounded by a 45s deadline)               │
│ 9. Persist results; write next state                    │
│10. Reply via LINE; fall back to push if token expired   │
│11. Release the lock (Lua CAS: only if still owned)      │
└─────────────────────────────────────────────────────────┘
```

Steps 1–4 are the reason the webhook answers in under a millisecond. LINE times
out after roughly ten seconds and retries on failure; doing AI work before
responding would produce duplicate deliveries and burnt reply tokens.

---

## Getting Started

### Prerequisites

- Rust 1.85+ (edition 2024)
- Docker and Docker Compose
- A LINE Messaging API channel
- A Google Gemini API key ([AI Studio](https://aistudio.google.com/))

### Setup

```bash
git clone <repository-url>
cd engos-tool-rs

cp .env.example .env      # then fill in your credentials
docker compose up -d      # PostgreSQL + Redis, migrations applied automatically

cargo run
```

The server listens on `http://0.0.0.0:8080` by default.

### Exposing the webhook during development

LINE only delivers to a public HTTPS endpoint:

```bash
ngrok http 8080
```

Set the webhook URL in the LINE Developers Console to
`https://<your-ngrok-domain>/webhook` and enable "Use webhook".

---

## Configuration

All variables are read once at start-up and validated. A missing or empty
**required** variable aborts the process rather than failing on the first
request.

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `DATABASE_URL` | ✅ | — | PostgreSQL connection string |
| `REDIS_URL` | ✅ | — | Redis connection string |
| `LINE_CHANNEL_SECRET` | ✅ | — | Used to verify `x-line-signature`. Without it the webhook cannot be authenticated, so it is mandatory. |
| `LINE_CHANNEL_ACCESS_TOKEN` | ✅ | — | LINE Messaging API token |
| `GEMINI_API_KEY` | ✅ | — | Google Gemini API key |
| `GEMINI_MODEL` | — | `gemini-2.5-flash` | Model id |
| `HOST` | — | `0.0.0.0` | Bind address |
| `PORT` | — | `8080` | Bind port |
| `DB_MAX_CONNECTIONS` | — | `20` | PostgreSQL pool size |
| `DB_ACQUIRE_TIMEOUT_SECS` | — | `5` | Pool acquisition timeout |
| `RUST_LOG` | — | `engos_tool_rs=info,tower_http=warn,sqlx=warn` | Log filter |
| `LOG_FORMAT` | — | pretty | Set to `json` for structured output |

`docker-compose.yml` additionally reads `POSTGRES_USER`, `POSTGRES_PASSWORD`,
`POSTGRES_DB`, and `POSTGRES_PORT` from the same `.env`.

> **Never commit `.env`.** It is already listed in `.gitignore`.

---

## Running

```bash
cargo run                                   # development
cargo run --release                         # optimised

RUST_LOG=engos_tool_rs=debug cargo run      # verbose logging
LOG_FORMAT=json cargo run                   # JSON logs for aggregators
```

### Building without a database

`sqlx::query!` verifies SQL at compile time against a live database. Cached
query metadata is committed in `.sqlx/`, so CI and container builds work with no
database reachable — but `SQLX_OFFLINE` must be set, because `sqlx` otherwise
picks up `DATABASE_URL` from `.env` and tries to connect:

```bash
SQLX_OFFLINE=true cargo build --release
```

After changing any SQL, regenerate the cache and commit it:

```bash
cargo install sqlx-cli --no-default-features --features rustls,postgres
cargo sqlx prepare -- --all-targets
git add .sqlx
```

---

## Testing

The straightforward path is with the database running:

```bash
docker compose up -d
cargo test                    # picks up DATABASE_URL from .env
cargo clippy --all-targets
```

Without a database, compilation needs the offline query cache. Note that
`sqlx` reads `DATABASE_URL` from `.env` at *compile* time, so simply unsetting
the shell variable is not enough — `SQLX_OFFLINE` must be set explicitly:

```bash
SQLX_OFFLINE=true cargo test           # 63 unit tests run
SQLX_OFFLINE=true cargo clippy --all-targets
```

Integration tests connect to a real database and skip themselves when
`DATABASE_URL` is unset or unreachable, so the command above passes either way.
To run them, keep the database up and let `.env` supply the URL.

Coverage is concentrated where mistakes are expensive: signature verification,
prompt sanitisation, secret redaction, progression rules, `ChatState`
serialisation (a storage contract with Redis), and the persistence behaviour of
the spaced-repetition and sentence-draft queries.

### Verifying the webhook by hand

```bash
SECRET='your_channel_secret'
BODY='{"events":[{"type":"message","webhookEventId":"e1","replyToken":"rt",
"source":{"type":"user","userId":"U1"},"message":{"type":"text","text":"hello"}}]}'
SIG=$(printf '%s' "$BODY" | openssl dgst -sha256 -hmac "$SECRET" -binary | base64)

curl -i -X POST http://localhost:8080/webhook \
  -H 'Content-Type: application/json' \
  -H "x-line-signature: $SIG" \
  -d "$BODY"
```

Omitting or altering the signature returns `401`.

### What the tests cover

- **State machine** — the conversation handlers run against in-memory fakes via
  the `AppDeps` seam, covering mode selection, guess/attempt limits, review
  completion, draft chains, roleplay turns and grading, and stale-state
  handling.
- **Persistence** — real SQL for spaced repetition, review ordering, draft
  chains and transactional round registration.
- **Security and safety** — signature verification, prompt sanitisation, secret
  redaction.
- **Domain rules** — progression, level ceilings, saturating arithmetic, and
  `ChatState` serialisation (a storage contract with Redis).

---

## Deployment

### Container

```bash
docker build -t engos:latest .

docker run --rm -p 8080:8080 \
  -e DATABASE_URL=... -e REDIS_URL=... \
  -e LINE_CHANNEL_SECRET=... -e LINE_CHANNEL_ACCESS_TOKEN=... \
  -e GEMINI_API_KEY=... \
  engos:latest
```

The image is a two-stage build (~127 MB), runs as an unprivileged user, defaults
to JSON logging, and compiles with `SQLX_OFFLINE=true` so no database is needed
at build time. Its `HEALTHCHECK` probes `/healthz`; orchestrators should also
probe `/readyz`, which checks both stores.

### Continuous integration

`.github/workflows/ci.yml` runs three jobs:

| Job | Checks |
|-----|--------|
| `check` | `cargo fmt --check`, `cargo clippy -D warnings`, unit tests (no database) |
| `integration` | Applies migrations, verifies `.sqlx` is current, runs the full suite against Postgres and Redis services |
| `docker` | Builds the image |

The `.sqlx` freshness check matters: a query changed without re-running
`cargo sqlx prepare` compiles fine locally against a live database but breaks
every database-less build, including the Docker image.

---

## Database

### Schema

| Table | Purpose |
|-------|---------|
| `users` | Learner identity, `current_level`, `progress_stack` |
| `vocabs` | Vocabulary pool, unique on `(word, category)` |
| `user_vocabs` | Per-learner progress: `seen_count`, `correct_count`, `last_reviewed_at` |
| `sentences` | Draft chains: `original_text`, `final_text`, `total_fix`, `is_passed` |

`user_vocabs` deliberately separates exposure (`seen_count`) from mastery
(`correct_count`). Review ordering is
`ORDER BY correct_count ASC, last_reviewed_at ASC NULLS FIRST, RANDOM()`, so the
words a learner keeps missing surface first. `last_reviewed_at` is stamped on
wrong answers too, which prevents a single hard word from being served every
round.

`sentences` keeps the first draft and the latest revision in separate columns.
`original_text` is never overwritten by a revision.

### Migrations

Files in `migrations/` run in filename order. Docker Compose mounts the whole
directory into `docker-entrypoint-initdb.d`, so a fresh volume is fully set up
on first start.

- `001_init.sql` — baseline schema
- `002_spaced_repetition.sql` — idempotent upgrade for databases created from an
  earlier schema; a no-op on a fresh database

Applying migrations to an existing database:

```bash
docker exec -i eng_os_postgres psql -U eng_os_user -d eng_os_db < migrations/002_spaced_repetition.sql
```

Resetting local data entirely:

```bash
docker compose down -v && docker compose up -d
```

---

## API Endpoints

| Method | Path | Purpose |
|--------|------|---------|
| `POST` | `/webhook` | LINE webhook. Requires a valid `x-line-signature`. |
| `GET` | `/` | Liveness |
| `GET` | `/healthz` | Liveness — process is up. Touches no dependencies, so a database blip will not trigger a restart loop. |
| `GET` | `/readyz` | Readiness — verifies PostgreSQL and Redis. `503` when either is unreachable. |

`/readyz` responses:

```json
{ "status": "ok", "postgres": "up", "redis": "up" }
{ "status": "degraded", "reason": "database" }
```

Point orchestrator liveness probes at `/healthz` and readiness probes at
`/readyz`.

---

## Operational Notes

### Security

- **Signature verification** runs on the raw request body before any parsing.
  LINE signs the exact bytes it sent, so deserialising and re-serialising would
  change key order and break verification. Comparison is constant-time.
- **Secrets are wrapped** in a `Secret` type whose `Debug` and `Display` render
  `[REDACTED]`. Upstream error strings additionally pass through
  `redact_secrets()`, because `reqwest::Error` embeds the request URL.
- **The Gemini API key travels in the `x-goog-api-key` header**, never a query
  parameter, so it cannot end up in error messages or proxy logs.
- **Learner text is sanitised** before entering any prompt: quotes escaped,
  newlines flattened, control characters dropped, length capped. This is defence
  in depth — grading remains advisory and can never award more than one level.

### Concurrency and delivery

- **Per-user locks** (`SET NX EX`) serialise concurrent messages from one
  learner. Without them, two quick messages both read the same state and the
  second write silently discards the first turn. Release uses a Lua
  compare-and-delete so a handler cannot delete a lock it no longer owns.
- **A turn is bounded by `TURN_DEADLINE` (45s), strictly below the lock TTL
  (90s)** — a compile-time assertion enforces the ordering. Without that
  invariant a slow AI call plus both LINE attempts could outlive the lock,
  letting a second message interleave. Overrunning work is aborted rather than
  detached, so it cannot write state after the lock is gone.
- **A contended lock is retried** for about two seconds before the learner is
  asked to resend, so an ordinary double-send does not discard what they typed.
- **Panics in background tasks are contained**, reported through `tracing`, and
  still release the lock — `CatchPanicLayer` only covers the HTTP handler, not
  spawned work.
- **Event deduplication** claims each `webhookEventId` for ten minutes, which
  absorbs LINE's retries.
- If Redis is unavailable, both mechanisms degrade to a warning and processing
  continues — an outage should not silence the bot.

### Resilience

The router applies panic recovery, a 256 KB request body limit, a 10-second
request timeout, and HTTP tracing. Shutdown is graceful on `SIGINT` and
`SIGTERM`, letting in-flight work finish rather than cutting learners off
mid-turn during a deploy.

### Observability

Structured logging via `tracing`. Every background task carries a span with
`user_id` and `event_id`, so one learner's turn can be followed end to end.
Errors log a stable `kind` field (`database`, `cache`, `ai_upstream`,
`ai_parse`, `messaging`, …) suitable for alerting, while learners only ever see
a safe Thai message.

### Tunable constants

Defined in `src/domain/chat_state.rs` and `src/infrastructure/http/line_webhook.rs`:

| Constant | Value | Meaning |
|----------|-------|---------|
| `VOCAB_ROUND_SIZE` | 3 | Words per vocab round |
| `MAX_VOCAB_ATTEMPTS` | 3 | Guesses before the answer is revealed |
| `ROLEPLAY_TOTAL_TURNS` | 5 | Turns before a session is graded |
| `STATE_TTL_SECONDS` | 3600 | Conversation state lifetime |
| `TURN_DEADLINE` | 45s | Hard ceiling on one turn |
| `LOCK_TTL_SECONDS` | 90 | Per-user lock lifetime |
| `EVENT_DEDUP_TTL_SECONDS` | 600 | Retry suppression window |
| `STACK_TO_LEVEL_UP` | 5 | Successful sessions per level |
| `MAX_LEVEL` | 4 | Level ceiling |

### Known limitation

There is **no per-user rate limit**. Signature verification blocks anonymous
abuse and `MAX_VOCAB_ATTEMPTS` bounds one obvious loop, but an authenticated
learner can still send messages continuously, and each message costs one or more
Gemini calls. Add a per-user daily quota before exposing this to an untrusted
audience.

---

## Project Layout

```
src/
├── domain/                     Entities and rules; no I/O
│   ├── error.rs                AppError, Secret, redact_secrets
│   ├── user.rs                 Progression rules
│   ├── vocab.rs                Vocab, VocabCategory
│   ├── user_vocab.rs           Spaced-repetition counters
│   ├── sentence.rs             Draft chain
│   └── chat_state.rs           Conversation state machine
│
├── application/                Use cases and port traits
│   ├── deps.rs                 AppDeps — the seam handlers depend on
│   ├── user/                   UserRepository, UserUseCase, UserService
│   ├── vocab/                  VocabRepository, VocabAiPort, VocabUseCase, VocabService
│   ├── sentence/               SentenceRepository, SentenceAiPort, SentenceUseCase
│   ├── roleplay/               RoleplayAiPort, RoleplayUseCase, RoleplayService
│   ├── messaging/              MessagingPort
│   └── session/                ChatStateRepository, SessionLockRepository
│
├── infrastructure/             Adapters
│   ├── config.rs               Validated environment configuration
│   ├── app_state.rs            Dependency wiring
│   ├── server.rs               Router, middleware, graceful shutdown
│   ├── telemetry.rs            tracing setup
│   ├── http/
│   │   ├── line_webhook.rs     Webhook handler and mode dispatch
│   │   ├── line_webhook_tests.rs  State-machine tests using fakes
│   │   ├── signature.rs        HMAC-SHA256 verification
│   │   └── health.rs           Liveness and readiness
│   ├── database/
│   │   ├── postgres/           Repository implementations
│   │   └── redis_repo.rs       State, locks, deduplication
│   └── external/
│       ├── line_api.rs         LINE Messaging client
│       └── gemini/             Gemini client, prompts, AI adapters
│
├── lib.rs
└── main.rs

migrations/                     Ordered SQL migrations
tests/                          Integration tests (require a database)
.sqlx/                          Offline query metadata — commit this
Dockerfile                      Two-stage build, non-root runtime
.github/workflows/ci.yml        Format, lint, tests, image build
rust-toolchain.toml             Pinned toolchain
```

---

## Troubleshooting

**`error communicating with database` at compile time**
`sqlx::query!` needs either a live database or the offline cache. Either start
the database and export `DATABASE_URL`, or build with `SQLX_OFFLINE=true`.

**Process exits at start-up with a config error**
A required variable is missing or empty. The log names it. This is intentional:
the process fails immediately rather than serving broken requests.

**Webhook returns 401**
The signature did not match. Confirm `LINE_CHANNEL_SECRET` matches the channel
sending the request, and that no proxy is rewriting the request body — the
signature covers the exact bytes.

**Webhook returns 200 but nothing arrives in LINE**
Processing is asynchronous, so transport failures appear only in the logs. Look
for `kind="messaging"`; a `401` there means `LINE_CHANNEL_ACCESS_TOKEN` is
wrong. Note that a reply token is single-use and short-lived — the client falls
back to a push message automatically.

**Learner is stuck in a mode**
Have them send `ยกเลิก`. State also expires on its own after an hour. To clear
it manually:

```bash
docker exec -i eng_os_redis redis-cli DEL "eng_os:chat_state:v2:<USER_ID>"
```

**State appears to reset after a deploy**
The Redis key carries a schema version (`v2`). Changing `ChatState`'s shape
requires bumping it so old payloads are not handed to a binary that no longer
understands them. Unreadable payloads are discarded and the learner returns to
the menu rather than being stranded.

---

## License

Add a license before publishing this repository.
