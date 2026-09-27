use anyhow::{Context, Result, bail};
use chrono::NaiveDate;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::client::FunlidayClient;

pub async fn serve(client: FunlidayClient, enable_write: bool) -> Result<()> {
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut stdout = tokio::io::stdout();
    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }
        let request: Value = serde_json::from_str(&line).context("invalid MCP JSON-RPC request")?;
        if let Some(response) = handle(&client, enable_write, request).await {
            stdout
                .write_all(serde_json::to_string(&response)?.as_bytes())
                .await?;
            stdout.write_all(b"\n").await?;
            stdout.flush().await?;
        }
    }
    Ok(())
}

async fn handle(client: &FunlidayClient, enable_write: bool, request: Value) -> Option<Value> {
    let id = request.get("id").cloned()?;
    let method = request
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let result = match method {
        "initialize" => Ok(json!({
            "protocolVersion":"2025-06-18",
            "capabilities":{"tools":{}},
            "serverInfo":{"name":"funliday","version":env!("CARGO_PKG_VERSION")}
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({"tools":tools(enable_write)})),
        "tools/call" => {
            call_tool(
                client,
                enable_write,
                request.get("params").cloned().unwrap_or_default(),
            )
            .await
        }
        _ => Err(anyhow::anyhow!("method not found: {method}")),
    };
    Some(match result {
        Ok(result) => json!({"jsonrpc":"2.0","id":id,"result":result}),
        Err(error) => {
            json!({"jsonrpc":"2.0","id":id,"error":{"code":-32000,"message":error.to_string()}})
        }
    })
}

fn tools(enable_write: bool) -> Vec<Value> {
    let mut result = vec![
        json!({"name":"list_trips","description":"List the authenticated user's private Funliday trips.","inputSchema":{"type":"object","properties":{}}}),
        json!({"name":"get_trip","description":"Read a private Funliday trip and its itinerary.","inputSchema":{"type":"object","properties":{"id":{"type":"string"}},"required":["id"]}}),
        json!({"name":"search_cities","description":"Search Funliday city IDs for trip creation.","inputSchema":{"type":"object","properties":{"query":{"type":"string","minLength":1}},"required":["query"]}}),
        json!({"name":"search_places","description":"Search Funliday/POI Bank places and IDs.","inputSchema":{"type":"object","properties":{"query":{"type":"string","minLength":1}},"required":["query"]}}),
        json!({"name":"get_trip_place_note","description":"Read a trip place note.","inputSchema":{"type":"object","properties":{"trip_id":{"type":"string"},"item_id":{"type":"string"}},"required":["trip_id","item_id"]}}),
    ];
    if enable_write {
        result.push(json!({"name":"create_trip","description":"Create a private Funliday trip. Requires a Funliday city ID and explicit confirmation.","inputSchema":{"type":"object","properties":{"name":{"type":"string","minLength":1},"city_ids":{"type":"array","items":{"type":"string"},"minItems":1},"start":{"type":"string","format":"date"},"end":{"type":"string","format":"date"},"trip_type":{"type":"integer","minimum":1,"maximum":4,"default":1},"confirm":{"const":true}},"required":["name","city_ids","start","end","confirm"]}}));
        result.push(json!({"name":"delete_trip","description":"Permanently delete a Funliday trip using its containerId.","inputSchema":{"type":"object","properties":{"container_id":{"type":"string"},"confirm":{"const":true}},"required":["container_id","confirm"]}}));
        result.push(json!({"name":"add_trip_place","description":"Add a searched place to a numbered trip day.","inputSchema":{"type":"object","properties":{"trip_id":{"type":"string"},"day":{"type":"integer","minimum":1},"poi_id":{"type":"string"},"name":{"type":"string"},"latitude":{"type":"number"},"longitude":{"type":"number"},"stay_minutes":{"type":"integer","minimum":0,"default":60},"confirm":{"const":true}},"required":["trip_id","day","poi_id","name","latitude","longitude","confirm"]}}));
        result.push(json!({"name":"add_custom_trip_place","description":"Add a custom place when Funliday search has no suitable result.","inputSchema":{"type":"object","properties":{"trip_id":{"type":"string"},"day":{"type":"integer","minimum":1},"name":{"type":"string"},"address":{"type":"string","default":""},"latitude":{"type":"number"},"longitude":{"type":"number"},"stay_minutes":{"type":"integer","minimum":0,"default":60},"confirm":{"const":true}},"required":["trip_id","day","name","latitude","longitude","confirm"]}}));
        result.push(json!({"name":"set_trip_place_time","description":"Set a trip place's fixed local start time and stay duration.","inputSchema":{"type":"object","properties":{"trip_id":{"type":"string"},"item_id":{"type":"string"},"start_seconds":{"type":"integer","minimum":0,"maximum":86399},"stay_minutes":{"type":"integer","minimum":0},"confirm":{"const":true}},"required":["trip_id","item_id","start_seconds","stay_minutes","confirm"]}}));
        result.push(json!({"name":"set_trip_place_transport","description":"Set custom travel time from this place to the next one.","inputSchema":{"type":"object","properties":{"trip_id":{"type":"string"},"item_id":{"type":"string"},"duration_minutes":{"type":"integer","minimum":0},"confirm":{"const":true}},"required":["trip_id","item_id","duration_minutes","confirm"]}}));
        result.push(json!({"name":"use_automatic_trip_place_transport","description":"Let Funliday calculate travel time from this place to the next one.","inputSchema":{"type":"object","properties":{"trip_id":{"type":"string"},"item_id":{"type":"string"},"confirm":{"const":true}},"required":["trip_id","item_id","confirm"]}}));
        result.push(json!({"name":"calculate_trip_place_transport","description":"Ask Funliday to calculate the route from this place to the next one.","inputSchema":{"type":"object","properties":{"trip_id":{"type":"string"},"item_id":{"type":"string"},"confirm":{"const":true}},"required":["trip_id","item_id","confirm"]}}));
        result.push(json!({"name":"set_trip_place_note","description":"Replace a trip place note.","inputSchema":{"type":"object","properties":{"trip_id":{"type":"string"},"item_id":{"type":"string"},"note":{"type":"string"},"confirm":{"const":true}},"required":["trip_id","item_id","note","confirm"]}}));
        result.push(json!({"name":"delete_trip_place","description":"Permanently delete one place from a trip.","inputSchema":{"type":"object","properties":{"trip_id":{"type":"string"},"item_id":{"type":"string"},"confirm":{"const":true}},"required":["trip_id","item_id","confirm"]}}));
    }
    result
}

async fn call_tool(client: &FunlidayClient, enable_write: bool, params: Value) -> Result<Value> {
    let name = required_string(&params, "name")?;
    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let data = match name {
        "list_trips" => client.list_trips().await?,
        "get_trip" => client.get_trip(required_string(&args, "id")?).await?,
        "search_cities" => {
            client
                .search_cities(required_string(&args, "query")?)
                .await?
        }
        "search_places" => {
            client
                .search_places(required_string(&args, "query")?)
                .await?
        }
        "create_trip" if enable_write => {
            require_confirmation(&args)?;
            let start = required_date(&args, "start")?;
            let end = required_date(&args, "end")?;
            if end < start {
                bail!("end date cannot be before start date");
            }
            client
                .create_trip(
                    required_string(&args, "name")?,
                    &required_string_array(&args, "city_ids")?,
                    &start.to_string(),
                    &end.to_string(),
                    args.get("trip_type").and_then(Value::as_u64).unwrap_or(1) as u8,
                )
                .await?
        }
        "delete_trip" if enable_write => {
            require_confirmation(&args)?;
            client
                .delete_trip(required_string(&args, "container_id")?)
                .await?
        }
        "add_trip_place" if enable_write => {
            require_confirmation(&args)?;
            client
                .add_place(
                    required_string(&args, "trip_id")?,
                    required_u32(&args, "day")?,
                    required_string(&args, "poi_id")?,
                    required_string(&args, "name")?,
                    required_f64(&args, "latitude")?,
                    required_f64(&args, "longitude")?,
                    args.get("stay_minutes")
                        .and_then(Value::as_u64)
                        .unwrap_or(60) as u32,
                )
                .await?
        }
        "add_custom_trip_place" if enable_write => {
            require_confirmation(&args)?;
            client
                .add_custom_place(
                    required_string(&args, "trip_id")?,
                    required_u32(&args, "day")?,
                    required_string(&args, "name")?,
                    args.get("address").and_then(Value::as_str).unwrap_or(""),
                    required_f64(&args, "latitude")?,
                    required_f64(&args, "longitude")?,
                    args.get("stay_minutes")
                        .and_then(Value::as_u64)
                        .unwrap_or(60) as u32,
                )
                .await?
        }
        "set_trip_place_time" if enable_write => {
            require_confirmation(&args)?;
            client
                .update_place_time(
                    required_string(&args, "trip_id")?,
                    required_string(&args, "item_id")?,
                    required_u32(&args, "start_seconds")?,
                    required_u32(&args, "stay_minutes")?,
                )
                .await?
        }
        "set_trip_place_transport" if enable_write => {
            require_confirmation(&args)?;
            client
                .set_custom_transport(
                    required_string(&args, "trip_id")?,
                    required_string(&args, "item_id")?,
                    required_u32(&args, "duration_minutes")?,
                )
                .await?
        }
        "use_automatic_trip_place_transport" if enable_write => {
            require_confirmation(&args)?;
            client
                .use_automatic_transport(
                    required_string(&args, "trip_id")?,
                    required_string(&args, "item_id")?,
                )
                .await?
        }
        "calculate_trip_place_transport" if enable_write => {
            require_confirmation(&args)?;
            client
                .calculate_transport_route(
                    required_string(&args, "trip_id")?,
                    required_string(&args, "item_id")?,
                )
                .await?
        }
        "get_trip_place_note" => {
            client
                .get_place_note(
                    required_string(&args, "trip_id")?,
                    required_string(&args, "item_id")?,
                )
                .await?
        }
        "set_trip_place_note" if enable_write => {
            require_confirmation(&args)?;
            client
                .set_place_note(
                    required_string(&args, "trip_id")?,
                    required_string(&args, "item_id")?,
                    required_string(&args, "note")?,
                )
                .await?
        }
        "delete_trip_place" if enable_write => {
            require_confirmation(&args)?;
            client
                .delete_place(
                    required_string(&args, "trip_id")?,
                    required_string(&args, "item_id")?,
                )
                .await?
        }
        "create_trip"
        | "delete_trip"
        | "add_trip_place"
        | "add_custom_trip_place"
        | "set_trip_place_time"
        | "set_trip_place_transport"
        | "use_automatic_trip_place_transport"
        | "calculate_trip_place_transport"
        | "set_trip_place_note"
        | "delete_trip_place" => {
            bail!("write tools are disabled; restart with --enable-write")
        }
        _ => bail!("unknown tool: {name}"),
    };
    Ok(
        json!({"content":[{"type":"text","text":serde_json::to_string_pretty(&data)?}],"structuredContent":data}),
    )
}

fn required_string<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .with_context(|| format!("`{key}` is required"))
}

fn required_string_array(value: &Value, key: &str) -> Result<Vec<String>> {
    let values = value
        .get(key)
        .and_then(Value::as_array)
        .with_context(|| format!("`{key}` must be an array"))?;
    if values.is_empty() {
        bail!("`{key}` cannot be empty");
    }
    values
        .iter()
        .map(|value| {
            value
                .as_str()
                .filter(|value| !value.trim().is_empty())
                .map(str::to_owned)
                .with_context(|| format!("every `{key}` item must be a non-empty string"))
        })
        .collect()
}

fn required_date(value: &Value, key: &str) -> Result<NaiveDate> {
    NaiveDate::parse_from_str(required_string(value, key)?, "%Y-%m-%d")
        .with_context(|| format!("`{key}` must use YYYY-MM-DD"))
}

fn required_u32(value: &Value, key: &str) -> Result<u32> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .with_context(|| format!("`{key}` must be a positive integer"))
}

fn required_f64(value: &Value, key: &str) -> Result<f64> {
    value
        .get(key)
        .and_then(Value::as_f64)
        .with_context(|| format!("`{key}` must be a number"))
}

fn require_confirmation(value: &Value) -> Result<()> {
    if value.get("confirm").and_then(Value::as_bool) != Some(true) {
        bail!("write operation requires `confirm: true`");
    }
    Ok(())
}
