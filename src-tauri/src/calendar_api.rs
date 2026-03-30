use serde::{Deserialize, Serialize};
use reqwest::Client;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CalendarDateTime {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    #[serde(rename = "dateTime", skip_serializing_if = "Option::is_none")]
    pub date_time: Option<String>,
    #[serde(rename = "timeZone", skip_serializing_if = "Option::is_none")]
    pub time_zone: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CalendarEvent {
    pub id: String,
    pub summary: Option<String>,
    pub description: Option<String>,
    pub location: Option<String>,
    pub start: Option<CalendarDateTime>,
    pub end: Option<CalendarDateTime>,
    #[serde(rename = "htmlLink")]
    pub html_link: Option<String>,
    #[serde(rename = "hangoutLink")]
    pub hangout_link: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NewCalendarEvent {
    pub summary: Option<String>,
    pub description: Option<String>,
    pub location: Option<String>,
    pub start: Option<CalendarDateTime>,
    pub end: Option<CalendarDateTime>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EventsResponse {
    pub items: Vec<CalendarEvent>,
}

fn calendar_api_url(path: &str) -> String {
    #[cfg(test)]
    {
        let base = std::env::var("TEST_CALENDAR_API_BASE")
            .unwrap_or_else(|_| "https://www.googleapis.com".to_string());
        format!("{}{}", base, path)
    }
    #[cfg(not(test))]
    {
        format!("https://www.googleapis.com{}", path)
    }
}

pub async fn get_upcoming_events(access_token: &str) -> Result<Vec<CalendarEvent>, String> {
    let client = Client::new();

    let now = chrono::Utc::now().to_rfc3339();
    let url = format!(
        "{}?timeMin={}&maxResults=50&orderBy=startTime&singleEvents=true",
        calendar_api_url("/calendar/v3/calendars/primary/events"),
        urlencoding::encode(&now)
    );

    let res = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", access_token))
        .send().await.map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(format!("Failed to fetch calendar events: {}", res.status()));
    }

    let events_resp: EventsResponse = res.json().await.map_err(|e| e.to_string())?;
    Ok(events_resp.items)
}

pub async fn get_events(access_token: &str, time_min: &str, time_max: &str) -> Result<Vec<CalendarEvent>, String> {
    let client = Client::new();
    let url = format!(
        "{}?timeMin={}&timeMax={}&maxResults=250&orderBy=startTime&singleEvents=true",
        calendar_api_url("/calendar/v3/calendars/primary/events"),
        urlencoding::encode(time_min),
        urlencoding::encode(time_max)
    );

    let res = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", access_token))
        .send().await.map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(format!("Failed to fetch calendar events: {}", res.status()));
    }

    let events_resp: EventsResponse = res.json().await.map_err(|e| e.to_string())?;
    Ok(events_resp.items)
}

pub async fn create_event(access_token: &str, event: &NewCalendarEvent) -> Result<CalendarEvent, String> {
    let client = Client::new();
    let url = calendar_api_url("/calendar/v3/calendars/primary/events");

    let res = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", access_token))
        .json(event)
        .send().await.map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(format!("Failed to create event: {}", res.status()));
    }

    let created: CalendarEvent = res.json().await.map_err(|e| e.to_string())?;
    Ok(created)
}

pub async fn update_event(access_token: &str, event_id: &str, event: &NewCalendarEvent) -> Result<CalendarEvent, String> {
    let client = Client::new();
    let url = format!("{}/{}", calendar_api_url("/calendar/v3/calendars/primary/events"), urlencoding::encode(event_id));

    let res = client
        .put(&url)
        .header("Authorization", format!("Bearer {}", access_token))
        .json(event)
        .send().await.map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(format!("Failed to update event: {}", res.status()));
    }

    let updated: CalendarEvent = res.json().await.map_err(|e| e.to_string())?;
    Ok(updated)
}

pub async fn delete_event(access_token: &str, event_id: &str) -> Result<(), String> {
    let client = Client::new();
    let url = format!("{}/{}", calendar_api_url("/calendar/v3/calendars/primary/events"), urlencoding::encode(event_id));

    let res = client
        .delete(&url)
        .header("Authorization", format!("Bearer {}", access_token))
        .send().await.map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(format!("Failed to delete event: {}", res.status()));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[tokio::test]
    async fn test_get_upcoming_events_success() {
        let _lock = ENV_LOCK.lock().unwrap();
        let server = MockServer::start();
        std::env::set_var("TEST_CALENDAR_API_BASE", server.base_url());

        let mock = server.mock(|when, then| {
            when.method(GET)
                .path("/calendar/v3/calendars/primary/events");
            then.status(200)
                .header("content-type", "application/json")
                .json_body(serde_json::json!({
                    "items": [
                        {
                            "id": "evt1",
                            "summary": "Team Standup",
                            "start": {"dateTime": "2026-03-10T09:00:00Z"},
                            "end": {"dateTime": "2026-03-10T09:30:00Z"},
                            "htmlLink": "https://calendar.google.com/event?id=evt1"
                        },
                        {
                            "id": "evt2",
                            "summary": "Lunch",
                            "start": {"date": "2026-03-10"},
                            "end": {"date": "2026-03-10"}
                        }
                    ]
                }));
        });

        let events = get_upcoming_events("fake_token").await.unwrap();
        mock.assert();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].id, "evt1");
        assert_eq!(events[0].summary.as_deref(), Some("Team Standup"));
        assert!(events[0].start.as_ref().unwrap().date_time.is_some());
        assert_eq!(events[1].id, "evt2");
        assert!(events[1].start.as_ref().unwrap().date.is_some());

        std::env::remove_var("TEST_CALENDAR_API_BASE");
    }

    #[tokio::test]
    async fn test_get_upcoming_events_empty() {
        let _lock = ENV_LOCK.lock().unwrap();
        let server = MockServer::start();
        std::env::set_var("TEST_CALENDAR_API_BASE", server.base_url());

        let mock = server.mock(|when, then| {
            when.method(GET)
                .path("/calendar/v3/calendars/primary/events");
            then.status(200)
                .header("content-type", "application/json")
                .json_body(serde_json::json!({"items": []}));
        });

        let events = get_upcoming_events("fake_token").await.unwrap();
        mock.assert();
        assert!(events.is_empty());

        std::env::remove_var("TEST_CALENDAR_API_BASE");
    }

    #[tokio::test]
    async fn test_get_upcoming_events_http_error() {
        let _lock = ENV_LOCK.lock().unwrap();
        let server = MockServer::start();
        std::env::set_var("TEST_CALENDAR_API_BASE", server.base_url());

        let mock = server.mock(|when, then| {
            when.method(GET)
                .path("/calendar/v3/calendars/primary/events");
            then.status(401);
        });

        let result = get_upcoming_events("bad_token").await;
        mock.assert();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("401"));

        std::env::remove_var("TEST_CALENDAR_API_BASE");
    }
}
