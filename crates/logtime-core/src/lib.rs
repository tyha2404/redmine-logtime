//! Domain logic that does not touch the network: settings and calendar summaries.

pub mod calendar;
pub mod planner;
mod settings;

pub use settings::Settings;
