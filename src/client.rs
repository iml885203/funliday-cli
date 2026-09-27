use anyhow::{Context, Result, bail};
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};
use serde_json::{Value, json};

use crate::auth::Credentials;

const DEFAULT_API_BASE: &str = "https://www.funlidays.com/api/";

pub struct FunlidayClient {
    http: reqwest::Client,
    base_url: String,
    web_base_url: String,
}

impl FunlidayClient {
    pub fn new(credentials: &Credentials) -> Result<Self> {
        Self::with_base_urls(credentials, DEFAULT_API_BASE, "https://www.funliday.com/")
    }

    fn with_base_urls(
        credentials: &Credentials,
        base_url: &str,
        web_base_url: &str,
    ) -> Result<Self> {
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
        let member_json = serde_json::to_string(&credentials.member_id)?;
        let token_json = json!({"token": credentials.access_token}).to_string();
        let member_cookie = urlencoding::encode(&member_json);
        let token_cookie = urlencoding::encode(&token_json);
        headers.insert(
            "cookie",
            HeaderValue::from_str(&format!(
                "fld-memberId={member_cookie}; fld-accessToken={token_cookie}"
            ))?,
        );
        let http = reqwest::Client::builder()
            .default_headers(headers)
            .build()?;
        Ok(Self {
            http,
            base_url: format!("{}/", base_url.trim_end_matches('/')),
            web_base_url: format!("{}/", web_base_url.trim_end_matches('/')),
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

    pub async fn search_cities(&self, query: &str) -> Result<Value> {
        let response = self
            .http
            .get(format!("{}api/next/autocomplete", self.web_base_url))
            .query(&[("keyword", query), ("type", "city")])
            .send()
            .await
            .context("Funliday city search failed")?;
        let status = response.status();
        let value: Value = response
            .json()
            .await
            .context("invalid city search response")?;
        if !status.is_success() || !value.is_array() {
            bail!("Funliday city search failed: HTTP {status}, response {value}");
        }
        Ok(value)
    }

    pub async fn search_places(&self, query: &str) -> Result<Value> {
        let response = self
            .http
            .get(format!("{}api/next/autocomplete", self.web_base_url))
            .query(&[("keyword", query), ("type", "place")])
            .send()
            .await
            .context("Funliday place search failed")?;
        let status = response.status();
        let value: Value = response
            .json()
            .await
            .context("invalid place search response")?;
        if !status.is_success() || !value.is_array() {
            bail!("Funliday place search failed: HTTP {status}, response {value}");
        }
        Ok(value)
    }

    pub async fn add_place(
        &self,
        trip_id: &str,
        day: u32,
        poi_id: &str,
        name: &str,
        latitude: f64,
        longitude: f64,
        stay_minutes: u32,
    ) -> Result<Value> {
        let before = self.get_trip(trip_id).await?;
        let itinerary = &before["itinerary"];
        let day_count = string_u32(itinerary, "dayCount")?;
        if day == 0 || day > day_count {
            bail!("day must be between 1 and {day_count}");
        }
        let revision = required_string_value(itinerary, "revision")?;
        let start = required_string_value(itinerary, "startDate")?
            .parse::<u64>()
            .context("invalid trip startDate")?;
        let previous_matches = matching_place_count(&before, day, poi_id);
        let action_at = start + u64::from(day - 1) * 86_400;
        let body = json!({
            "parseTripObjectId":trip_id,
            "daySequence":day.to_string(),
            "revision":revision,
            "transportationType":"4",
            "addToCollections":"0",
            "name":name,
            "location":{"lat":latitude,"lng":longitude},
            "dataSource":"3",
            "infoForPoiBank":{"language":"zh_tw","data":[{"id":poi_id,"actionAt":action_at.to_string()}]},
            "poiBankNextId":poi_id,
            "stayTime":(stay_minutes * 60).to_string(),
            "deviceId":uuid::Uuid::new_v4().to_string()
        });
        let response = self
            .http
            .post(format!("{}addPoi", self.base_url))
            .json(&body)
            .send()
            .await
            .context("Funliday addPoi request failed")?;
        let http_status = response.status();
        let value: Value = response.json().await.unwrap_or_else(|_| json!({}));
        if http_status.is_success() && value.get("status").and_then(Value::as_str) == Some("200") {
            return Ok(value);
        }
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
        let after = self.get_trip(trip_id).await?;
        if matching_place_count(&after, day, poi_id) > previous_matches {
            return Ok(json!({
                "status":"200",
                "reconciled":true,
                "message":"Funliday committed the place although addPoi returned an error",
                "trip":after
            }));
        }
        bail!("Funliday `addPoi` failed: HTTP {http_status}, response {value}")
    }

    pub async fn add_custom_place(
        &self,
        trip_id: &str,
        day: u32,
        name: &str,
        address: &str,
        latitude: f64,
        longitude: f64,
        stay_minutes: u32,
    ) -> Result<Value> {
        let before = self.get_trip(trip_id).await?;
        let itinerary = &before["itinerary"];
        let day_count = string_u32(itinerary, "dayCount")?;
        if day == 0 || day > day_count {
            bail!("day must be between 1 and {day_count}");
        }
        let revision = required_string_value(itinerary, "revision")?;
        let start = required_string_value(itinerary, "startDate")?
            .parse::<u64>()
            .context("invalid trip startDate")?;
        let previous_matches = matching_custom_place_count(&before, day, name);
        let action_at = start + u64::from(day - 1) * 86_400;
        let body = json!({
            "parseTripObjectId":trip_id,
            "daySequence":day.to_string(),
            "revision":revision,
            "transportationType":"4",
            "addToCollections":"0",
            "name":name,
            "address":address,
            "location":{"lat":latitude,"lng":longitude},
            "dataSource":"4",
            "infoForPoiBank":{
                "language":"zh_tw",
                "name":name,
                "data":[{"id":address,"actionAt":action_at.to_string()}]
            },
            "stayTime":(stay_minutes * 60).to_string(),
            "deviceId":uuid::Uuid::new_v4().to_string()
        });
        let response = self
            .http
            .post(format!("{}addPoi", self.base_url))
            .json(&body)
            .send()
            .await
            .context("Funliday addPoi request failed")?;
        let http_status = response.status();
        let value: Value = response.json().await.unwrap_or_else(|_| json!({}));
        if http_status.is_success() && value.get("status").and_then(Value::as_str) == Some("200") {
            return Ok(value);
        }
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
        let after = self.get_trip(trip_id).await?;
        if matching_custom_place_count(&after, day, name) > previous_matches {
            return Ok(json!({
                "status":"200",
                "reconciled":true,
                "message":"Funliday committed the custom place although addPoi returned an error",
                "trip":after
            }));
        }
        bail!("Funliday custom `addPoi` failed: HTTP {http_status}, response {value}")
    }

    pub async fn update_place_time(
        &self,
        trip_id: &str,
        item_id: &str,
        start_seconds: u32,
        stay_minutes: u32,
    ) -> Result<Value> {
        let trip = self.get_trip(trip_id).await?;
        ensure_item(&trip, item_id)?;
        let revision = required_string_value(&trip["itinerary"], "revision")?;
        self.post(
            "updatePoiStartTime",
            &json!({
                "parseTripObjectId":trip_id,
                "parsePoiObjectId":item_id,
                "revision":revision,
                "customizeStartTime":start_seconds.to_string(),
                "stayTime":(stay_minutes * 60).to_string(),
                "deviceId":uuid::Uuid::new_v4().to_string()
            }),
        )
        .await
    }

    pub async fn set_custom_transport(
        &self,
        trip_id: &str,
        item_id: &str,
        duration_minutes: u32,
    ) -> Result<Value> {
        let trip = self.get_trip(trip_id).await?;
        ensure_item(&trip, item_id)?;
        let revision = required_string_value(&trip["itinerary"], "revision")?;
        self.post(
            "customizeTransportationTime",
            &json!({
                "parseTripObjectId":trip_id,
                "parsePoiObjectId":item_id,
                "customizeTransportationTimeFlag":"1",
                "customizeTransportationTime":(duration_minutes * 60).to_string(),
                "revision":revision,
                "deviceId":uuid::Uuid::new_v4().to_string()
            }),
        )
        .await
    }

    pub async fn set_place_note(&self, trip_id: &str, item_id: &str, note: &str) -> Result<Value> {
        let trip = self.get_trip(trip_id).await?;
        ensure_item(&trip, item_id)?;
        self.post(
            "postTextNote",
            &json!({
                "parseTripObjectId":trip_id,
                "parsePoiObjectId":item_id,
                "textNote":note,
                "deviceId":uuid::Uuid::new_v4().to_string()
            }),
        )
        .await
    }

    pub async fn get_place_note(&self, trip_id: &str, item_id: &str) -> Result<Value> {
        self.post(
            "getTextNote",
            &json!({"parseTripObjectId":trip_id,"parsePoiObjectId":item_id}),
        )
        .await
    }

    pub async fn delete_place(&self, trip_id: &str, item_id: &str) -> Result<Value> {
        let trip = self.get_trip(trip_id).await?;
        ensure_item(&trip, item_id)?;
        let revision = required_string_value(&trip["itinerary"], "revision")?;
        self.post(
            "deletePois",
            &json!({
                "parseTripObjectId":trip_id,
                "idArray":[item_id],
                "revision":revision,
                "deviceId":uuid::Uuid::new_v4().to_string()
            }),
        )
        .await
    }

    pub async fn create_trip(
        &self,
        name: &str,
        city_ids: &[String],
        start: &str,
        end: &str,
        trip_type: u8,
    ) -> Result<Value> {
        let cities = serde_json::to_string(city_ids)?;
        self.next_request(
            reqwest::Method::POST,
            "api/next/containers",
            Some(&[
                ("name", name),
                ("userCities", &cities),
                ("dateStart", &start.replace('-', "")),
                ("dateEnd", &end.replace('-', "")),
                ("tripType", &trip_type.to_string()),
            ]),
        )
        .await
    }

    pub async fn delete_trip(&self, container_id: &str) -> Result<Value> {
        self.next_request(
            reqwest::Method::DELETE,
            &format!("api/next/containers/{container_id}"),
            None,
        )
        .await
    }

    async fn next_request(
        &self,
        method: reqwest::Method,
        path: &str,
        form: Option<&[(&str, &str)]>,
    ) -> Result<Value> {
        let mut request = self
            .http
            .request(method, format!("{}{}", self.web_base_url, path));
        if let Some(form) = form {
            request = request.form(form);
        }
        let response = request.send().await?;
        let status = response.status();
        let value: Value = response
            .json()
            .await
            .context("Funliday returned invalid JSON")?;
        if !status.is_success() || value.get("success").and_then(Value::as_bool) != Some(true) {
            bail!("Funliday `{path}` failed: HTTP {status}, response {value}");
        }
        Ok(value)
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

fn required_string_value<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .with_context(|| format!("Funliday response is missing `{key}`"))
}

fn string_u32(value: &Value, key: &str) -> Result<u32> {
    required_string_value(value, key)?
        .parse()
        .with_context(|| format!("Funliday `{key}` is not a number"))
}

fn matching_place_count(trip: &Value, day: u32, poi_id: &str) -> usize {
    trip.pointer("/itinerary/pois")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter(|item| {
                    item.get("daySequence").and_then(Value::as_u64) == Some(u64::from(day))
                        && item.get("poiBankNextId").and_then(Value::as_str) == Some(poi_id)
                })
                .count()
        })
        .unwrap_or(0)
}

fn matching_custom_place_count(trip: &Value, day: u32, name: &str) -> usize {
    trip.pointer("/itinerary/pois")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter(|item| {
                    item.get("daySequence").and_then(Value::as_u64) == Some(u64::from(day))
                        && item.get("name").and_then(Value::as_str) == Some(name)
                })
                .count()
        })
        .unwrap_or(0)
}

fn ensure_item<'a>(trip: &'a Value, item_id: &str) -> Result<&'a Value> {
    trip.pointer("/itinerary/pois")
        .and_then(Value::as_array)
        .and_then(|items| {
            items
                .iter()
                .find(|item| item.get("_id").and_then(Value::as_str) == Some(item_id))
        })
        .with_context(|| format!("item `{item_id}` was not found in this trip"))
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
        let client = FunlidayClient::with_base_urls(
            &credentials(),
            &format!("{}/api", server.base_url()),
            &server.base_url(),
        )
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
        let client = FunlidayClient::with_base_urls(
            &credentials(),
            &format!("{}/api", server.base_url()),
            &server.base_url(),
        )
        .unwrap();
        let result = client.get_trip("trip-1").await.unwrap();
        assert_eq!(result["trip"]["tripName"], "Test");
        assert_eq!(result["itinerary"]["days"], json!([]));
    }

    #[tokio::test]
    async fn creates_trip_with_next_api_cookie_auth() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/api/next/containers")
                .header_exists("cookie");
            then.status(200)
                .json_body(json!({"success":true,"data":{"id":"container-1"}}));
        });
        let client = FunlidayClient::with_base_urls(
            &credentials(),
            &format!("{}/api", server.base_url()),
            &server.base_url(),
        )
        .unwrap();
        let result = client
            .create_trip("Test", &["7868657".into()], "2026-10-20", "2026-10-21", 1)
            .await
            .unwrap();
        assert_eq!(result["data"]["id"], "container-1");
        mock.assert();
    }

    #[tokio::test]
    async fn deletes_container_with_next_api() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(DELETE).path("/api/next/containers/c1");
            then.status(200).json_body(json!({"success":true}));
        });
        let client = FunlidayClient::with_base_urls(
            &credentials(),
            &format!("{}/api", server.base_url()),
            &server.base_url(),
        )
        .unwrap();
        client.delete_trip("c1").await.unwrap();
        mock.assert();
    }

    #[test]
    fn counts_custom_places_by_day_and_name() {
        let trip = json!({"itinerary":{"pois":[
            {"daySequence":1,"name":"Hotel"},
            {"daySequence":2,"name":"Hotel"},
            {"daySequence":2,"name":"Station"}
        ]}});
        assert_eq!(matching_custom_place_count(&trip, 1, "Hotel"), 1);
        assert_eq!(matching_custom_place_count(&trip, 2, "Hotel"), 1);
        assert_eq!(matching_custom_place_count(&trip, 2, "Missing"), 0);
    }
}
