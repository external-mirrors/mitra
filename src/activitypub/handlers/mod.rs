pub use super::receiver::HandlerError;
// Handlers should return object type if activity has been accepted
// or None if it has been ignored
pub type HandlerResult = Result<Option<&'static str>, HandlerError>;

pub mod accept;
pub mod add;
mod agreement;
pub mod announce;
pub mod create;
pub mod delete;
pub mod emoji;
pub mod follow;
pub mod like;
pub mod r#move;
pub mod offer;
pub mod reject;
pub mod remove;
pub mod undo;
pub mod update;
