use anyhow::{Context, Result, bail};
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};
use serde_json::{Value, json};

use crate::auth::Credentials;

const DEFAULT_API_BASE: &str = "https://www.funlidays.com/api/";

pub struct FunlidayClient {
    http: reqwest::Client,
    base_url: String,
}

impl FunlidayClient {
    pub fn new(credentials: &Credentials) -> Result<Self> {
        Self::with_base_url(credentials, DEFAULT_API_BASE)
    }

    fn with_base_url(credentials: &Credentials, base_url: &str) -> Result<Self> {
        let mut headers = HeaderMap::new();
        let authorization = format!(
            "Bearer {}_{}",
            credentials.member_id, credentials.access_token
        );
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&authorization).context("invalid credentials")?,
        );
        headers.insert("x-funliday-langapp", HeaderValue::from_static("zh-tw"));
        headers.insert(
            "x-funliday-timezone",
            HeaderValue::from_static("Asia/Taipei"),
        );
        let http = reqwest::Client::builder()
            .default_headers(headers)
            .build()?;
        Ok(Self {
            http,
            base_url: format!("{}/", base_url.trim_end_matches('/')),
        })
    }

    pub async fn list_trips(&self) -> Result<Value> {
        self.post(
            "getTripList",
            &json!({"deviceId":"funliday-cli","skip":"0","limit":"100"}),
        )
        .await
    }

    pub async fn get_trip(&self, trip_id: &str) -> Result<Value> {
        let list = self.list_trips().await?;
        let trip = list
            .pointer("/results/trips")
            .and_then(Value::as_array)
            .and_then(|trips| {
                trips
                    .iter()
                    .find(|trip| trip.get("_id").and_then(Value::as_str) == Some(trip_id))
            })
            .cloned()
            .with_context(|| format!("trip `{trip_id}` was not found"))?;
        let itinerary = self
            .post(
                "getPoisOfTrip",
                &json!({"deviceId":"funliday-cli","parseTripObjectId":trip_id}),
            )
            .await?;
        Ok(json!({"trip":trip,"itinerary":itinerary["results"]}))
    }

    async fn post(&self, path: &str, body: &Value) -> Result<Value> {
        let response = self
            .http
            .post(format!("{}{}", self.base_url, path))
            .json(body)
            .send()
            .await
            .with_context(|| format!("Funliday request `{path}` failed"))?;
        let status = response.status();
        let value: Value = response
            .json()
            .await
            .with_context(|| format!("Funliday returned invalid JSON for `{path}`"))?;
        if !status.is_success() || value.get("status").and_then(Value::as_str) != Some("200") {
            bail!("Funliday `{path}` failed: HTTP {status}, response {value}");
        }
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use httpmock::prelude::*;

    use super::*;

    fn credentials() -> Credentials {
        Credentials {
            member_id: "member".into(),
            access_token: "token".into(),
        }
    }

    #[tokio::test]
    async fn lists_trips_with_funliday_authentication() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/api/getTripList")
                .header("authorization", "Bearer member_token")
                .json_body(json!({
                    "deviceId":"funliday-cli","skip":"0","limit":"100"
                }));
            then.status(200)
                .json_body(json!({"status":"200","results":{"trips":[]}}));
        });
        let client =
            FunlidayClient::with_base_url(&credentials(), &format!("{}/api", server.base_url()))
                .unwrap();
        let result = client.list_trips().await.unwrap();
        assert_eq!(result["results"]["trips"], json!([]));
        mock.assert();
    }

    #[tokio::test]
    async fn combines_trip_metadata_and_itinerary() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST).path("/api/getTripList");
            then.status(200).json_body(json!({
                "status":"200","results":{"trips":[{"_id":"trip-1","tripName":"Test"}]}
            }));
        });
        server.mock(|when, then| {
            when.method(POST)
                .path("/api/getPoisOfTrip")
                .json_body(json!({
                    "deviceId":"funliday-cli","parseTripObjectId":"trip-1"
                }));
            then.status(200).json_body(json!({
                "status":"200","results":{"days":[]}
            }));
        });
        let client =
            FunlidayClient::with_base_url(&credentials(), &format!("{}/api", server.base_url()))
                .unwrap();
        let result = client.get_trip("trip-1").await.unwrap();
        assert_eq!(result["trip"]["tripName"], "Test");
        assert_eq!(result["itinerary"]["days"], json!([]));
    }
}
