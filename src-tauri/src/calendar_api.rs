use serde::{Deserialize, Serialize};
use reqwest::Client;

#[derive(Debug, Serialize, Deserialize)]
pub struct CalendarDateTime {
    pub date: Option<String>,
    #[serde(rename = "dateTime")]
    pub date_time: Option<String>,
    #[serde(rename = "timeZone")]
    pub time_zone: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CalendarEvent {
    pub id: String,
    pub summary: Option<String>,
    pub description: Option<String>,
    pub location: Option<String>,
    pub start: Option<CalendarDateTime>,
    pub end: Option<CalendarDateTime>,
    #[serde(rename = "htmlLink")]
    pub html_link: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EventsResponse {
    pub items: Vec<CalendarEvent>,
}

pub async fn get_upcoming_events(access_token: &str) -> Result<Vec<CalendarEvent>, String> {
    let client = Client::new();
    
    let now = chrono::Utc::now().to_rfc3339();
    let url = format!(
        "https://www.googleapis.com/calendar/v3/calendars/primary/events?timeMin={}&maxResults=50&orderBy=startTime&singleEvents=true",
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
