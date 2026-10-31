//! Read-only smoke test against a real Redmine:
//!   REDMINE_URL=http://localhost:3000 REDMINE_API_KEY=... \
//!     cargo run -p redmine-client --example smoke -- 2026-10-01 2026-10-31

use chrono::NaiveDate;
use redmine_client::{ClientOptions, RedmineClient};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let url = std::env::var("REDMINE_URL")?;
    let key = std::env::var("REDMINE_API_KEY")?;
    let mut args = std::env::args().skip(1);
    let from: NaiveDate = args.next().as_deref().unwrap_or("2026-10-01").parse()?;
    let to: NaiveDate = args.next().as_deref().unwrap_or("2026-10-31").parse()?;

    let client = RedmineClient::new(&url, &key, ClientOptions::default())?;
    let me = client.current_user().await?;
    println!("user: {} ({}) #{}", me.display_name(), me.login, me.id);

    let activities = client.time_entry_activities().await?;
    println!(
        "activities: {:?}",
        activities.iter().map(|a| &a.name).collect::<Vec<_>>()
    );

    let entries = client.my_time_entries(from, to).await?;
    let total: f64 = entries.iter().map(|e| e.hours).sum();
    println!("{} entries, {total}h from {from} to {to}", entries.len());
    Ok(())
}
