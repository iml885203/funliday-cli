mod auth;
mod browser_login;
mod client;
mod mcp;

use anyhow::{Result, bail};
use chrono::{NaiveDate, NaiveTime, Timelike};
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
    /// Search Funliday city IDs used when creating trips.
    Cities {
        #[command(subcommand)]
        command: CitiesCommand,
    },
    /// Search Funliday places and POI IDs.
    Places {
        #[command(subcommand)]
        command: PlacesCommand,
    },
    /// Run a stdio MCP server (read-only unless explicitly enabled).
    Mcp {
        #[arg(long)]
        enable_write: bool,
    },
}

#[derive(Subcommand)]
enum CitiesCommand {
    Search { query: String },
}

#[derive(Subcommand)]
enum PlacesCommand {
    Search { query: String },
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
    /// Add a Funliday/POI Bank place to one itinerary day.
    AddPlace {
        id: String,
        #[arg(long)]
        day: u32,
        #[arg(long)]
        poi: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        latitude: f64,
        #[arg(long)]
        longitude: f64,
        #[arg(long, default_value_t = 60)]
        stay_minutes: u32,
        #[arg(long)]
        yes: bool,
    },
    /// Add a custom place when Funliday search has no suitable result.
    AddCustomPlace {
        id: String,
        #[arg(long)]
        day: u32,
        #[arg(long)]
        name: String,
        #[arg(long, default_value = "")]
        address: String,
        #[arg(long)]
        latitude: f64,
        #[arg(long)]
        longitude: f64,
        #[arg(long, default_value_t = 60)]
        stay_minutes: u32,
        #[arg(long)]
        yes: bool,
    },
    /// Set a place's fixed start time and stay duration.
    SetPlaceTime {
        id: String,
        item_id: String,
        #[arg(long, value_parser = parse_time)]
        start: NaiveTime,
        #[arg(long)]
        stay_minutes: u32,
        #[arg(long)]
        yes: bool,
    },
    /// Set custom travel duration for the segment arriving at a place.
    SetTransport {
        id: String,
        item_id: String,
        #[arg(long)]
        duration_minutes: u32,
        #[arg(long)]
        yes: bool,
    },
    /// Read a place note.
    ShowPlaceNote { id: String, item_id: String },
    /// Replace a place note.
    SetPlaceNote {
        id: String,
        item_id: String,
        #[arg(long)]
        note: String,
        #[arg(long)]
        yes: bool,
    },
    /// Delete one itinerary place.
    DeletePlace {
        id: String,
        item_id: String,
        #[arg(long)]
        yes: bool,
    },
}

fn parse_date(value: &str) -> std::result::Result<NaiveDate, chrono::ParseError> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
}

fn parse_time(value: &str) -> std::result::Result<NaiveTime, chrono::ParseError> {
    NaiveTime::parse_from_str(value, "%H:%M")
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
                "tripWrite": "create/delete trips; add/delete places; edit times, travel durations, and notes",
                "mcp": "trip discovery and itinerary management; writes available behind --enable-write",
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
                TripsCommand::AddPlace {
                    id,
                    day,
                    poi,
                    name,
                    latitude,
                    longitude,
                    stay_minutes,
                    yes,
                } => {
                    if !yes {
                        bail!("adding a place requires --yes");
                    }
                    client
                        .add_place(&id, day, &poi, &name, latitude, longitude, stay_minutes)
                        .await?
                }
                TripsCommand::AddCustomPlace {
                    id,
                    day,
                    name,
                    address,
                    latitude,
                    longitude,
                    stay_minutes,
                    yes,
                } => {
                    if !yes {
                        bail!("adding a custom place requires --yes");
                    }
                    client
                        .add_custom_place(
                            &id,
                            day,
                            &name,
                            &address,
                            latitude,
                            longitude,
                            stay_minutes,
                        )
                        .await?
                }
                TripsCommand::SetPlaceTime {
                    id,
                    item_id,
                    start,
                    stay_minutes,
                    yes,
                } => {
                    if !yes {
                        bail!("changing a place time requires --yes");
                    }
                    client
                        .update_place_time(
                            &id,
                            &item_id,
                            start.num_seconds_from_midnight(),
                            stay_minutes,
                        )
                        .await?
                }
                TripsCommand::SetTransport {
                    id,
                    item_id,
                    duration_minutes,
                    yes,
                } => {
                    if !yes {
                        bail!("changing transportation requires --yes");
                    }
                    client
                        .set_custom_transport(&id, &item_id, duration_minutes)
                        .await?
                }
                TripsCommand::ShowPlaceNote { id, item_id } => {
                    client.get_place_note(&id, &item_id).await?
                }
                TripsCommand::SetPlaceNote {
                    id,
                    item_id,
                    note,
                    yes,
                } => {
                    if !yes {
                        bail!("changing a place note requires --yes");
                    }
                    client.set_place_note(&id, &item_id, &note).await?
                }
                TripsCommand::DeletePlace { id, item_id, yes } => {
                    if !yes {
                        bail!("deleting a place requires --yes");
                    }
                    client.delete_place(&id, &item_id).await?
                }
            };
            println!("{}", serde_json::to_string_pretty(&value)?);
        }
        Command::Cities { command } => {
            let client = FunlidayClient::new(&auth::load()?)?;
            let value = match command {
                CitiesCommand::Search { query } => client.search_cities(&query).await?,
            };
            println!("{}", serde_json::to_string_pretty(&value)?);
        }
        Command::Places { command } => {
            let client = FunlidayClient::new(&auth::load()?)?;
            let value = match command {
                PlacesCommand::Search { query } => client.search_places(&query).await?,
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
