mod auth;
mod browser_login;
mod client;
mod mcp;

use anyhow::{Result, bail};
use chrono::NaiveDate;
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
    /// Run a stdio MCP server (read-only unless explicitly enabled).
    Mcp {
        #[arg(long)]
        enable_write: bool,
    },
}

#[derive(Subcommand)]
enum TripsCommand {
    /// List personal itineraries.
    List,
    /// Show trip metadata and its daily itinerary.
    Show { id: String },
    /// Create a personal itinerary. City IDs currently come from Funliday autocomplete.
    Create {
        #[arg(long)]
        name: String,
        #[arg(long = "city", required = true)]
        cities: Vec<String>,
        #[arg(long, value_parser = parse_date)]
        start: NaiveDate,
        #[arg(long, value_parser = parse_date)]
        end: NaiveDate,
        /// 1 solo, 2 couple, 3 friends, 4 family.
        #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u8).range(1..=4))]
        trip_type: u8,
        #[arg(long)]
        yes: bool,
    },
    /// Delete a trip using its containerId (irreversible).
    Delete {
        container_id: String,
        #[arg(long)]
        yes: bool,
    },
}

fn parse_date(value: &str) -> std::result::Result<NaiveDate, chrono::ParseError> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
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
                "tripWrite": "create and delete implemented; update and itinerary edits pending",
                "mcp": "read tools implemented; create and delete available behind --enable-write",
                "officialDeveloperPortal": "currently unavailable (Heroku application error)"
            }))?
        ),
        Command::Trips { command } => {
            let credentials = auth::load()?;
            let client = FunlidayClient::new(&credentials)?;
            let value = match command {
                TripsCommand::List => client.list_trips().await?,
                TripsCommand::Show { id } => client.get_trip(&id).await?,
                TripsCommand::Create {
                    name,
                    cities,
                    start,
                    end,
                    trip_type,
                    yes,
                } => {
                    if !yes {
                        bail!("creating a trip requires --yes");
                    }
                    if end < start {
                        bail!("end date cannot be before start date");
                    }
                    client
                        .create_trip(
                            &name,
                            &cities,
                            &start.to_string(),
                            &end.to_string(),
                            trip_type,
                        )
                        .await?
                }
                TripsCommand::Delete { container_id, yes } => {
                    if !yes {
                        bail!("deleting a trip requires --yes");
                    }
                    client.delete_trip(&container_id).await?
                }
            };
            println!("{}", serde_json::to_string_pretty(&value)?);
        }
        Command::Mcp { enable_write } => {
            let credentials = auth::load()?;
            mcp::serve(FunlidayClient::new(&credentials)?, enable_write).await?;
        }
    }
    Ok(())
}
