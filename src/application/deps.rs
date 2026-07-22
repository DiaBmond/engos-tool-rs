use crate::application::messaging::ports::MessagingPort;
use crate::application::roleplay::ports::RoleplayUseCase;
use crate::application::sentence::ports::SentenceUseCase;
use crate::application::session::ports::{ChatStateRepository, SessionLockRepository};
use crate::application::user::ports::UserUseCase;
use crate::application::vocab::ports::VocabUseCase;

/// Everything a transport handler is allowed to reach for, bundled behind one
/// generic parameter.
///
/// Handlers are written against `D: AppDeps` rather than a concrete struct, so
/// the conversation state machine can be driven by in-memory fakes in tests. It
/// previously depended on concrete Postgres, Redis, Gemini and LINE types,
/// which meant the most defect-prone code in the project — the state machine —
/// could not be tested at all.
///
/// Associated types rather than type parameters keep this to a single generic
/// throughout the transport layer.
pub trait AppDeps: Clone + Send + Sync + 'static {
    type Users: UserUseCase;
    type Vocab: VocabUseCase;
    type Sentences: SentenceUseCase;
    type Roleplay: RoleplayUseCase;
    type Session: ChatStateRepository + SessionLockRepository;
    type Messaging: MessagingPort;

    fn users(&self) -> &Self::Users;
    fn vocab(&self) -> &Self::Vocab;
    fn sentences(&self) -> &Self::Sentences;
    fn roleplay(&self) -> &Self::Roleplay;
    fn session(&self) -> &Self::Session;
    fn messaging(&self) -> &Self::Messaging;

    /// Secret used to verify the `x-line-signature` header.
    fn line_channel_secret(&self) -> &str;
}
