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
    Login {
        /// Capture an already-open authenticated Chrome debugging session.
        #[arg(long)]
        debug_port: Option<u16>,
    },
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
    /// Manage saved places and collection folders.
    Collections {
        #[command(subcommand)]
        command: CollectionsCommand,
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
enum CollectionsCommand {
    /// List collection folders.
    Folders,
    /// List saved places, optionally within one folder.
    List {
        #[arg(long)]
        folder: Option<String>,
    },
    /// Create a collection folder.
    CreateFolder {
        #[arg(long)]
        name: String,
        #[arg(long)]
        yes: bool,
    },
    /// Delete a collection folder (irreversible).
    DeleteFolder {
        folder_id: String,
        #[arg(long)]
        yes: bool,
    },
    /// Save a searched Funliday place, optionally into a folder.
    AddPlace {
        #[arg(long)]
        poi: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        latitude: f64,
        #[arg(long)]
        longitude: f64,
        #[arg(long)]
        folder: Option<String>,
        #[arg(long)]
        yes: bool,
    },
    /// Remove a saved place using its collection item ID.
    RemovePlace {
        collection_id: String,
        #[arg(long)]
        folder: Option<String>,
        #[arg(long)]
        yes: bool,
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
    /// Set custom travel duration from this place to the next one.
    SetTransport {
        id: String,
        item_id: String,
        #[arg(long)]
        duration_minutes: u32,
        #[arg(long)]
        yes: bool,
    },
    /// Let Funliday calculate travel time from this place to the next one.
    UseAutoTransport {
        id: String,
        item_id: String,
        #[arg(long)]
        yes: bool,
    },
    /// Ask Funliday to calculate the route from this place to the next one.
    CalculateTransport {
        id: String,
        item_id: String,
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
        Command::Login { debug_port } => {
            let credentials = match debug_port {
                Some(port) => browser_login::capture(port).await?,
                None => browser_login::login().await?,
            };
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
                "tripWrite": "create/delete trips; add/delete places; edit times, custom/automatic travel durations, and notes",
                "collections": "list folders and saved places; create/delete folders; save/remove places",
                "mcp": "trip, itinerary, and collection management; writes available behind --enable-write",
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
                TripsCommand::UseAutoTransport { id, item_id, yes } => {
                    if !yes {
                        bail!("enabling automatic transportation requires --yes");
                    }
                    client.use_automatic_transport(&id, &item_id).await?
                }
                TripsCommand::CalculateTransport { id, item_id, yes } => {
                    if !yes {
                        bail!("calculating transportation requires --yes");
                    }
                    client.calculate_transport_route(&id, &item_id).await?
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
        Command::Collections { command } => {
            let client = FunlidayClient::new(&auth::load()?)?;
            let value = match command {
                CollectionsCommand::Folders => client.list_collection_folders().await?,
                CollectionsCommand::List { folder } => {
                    client.list_collections(folder.as_deref()).await?
                }
                CollectionsCommand::CreateFolder { name, yes } => {
                    if !yes {
                        bail!("creating a collection folder requires --yes");
                    }
                    client.create_collection_folder(&name).await?
                }
                CollectionsCommand::DeleteFolder { folder_id, yes } => {
                    if !yes {
                        bail!("deleting a collection folder requires --yes");
                    }
                    client.delete_collection_folder(&folder_id).await?
                }
                CollectionsCommand::AddPlace {
                    poi,
                    name,
                    latitude,
                    longitude,
                    folder,
                    yes,
                } => {
                    if !yes {
                        bail!("saving a place requires --yes");
                    }
                    client
                        .save_collection_place(&poi, &name, latitude, longitude, folder.as_deref())
                        .await?
                }
                CollectionsCommand::RemovePlace {
                    collection_id,
                    folder,
                    yes,
                } => {
                    if !yes {
                        bail!("removing a saved place requires --yes");
                    }
                    client
                        .remove_collection_place(&collection_id, folder.as_deref())
                        .await?
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
