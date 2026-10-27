use chrono::{DateTime, NaiveDate, Utc};
use mcp_runtime::input::{Parse, object, optional};
use rmcp::model::JsonObject;
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct Query {
    pub start: NaiveDate,
    pub end: NaiveDate,
    pub latitude: f64,
    pub longitude: f64,
    pub last_notified: u32,
}

fn date(parse: &mut Parse, value: Option<&Value>) -> Option<NaiveDate> {
    let text = parse.string(value, (10, 10))?;
    let date = NaiveDate::parse_from_str(&text, "%Y-%m-%d")
        .ok()
        .filter(|date| date.to_string() == text);
    if date.is_none() {
        parse.issue("Expected a real UTC date in YYYY-MM-DD format".into(), true);
    }
    date
}

fn number(parse: &mut Parse, value: Option<&Value>, min: f64, max: f64) -> Option<f64> {
    let Some(number) = value.and_then(Value::as_f64) else {
        parse.wrong_type("number", value);
        return None;
    };
    if !number.is_finite() || !(min..=max).contains(&number) {
        parse.issue(format!("Expected a number between {min} and {max}"), true);
        return None;
    }
    Some(number)
}

pub fn parse(args: &JsonObject, now: DateTime<Utc>) -> Result<Query, String> {
    let (start, end, latitude, longitude, last) = object(
        args,
        &[
            "start_date",
            "end_date",
            "latitude",
            "longitude",
            "last_notified_threshold",
        ],
        |parse, args| {
            let start = date(&mut parse.at("start_date"), args.get("start_date"));
            let end = optional(parse, args, "end_date", date);
            let latitude = number(&mut parse.at("latitude"), args.get("latitude"), -90.0, 90.0);
            let longitude = number(
                &mut parse.at("longitude"),
                args.get("longitude"),
                -180.0,
                180.0,
            );
            let last = optional(parse, args, "last_notified_threshold", |parse, value| {
                let n = number(parse, value, 30.0, 36_600.0)?;
                if n % 5.0 != 0.0 {
                    parse.issue(
                        "Expected 30 (none sent), or a multiple of 5 from 35".into(),
                        true,
                    );
                    return None;
                }
                Some(n as u32)
            });
            (start, end, latitude, longitude, last)
        },
        |_| None,
    )?;
    let start = start.ok_or_else(String::new)?;
    let end = end.unwrap_or(now.date_naive());
    if start > end || end > now.date_naive() || (end - start).num_days() >= 366 {
        return Err("Dates must be ordered, not in the future, and span at most 366 days".into());
    }
    Ok(Query {
        start,
        end,
        latitude: latitude.ok_or_else(String::new)?,
        longitude: longitude.ok_or_else(String::new)?,
        last_notified: last.unwrap_or(30),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn rejects_invalid_dates_coordinates_cursors_and_unknown_keys() {
        let now = "2026-10-27T18:30:00Z".parse().unwrap();
        let valid = json!({"start_date":"2026-10-25","latitude":64.1,"longitude":-21.9});
        let q = parse(valid.as_object().unwrap(), now).unwrap();
        assert_eq!(q.last_notified, 30);
        assert_eq!(q.end.to_string(), "2026-10-27");
        for (key, value) in [
            ("start_date", json!("2026-02-30")),
            ("start_date", json!("2026-10-28")),
            ("start_date", json!("2025-01-01")),
            ("start_date", json!(null)),
            ("latitude", json!(91)),
            ("longitude", json!("-21.9")),
            ("longitude", json!(-181)),
            ("last_notified_threshold", json!(36)),
            ("last_notified_threshold", json!(35.5)),
            ("end_date", json!("2026-10-24")),
            ("end_date", json!(null)),
            ("extra", json!(true)),
        ] {
            let mut args = valid.clone();
            args[key] = value;
            assert!(parse(args.as_object().unwrap(), now).is_err(), "{args}");
        }
    }
}
