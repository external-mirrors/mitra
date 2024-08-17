mod accept;
pub mod activity;
mod add;
mod agreement;
mod announce;
pub mod create;
mod delete;
pub mod emoji;
mod follow;
mod like;
mod r#move;
mod offer;
pub mod proposal;
mod reject;
mod remove;
mod undo;
mod update;

use crate::errors::HandlerError;

use activity::Descriptor;

// Handlers should return activity description if activity has been accepted
// or None if it has been ignored
type HandlerResult = Result<Option<Descriptor>, HandlerError>;
