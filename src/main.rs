mod auth;
mod browser_login;
mod client;

use anyhow::Result;
use clap::{Parser, Subcommand};
use serde_json::json;

use client::FunlidayClient;

#[derive(Parser)]
#[command(
    name = "funliday",
    version,
    about = "Unofficial Funliday CLI and MCP server"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Sign in on Funliday's official page using an isolated Chrome profile.
    Login,
    /// Show whether a local session is available (never prints the token).
    Status,
    /// Remove the local session and isolated browser profile.
    Logout,
    /// Print the currently supported discovery status.
    Capabilities,
    /// Read personal itineraries.
    Trips {
        #[command(subcommand)]
        command: TripsCommand,
    },
}

#[derive(Subcommand)]
enum TripsCommand {
    /// List personal itineraries.
    List,
    /// Show trip metadata and its daily itinerary.
    Show { id: String },
}

#[tokio::main]
async fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Login => {
            let credentials = browser_login::login().await?;
            auth::save(&credentials)?;
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &json!({"loggedIn":true,"memberId":credentials.member_id})
                )?
            );
        }
        Command::Status => match auth::load() {
            Ok(credentials) => println!(
                "{}",
                serde_json::to_string_pretty(
                    &json!({"loggedIn":true,"memberId":credentials.member_id})
                )?
            ),
            Err(_) => println!(
                "{}",
                serde_json::to_string_pretty(&json!({"loggedIn":false}))?
            ),
        },
        Command::Logout => {
            auth::delete()?;
            let profile = auth::config_dir()?.join("browser-profile");
            if profile.exists() {
                std::fs::remove_dir_all(profile)?;
            }
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({"loggedIn":false}))?
            );
        }
        Command::Capabilities => println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "browserLogin": "implemented",
                "tripRead": "implemented",
                "tripWrite": "pending authenticated API discovery",
                "mcp": "pending trip API validation",
                "officialDeveloperPortal": "currently unavailable (Heroku application error)"
            }))?
        ),
        Command::Trips { command } => {
            let credentials = auth::load()?;
            let client = FunlidayClient::new(&credentials)?;
            let value = match command {
                TripsCommand::List => client.list_trips().await?,
                TripsCommand::Show { id } => client.get_trip(&id).await?,
            };
            println!("{}", serde_json::to_string_pretty(&value)?);
        }
    }
    Ok(())
}
