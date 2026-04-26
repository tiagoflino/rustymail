use crate::calendar_api::{CalendarDateTime, CalendarEvent, NewCalendarEvent};
use reqwest::Client;

fn caldav_api_url(base: &str, path: &str) -> String {
    #[cfg(test)]
    {
        let base_override = std::env::var("TEST_CALDAV_BASE").unwrap_or_else(|_| base.to_string());
        format!("{}{}", base_override, path)
    }
    #[cfg(not(test))]
    {
        format!("{}{}", base, path)
    }
}

pub fn format_caldav_datetime(iso: &str) -> String {
    iso.replace(['-', ':'], "")
}

fn build_ical_event(uid: &str, event: &NewCalendarEvent) -> String {
    let mut lines = vec![
        "BEGIN:VCALENDAR".to_string(),
        "VERSION:2.0".to_string(),
        "PRODID:-//Rustymail//CalDAV//EN".to_string(),
        "BEGIN:VEVENT".to_string(),
        format!("UID:{}", uid),
    ];

    if let Some(ref summary) = event.summary {
        lines.push(format!("SUMMARY:{}", summary));
    }
    if let Some(ref description) = event.description {
        lines.push(format!("DESCRIPTION:{}", description));
    }
    if let Some(ref location) = event.location {
        lines.push(format!("LOCATION:{}", location));
    }

    if let Some(ref start) = event.start {
        if let Some(ref dt) = start.date_time {
            lines.push(format!("DTSTART:{}", format_caldav_datetime(dt)));
        } else if let Some(ref d) = start.date {
            lines.push(format!("DTSTART;VALUE=DATE:{}", d.replace('-', "")));
        }
    }

    if let Some(ref end) = event.end {
        if let Some(ref dt) = end.date_time {
            lines.push(format!("DTEND:{}", format_caldav_datetime(dt)));
        } else if let Some(ref d) = end.date {
            lines.push(format!("DTEND;VALUE=DATE:{}", d.replace('-', "")));
        }
    }

    lines.push("END:VEVENT".to_string());
    lines.push("END:VCALENDAR".to_string());
    lines.join("\r\n")
}

pub fn parse_ical_event(ical_data: &str, resource_url: &str) -> Option<CalendarEvent> {
    let calendars = icalendar::parser::unfold(ical_data);
    let parsed = icalendar::parser::read_calendar(&calendars).ok()?;

    for component in &parsed.components {
        if component.name.as_str() != "VEVENT" {
            continue;
        }

        let mut uid = String::new();
        let mut summary = None;
        let mut description = None;
        let mut location = None;
        let mut dtstart = None;
        let mut dtend = None;

        for prop in &component.properties {
            match prop.name.as_str() {
                "UID" => uid = prop.val.as_str().to_string(),
                "SUMMARY" => summary = Some(prop.val.as_str().to_string()),
                "DESCRIPTION" => description = Some(prop.val.as_str().to_string()),
                "LOCATION" => location = Some(prop.val.as_str().to_string()),
                "DTSTART" => {
                    dtstart = Some(parse_ical_datetime_prop(prop));
                }
                "DTEND" => {
                    dtend = Some(parse_ical_datetime_prop(prop));
                }
                _ => {}
            }
        }

        let id = if resource_url.is_empty() { uid } else { resource_url.to_string() };

        return Some(CalendarEvent {
            id,
            summary,
            description,
            location,
            start: dtstart,
            end: dtend,
            html_link: None,
            hangout_link: None,
        });
    }

    None
}

fn parse_ical_datetime_prop(prop: &icalendar::parser::Property) -> CalendarDateTime {
    let val = prop.val.as_str();
    let has_value_date = prop.params.iter().any(|p| {
        p.key.as_str() == "VALUE" && p.val.as_ref().map(|v| v.as_str()) == Some("DATE")
    });

    if has_value_date || (val.len() == 8 && val.chars().all(|c| c.is_ascii_digit())) {
        let date_str = if val.len() == 8 {
            format!("{}-{}-{}", &val[0..4], &val[4..6], &val[6..8])
        } else {
            val.to_string()
        };
        CalendarDateTime {
            date: Some(date_str),
            date_time: None,
            time_zone: None,
        }
    } else {
        let dt_str = ical_to_iso(val);
        let tz = prop.params.iter().find(|p| p.key.as_str() == "TZID").and_then(|p| p.val.as_ref().map(|v| v.as_str().to_string()));
        CalendarDateTime {
            date: None,
            date_time: Some(dt_str),
            time_zone: tz,
        }
    }
}

fn ical_to_iso(val: &str) -> String {
    if val.len() >= 15 {
        let base = &val[..15];
        let suffix = if val.ends_with('Z') { "Z" } else { "" };
        format!(
            "{}-{}-{}T{}:{}:{}{}",
            &base[0..4],
            &base[4..6],
            &base[6..8],
            &base[9..11],
            &base[11..13],
            &base[13..15],
            suffix
        )
    } else {
        val.to_string()
    }
}

pub fn parse_multistatus_events(xml: &str) -> Vec<(String, String)> {
    use quick_xml::events::Event;
    use quick_xml::reader::Reader;

    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut results: Vec<(String, String)> = Vec::new();
    let mut buf = Vec::new();
    let mut inside_response = false;
    let mut current_href = String::new();
    let mut current_cal_data = String::new();
    let mut current_tag = String::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name_bytes = e.name().as_ref().to_vec();
                let local = local_name(&name_bytes);
                match local.as_str() {
                    "response" => {
                        inside_response = true;
                        current_href.clear();
                        current_cal_data.clear();
                    }
                    "href" | "calendar-data" if inside_response => {
                        current_tag = local;
                    }
                    _ => {
                        current_tag.clear();
                    }
                }
            }
            Ok(Event::Text(e)) => {
                if inside_response {
                    let text = e.unescape().unwrap_or_default().to_string();
                    match current_tag.as_str() {
                        "href" => current_href = text,
                        "calendar-data" => current_cal_data = text,
                        _ => {}
                    }
                }
            }
            Ok(Event::End(e)) => {
                let name_bytes = e.name().as_ref().to_vec();
                let local = local_name(&name_bytes);
                if local == "response" && inside_response {
                    if !current_href.is_empty() && !current_cal_data.is_empty() {
                        results.push((current_href.clone(), current_cal_data.clone()));
                    }
                    inside_response = false;
                }
                current_tag.clear();
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    results
}

fn local_name(full: &[u8]) -> String {
    let s = std::str::from_utf8(full).unwrap_or("");
    if let Some(pos) = s.rfind(':') {
        s[pos + 1..].to_string()
    } else {
        s.to_string()
    }
}

fn parse_propfind_property(xml: &str, property: &str) -> Option<String> {
    use quick_xml::events::Event;
    use quick_xml::reader::Reader;

    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut buf = Vec::new();
    let mut capture_href = false;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name_bytes = e.name().as_ref().to_vec();
                let local = local_name(&name_bytes);
                if local == property {
                    capture_href = true;
                } else if capture_href && local == "href" {
                } else if capture_href && local != "href" {
                    capture_href = false;
                }
            }
            Ok(Event::Text(e)) => {
                if capture_href {
                    let text = e.unescape().unwrap_or_default().to_string();
                    if !text.is_empty() {
                        return Some(text);
                    }
                }
            }
            Ok(Event::End(e)) => {
                let name_bytes = e.name().as_ref().to_vec();
                let local = local_name(&name_bytes);
                if local == property {
                    capture_href = false;
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    None
}

pub async fn discover_caldav_url(
    host_domain: &str,
    username: &str,
    password: &str,
) -> Result<String, String> {
    let client = Client::builder()
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .map_err(|e| e.to_string())?;

    let well_known_url = caldav_api_url(
        &format!("https://{}", host_domain),
        "/.well-known/caldav",
    );

    let resp = client
        .get(&well_known_url)
        .basic_auth(username, Some(password))
        .send()
        .await
        .map_err(|e| format!("CalDAV discovery failed: {}", e))?;

    let base_url = resp.url().to_string();
    let base_origin = if let Ok(url) = reqwest::Url::parse(&base_url) {
        format!("{}://{}", url.scheme(), url.host_str().unwrap_or(host_domain))
    } else {
        format!("https://{}", host_domain)
    };

    let propfind_body = r#"<?xml version="1.0"?>
<d:propfind xmlns:d="DAV:">
  <d:prop><d:current-user-principal/></d:prop>
</d:propfind>"#;

    let principal_resp = client
        .request(
            reqwest::Method::from_bytes(b"PROPFIND").unwrap(),
            &base_url,
        )
        .basic_auth(username, Some(password))
        .header("Depth", "0")
        .header("Content-Type", "application/xml")
        .body(propfind_body.to_string())
        .send()
        .await
        .map_err(|e| format!("PROPFIND principal failed: {}", e))?;

    let principal_xml = principal_resp.text().await.map_err(|e| e.to_string())?;

    let principal_href = parse_propfind_property(&principal_xml, "current-user-principal")
        .ok_or_else(|| "Could not find current-user-principal".to_string())?;

    let principal_url = if principal_href.starts_with("http") {
        principal_href.clone()
    } else {
        format!("{}{}", base_origin, principal_href)
    };

    let home_body = r#"<?xml version="1.0"?>
<d:propfind xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav">
  <d:prop><c:calendar-home-set/></d:prop>
</d:propfind>"#;

    let home_resp = client
        .request(
            reqwest::Method::from_bytes(b"PROPFIND").unwrap(),
            &principal_url,
        )
        .basic_auth(username, Some(password))
        .header("Depth", "0")
        .header("Content-Type", "application/xml")
        .body(home_body.to_string())
        .send()
        .await
        .map_err(|e| format!("PROPFIND calendar-home failed: {}", e))?;

    let home_xml = home_resp.text().await.map_err(|e| e.to_string())?;

    let home_href = parse_propfind_property(&home_xml, "calendar-home-set")
        .ok_or_else(|| "Could not find calendar-home-set".to_string())?;

    if home_href.starts_with("http") {
        Ok(home_href)
    } else {
        Ok(format!("{}{}", base_origin, home_href))
    }
}

pub async fn caldav_get_events(
    caldav_url: &str,
    username: &str,
    password: &str,
    time_min: &str,
    time_max: &str,
) -> Result<Vec<CalendarEvent>, String> {
    let client = Client::new();

    let start_fmt = format_caldav_datetime(time_min);
    let end_fmt = format_caldav_datetime(time_max);

    let report_body = format!(
        r#"<?xml version="1.0"?>
<c:calendar-query xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav">
  <d:prop><d:getetag/><c:calendar-data/></d:prop>
  <c:filter>
    <c:comp-filter name="VCALENDAR">
      <c:comp-filter name="VEVENT">
        <c:time-range start="{}" end="{}"/>
      </c:comp-filter>
    </c:comp-filter>
  </c:filter>
</c:calendar-query>"#,
        start_fmt, end_fmt
    );

    let url = caldav_api_url(caldav_url, "");

    let resp = client
        .request(reqwest::Method::from_bytes(b"REPORT").unwrap(), &url)
        .basic_auth(username, Some(password))
        .header("Depth", "1")
        .header("Content-Type", "application/xml")
        .body(report_body)
        .send()
        .await
        .map_err(|e| format!("CalDAV REPORT failed: {}", e))?;

    if !resp.status().is_success() && resp.status().as_u16() != 207 {
        return Err(format!("CalDAV REPORT returned {}", resp.status()));
    }

    let xml = resp.text().await.map_err(|e| e.to_string())?;
    let pairs = parse_multistatus_events(&xml);

    let mut events = Vec::new();
    for (href, cal_data) in pairs {
        if let Some(event) = parse_ical_event(&cal_data, &href) {
            events.push(event);
        }
    }

    Ok(events)
}

pub async fn caldav_create_event(
    caldav_url: &str,
    username: &str,
    password: &str,
    event: &NewCalendarEvent,
) -> Result<CalendarEvent, String> {
    let client = Client::new();
    let uid = uuid::Uuid::new_v4().to_string();
    let ical = build_ical_event(&uid, event);

    let event_url = caldav_api_url(caldav_url, &format!("/{}.ics", uid));

    let resp = client
        .put(&event_url)
        .basic_auth(username, Some(password))
        .header("Content-Type", "text/calendar")
        .header("If-None-Match", "*")
        .body(ical)
        .send()
        .await
        .map_err(|e| format!("CalDAV PUT failed: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("CalDAV create event failed: {}", resp.status()));
    }

    Ok(CalendarEvent {
        id: event_url,
        summary: event.summary.clone(),
        description: event.description.clone(),
        location: event.location.clone(),
        start: event.start.clone(),
        end: event.end.clone(),
        html_link: None,
        hangout_link: None,
    })
}

pub async fn caldav_update_event(
    caldav_url: &str,
    username: &str,
    password: &str,
    event_id: &str,
    event: &NewCalendarEvent,
) -> Result<CalendarEvent, String> {
    let client = Client::new();

    let uid = event_id
        .rsplit('/')
        .next()
        .unwrap_or(event_id)
        .trim_end_matches(".ics");
    let ical = build_ical_event(uid, event);

    let url = caldav_api_url(caldav_url, "");
    let full_url = if event_id.starts_with("http") {
        event_id.to_string()
    } else {
        format!("{}{}", url, event_id)
    };

    let resp = client
        .put(&full_url)
        .basic_auth(username, Some(password))
        .header("Content-Type", "text/calendar")
        .body(ical)
        .send()
        .await
        .map_err(|e| format!("CalDAV PUT update failed: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("CalDAV update event failed: {}", resp.status()));
    }

    Ok(CalendarEvent {
        id: event_id.to_string(),
        summary: event.summary.clone(),
        description: event.description.clone(),
        location: event.location.clone(),
        start: event.start.clone(),
        end: event.end.clone(),
        html_link: None,
        hangout_link: None,
    })
}

pub async fn caldav_delete_event(
    caldav_url: &str,
    username: &str,
    password: &str,
    event_id: &str,
) -> Result<(), String> {
    let client = Client::new();

    let url = caldav_api_url(caldav_url, "");
    let full_url = if event_id.starts_with("http") {
        event_id.to_string()
    } else {
        format!("{}{}", url, event_id)
    };

    let resp = client
        .delete(&full_url)
        .basic_auth(username, Some(password))
        .send()
        .await
        .map_err(|e| format!("CalDAV DELETE failed: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("CalDAV delete event failed: {}", resp.status()));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn test_format_caldav_datetime() {
        assert_eq!(
            format_caldav_datetime("2026-04-25T09:00:00Z"),
            "20260425T090000Z"
        );
        assert_eq!(
            format_caldav_datetime("2026-04-25T00:00:00Z"),
            "20260425T000000Z"
        );
        assert_eq!(
            format_caldav_datetime("2026-12-31T23:59:59Z"),
            "20261231T235959Z"
        );
    }

    #[test]
    fn test_build_ical_event() {
        let event = NewCalendarEvent {
            summary: Some("Team Meeting".to_string()),
            description: Some("Weekly sync".to_string()),
            location: Some("Room 1".to_string()),
            start: Some(CalendarDateTime {
                date: None,
                date_time: Some("2026-04-15T09:00:00Z".to_string()),
                time_zone: None,
            }),
            end: Some(CalendarDateTime {
                date: None,
                date_time: Some("2026-04-15T10:00:00Z".to_string()),
                time_zone: None,
            }),
        };

        let ical = build_ical_event("test-uid-123", &event);
        assert!(ical.contains("BEGIN:VCALENDAR"));
        assert!(ical.contains("BEGIN:VEVENT"));
        assert!(ical.contains("UID:test-uid-123"));
        assert!(ical.contains("SUMMARY:Team Meeting"));
        assert!(ical.contains("DESCRIPTION:Weekly sync"));
        assert!(ical.contains("LOCATION:Room 1"));
        assert!(ical.contains("DTSTART:20260415T090000Z"));
        assert!(ical.contains("DTEND:20260415T100000Z"));
        assert!(ical.contains("END:VEVENT"));
        assert!(ical.contains("END:VCALENDAR"));
    }

    #[test]
    fn test_build_ical_event_all_day() {
        let event = NewCalendarEvent {
            summary: Some("Vacation".to_string()),
            description: None,
            location: None,
            start: Some(CalendarDateTime {
                date: Some("2026-04-15".to_string()),
                date_time: None,
                time_zone: None,
            }),
            end: Some(CalendarDateTime {
                date: Some("2026-04-16".to_string()),
                date_time: None,
                time_zone: None,
            }),
        };

        let ical = build_ical_event("allday-uid", &event);
        assert!(ical.contains("DTSTART;VALUE=DATE:20260415"));
        assert!(ical.contains("DTEND;VALUE=DATE:20260416"));
        assert!(!ical.contains("DTSTART:20260415T"));
    }

    #[test]
    fn test_parse_ical_event() {
        let ical = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:event1@example.com\r\nDTSTART:20260415T090000Z\r\nDTEND:20260415T100000Z\r\nSUMMARY:Team Meeting\r\nLOCATION:Room 1\r\nDESCRIPTION:Weekly sync\r\nEND:VEVENT\r\nEND:VCALENDAR";

        let event = parse_ical_event(ical, "/calendars/user/default/event1.ics").unwrap();
        assert_eq!(event.id, "/calendars/user/default/event1.ics");
        assert_eq!(event.summary.as_deref(), Some("Team Meeting"));
        assert_eq!(event.description.as_deref(), Some("Weekly sync"));
        assert_eq!(event.location.as_deref(), Some("Room 1"));
        let start = event.start.unwrap();
        assert!(start.date_time.is_some());
        assert_eq!(start.date_time.unwrap(), "2026-04-15T09:00:00Z");
        let end = event.end.unwrap();
        assert_eq!(end.date_time.unwrap(), "2026-04-15T10:00:00Z");
    }

    #[test]
    fn test_parse_ical_event_all_day() {
        let ical = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:allday@example.com\r\nDTSTART;VALUE=DATE:20260415\r\nDTEND;VALUE=DATE:20260416\r\nSUMMARY:All Day Event\r\nEND:VEVENT\r\nEND:VCALENDAR";

        let event = parse_ical_event(ical, "").unwrap();
        assert_eq!(event.id, "allday@example.com");
        let start = event.start.unwrap();
        assert!(start.date.is_some());
        assert_eq!(start.date.unwrap(), "2026-04-15");
        assert!(start.date_time.is_none());
    }

    #[test]
    fn test_parse_multistatus_events() {
        let xml = r#"<?xml version="1.0"?>
<d:multistatus xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav">
  <d:response>
    <d:href>/calendars/user/default/event1.ics</d:href>
    <d:propstat>
      <d:prop>
        <d:getetag>"etag123"</d:getetag>
        <c:calendar-data>BEGIN:VCALENDAR
VERSION:2.0
BEGIN:VEVENT
UID:event1@example.com
DTSTART:20260415T090000Z
DTEND:20260415T100000Z
SUMMARY:Team Meeting
END:VEVENT
END:VCALENDAR</c:calendar-data>
      </d:prop>
      <d:status>HTTP/1.1 200 OK</d:status>
    </d:propstat>
  </d:response>
</d:multistatus>"#;

        let pairs = parse_multistatus_events(xml);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].0, "/calendars/user/default/event1.ics");
        assert!(pairs[0].1.contains("VEVENT"));
        assert!(pairs[0].1.contains("Team Meeting"));
    }

    #[test]
    fn test_parse_multistatus_empty() {
        let xml = r#"<?xml version="1.0"?>
<d:multistatus xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav">
</d:multistatus>"#;

        let pairs = parse_multistatus_events(xml);
        assert!(pairs.is_empty());
    }

    #[tokio::test]
    async fn test_caldav_get_events_via_mock() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let server = MockServer::start();
        std::env::set_var("TEST_CALDAV_BASE", server.base_url());

        let cal_data = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:mock1@test.com\r\nDTSTART:20260415T090000Z\r\nDTEND:20260415T100000Z\r\nSUMMARY:Mock Event\r\nEND:VEVENT\r\nEND:VCALENDAR";

        let response_xml = format!(
            r#"<?xml version="1.0"?>
<d:multistatus xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav">
  <d:response>
    <d:href>/calendars/user/default/mock1.ics</d:href>
    <d:propstat>
      <d:prop>
        <d:getetag>"etag1"</d:getetag>
        <c:calendar-data>{}</c:calendar-data>
      </d:prop>
    </d:propstat>
  </d:response>
</d:multistatus>"#,
            cal_data
        );

        let mock = server.mock(|when, then| {
            when.path("/");
            then.status(207)
                .header("content-type", "application/xml")
                .body(&response_xml);
        });

        let events = caldav_get_events(
            &server.base_url(),
            "user@test.com",
            "password123",
            "2026-04-01T00:00:00Z",
            "2026-04-30T23:59:59Z",
        )
        .await
        .unwrap();

        mock.assert();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].summary.as_deref(), Some("Mock Event"));
        assert_eq!(events[0].id, "/calendars/user/default/mock1.ics");

        std::env::remove_var("TEST_CALDAV_BASE");
    }

    #[tokio::test]
    async fn test_caldav_create_event_via_mock() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let server = MockServer::start();
        std::env::set_var("TEST_CALDAV_BASE", server.base_url());

        let mock = server.mock(|when, then| {
            when.method(PUT).path_contains(".ics");
            then.status(201);
        });

        let event = NewCalendarEvent {
            summary: Some("New Event".to_string()),
            description: None,
            location: None,
            start: Some(CalendarDateTime {
                date: None,
                date_time: Some("2026-04-15T09:00:00Z".to_string()),
                time_zone: None,
            }),
            end: Some(CalendarDateTime {
                date: None,
                date_time: Some("2026-04-15T10:00:00Z".to_string()),
                time_zone: None,
            }),
        };

        let test_password =
            std::env::var("TEST_CALDAV_PASSWORD").unwrap_or_else(|_| "test-password".to_string());

        let created = caldav_create_event(
            &server.base_url(),
            "user@test.com",
            &test_password,
            &event,
        )
        .await
        .unwrap();

        mock.assert();
        assert_eq!(created.summary.as_deref(), Some("New Event"));
        assert!(created.id.ends_with(".ics"));

        std::env::remove_var("TEST_CALDAV_BASE");
    }

    #[tokio::test]
    async fn test_caldav_delete_event_via_mock() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let server = MockServer::start();
        std::env::set_var("TEST_CALDAV_BASE", server.base_url());

        let mock = server.mock(|when, then| {
            when.method(DELETE).path("/calendars/user/default/event1.ics");
            then.status(204);
        });

        let result = caldav_delete_event(
            &server.base_url(),
            "user@test.com",
            "password123",
            "/calendars/user/default/event1.ics",
        )
        .await;

        mock.assert();
        assert!(result.is_ok());

        std::env::remove_var("TEST_CALDAV_BASE");
    }

    #[tokio::test]
    async fn test_caldav_url_migration() {
        let options = sqlx::sqlite::SqliteConnectOptions::from_str("sqlite::memory:")
            .unwrap()
            .create_if_missing(true);
        let pool = sqlx::SqlitePool::connect_with(options).await.unwrap();
        crate::db::apply_schema(&pool).await.unwrap();
        crate::db::run_migrations(&pool).await.unwrap();

        let has_col = sqlx::query_scalar::<_, i32>(
            "SELECT COUNT(*) FROM pragma_table_info('imap_config') WHERE name = 'caldav_url'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(has_col, 1);

        sqlx::query(
            "INSERT INTO imap_config (account_id, imap_host, imap_port, smtp_host, smtp_port, auth_method, use_tls, caldav_url) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind("acc1")
        .bind("imap.fastmail.com")
        .bind(993)
        .bind("smtp.fastmail.com")
        .bind(587)
        .bind("password")
        .bind(1)
        .bind("https://caldav.fastmail.com/dav/calendars/user/acc1@fastmail.com/")
        .execute(&pool)
        .await
        .unwrap();

        let url: Option<String> = sqlx::query_scalar(
            "SELECT caldav_url FROM imap_config WHERE account_id = 'acc1'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            url.unwrap(),
            "https://caldav.fastmail.com/dav/calendars/user/acc1@fastmail.com/"
        );
    }

    use std::str::FromStr;
}
