pub mod action;
pub mod onboarding;
pub mod reduce;
pub mod roles;
pub mod state;

pub use action::{Action, Command, Key, OpenSpec, SyncNote};
pub use reduce::reduce;
pub use state::*;
