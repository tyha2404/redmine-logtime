//! Domain logic that does not touch the network: settings and calendar summaries.

pub mod calendar;
pub mod leave;
pub mod planner;
mod settings;

pub use settings::{LeaveType, Settings, default_allowance};
