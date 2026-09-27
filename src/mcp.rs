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
    ];
    if enable_write {
        result.push(json!({"name":"create_trip","description":"Create a private Funliday trip. Requires a Funliday city ID and explicit confirmation.","inputSchema":{"type":"object","properties":{"name":{"type":"string","minLength":1},"city_ids":{"type":"array","items":{"type":"string"},"minItems":1},"start":{"type":"string","format":"date"},"end":{"type":"string","format":"date"},"trip_type":{"type":"integer","minimum":1,"maximum":4,"default":1},"confirm":{"const":true}},"required":["name","city_ids","start","end","confirm"]}}));
        result.push(json!({"name":"delete_trip","description":"Permanently delete a Funliday trip using its containerId.","inputSchema":{"type":"object","properties":{"container_id":{"type":"string"},"confirm":{"const":true}},"required":["container_id","confirm"]}}));
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
        "create_trip" | "delete_trip" => {
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

fn require_confirmation(value: &Value) -> Result<()> {
    if value.get("confirm").and_then(Value::as_bool) != Some(true) {
        bail!("write operation requires `confirm: true`");
    }
    Ok(())
}
