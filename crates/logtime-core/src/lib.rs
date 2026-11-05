//! Domain logic that does not touch the network: settings, calendar
//! summaries, leave, recurring work and the hour planner.

pub mod calendar;
pub mod leave;
pub mod planner;
pub mod recurring;
mod settings;

pub use settings::{LeaveType, RecurringTask, Settings, default_allowance};
