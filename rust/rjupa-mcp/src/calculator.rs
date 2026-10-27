use std::collections::BTreeMap;

use chrono::{DateTime, Duration, Timelike, Utc};
use mcp_runtime::Fail;
use serde_json::{Value, json};

use crate::{http, input::Query};

const SHAPE: Fail = Fail::Safe(
    "The Rjúpa calculator response changed or did not match the request. No reminder state was changed.",
);

fn finite(value: &Value) -> Result<f64, Fail> {
    value.as_f64().filter(|n| n.is_finite()).ok_or(SHAPE)
}

fn count(value: &Value) -> Result<u64, Fail> {
    value.as_u64().filter(|n| *n <= 8784).ok_or(SHAPE)
}

/// Compare unrounded hourly temperature sums, not the site's two-decimal display total.
fn milestones(temperature_sum: f64, last: u32) -> (Vec<u32>, u32) {
    let mut pending = Vec::new();
    let mut next = 35;
    while f64::from(next) * 24.0 <= temperature_sum {
        if next > last {
            pending.push(next);
        }
        next += 5;
    }
    (pending, next.max(last + 5))
}

pub fn shape(
    body: &Value,
    q: &Query,
    checked: DateTime<Utc>,
    fetched: DateTime<Utc>,
) -> Result<Value, Fail> {
    if body["startDate"].as_str() != Some(q.start.to_string().as_str())
        || body["endDate"].as_str() != Some(q.end.to_string().as_str())
        || (finite(&body["location"]["latitude"])? - q.latitude).abs() > 0.000001
        || (finite(&body["location"]["longitude"])? - q.longitude).abs() > 0.000001
    {
        return Err(SHAPE);
    }
    let site_total = finite(&body["total"])?;
    if site_total < 0.0 {
        return Err(SHAPE);
    }
    let quality = &body["quality"];
    let source = quality["source"].as_str().ok_or(SHAPE)?;
    if ![
        "vedur",
        "vedur-interpolated",
        "vedur-open-meteo",
        "open-meteo",
    ]
    .contains(&source)
    {
        return Err(SHAPE);
    }
    let declared_expected = count(&quality["expectedHours"])?;
    let observed = count(&quality["observedHours"])?;
    let interpolated = count(&quality["interpolatedHours"])?;
    let modeled = count(&quality["modeledHours"])?;
    let station = if body["station"].is_null() {
        Value::Null
    } else {
        let station = &body["station"];
        let name = station["name"]
            .as_str()
            .filter(|s| s.chars().count() <= 200)
            .ok_or(SHAPE)?;
        json!({"id":station["id"].as_u64().ok_or(SHAPE)?,"name":name,
            "latitude":finite(&station["latitude"])?,"longitude":finite(&station["longitude"])?})
    };
    let start = q.start.and_hms_opt(0, 0, 0).ok_or(SHAPE)?.and_utc();
    let requested_end = q
        .end
        .succ_opt()
        .ok_or(SHAPE)?
        .and_hms_opt(0, 0, 0)
        .ok_or(SHAPE)?
        .and_utc();
    // Only completed UTC hours can trigger a reminder. Current/future rows are not evidence.
    let cutoff = requested_end.min(
        checked
            .with_minute(0)
            .and_then(|t| t.with_second(0))
            .and_then(|t| t.with_nanosecond(0))
            .ok_or(SHAPE)?,
    );
    let expected = (cutoff - start).num_hours() as u64;
    let rows = body["hourly"]
        .as_array()
        .filter(|rows| rows.len() <= 8784)
        .ok_or(SHAPE)?;
    let mut hours = BTreeMap::new();
    let mut all_sum = 0.0;
    let mut excluded = 0;
    let mut unique = BTreeMap::new();
    for row in rows {
        let time: DateTime<Utc> = row["t"].as_str().ok_or(SHAPE)?.parse().map_err(|_| SHAPE)?;
        let temp = finite(&row["temp"])?;
        if !(-100.0..=100.0).contains(&temp)
            || time.minute() != 0
            || time.second() != 0
            || time.nanosecond() != 0
            || time < start
            || time >= requested_end
            || unique.insert(time, ()).is_some()
        {
            return Err(SHAPE);
        }
        all_sum += temp.max(1.0);
        if time >= cutoff {
            excluded += 1;
            continue;
        }
        hours.insert(time, temp.max(1.0));
    }
    if (all_sum / 24.0 - site_total).abs() > 0.011 {
        return Err(SHAPE);
    }
    let sum: f64 = hours.values().sum();
    // Missing hours are not silently zero-filled. Quality metadata must also account for rows.
    let complete = hours.len() as u64 == expected
        && expected > 0
        && declared_expected == rows.len() as u64
        && observed + interpolated + modeled == rows.len() as u64;
    let (pending, next) = milestones(sum, q.last_notified);
    let latest = hours.keys().next_back().map(|t| t.to_rfc3339());
    Ok(json!({
        "source":{"url":http::URL,"api_kind":"internal_unversioned","weather_source":source},
        "attribution":"Calculator: rjupa.irdn.is. Weather: Veðurstofa Íslands (IMO); Open-Meteo where identified by quality.",
        "fetched_at":fetched.to_rfc3339(),"checked_at":checked.to_rfc3339(),
        "start_date":q.start.to_string(),"end_date":q.end.to_string(),
        "location":{"latitude":q.latitude,"longitude":q.longitude},"station":station,
        "units":"°C·day","degree_days":sum / 24.0,"site_total":site_total,
        "coverage":{"complete":complete,"expected_hours":expected,"included_hours":hours.len(),
            "excluded_unfinished_or_future_hours":excluded,"latest_hour":latest,
            "through":(hours.keys().next_back().map(|t| *t + Duration::hours(1))).map(|t|t.to_rfc3339())},
        "quality":{"source":source,"expected_hours":declared_expected,"observed_hours":observed,
            "interpolated_hours":interpolated,"modeled_hours":modeled,
            "estimated":interpolated > 0 || modeled > 0},
        "reminders_ready":complete,"last_notified_threshold":q.last_notified,
        "pending_thresholds":if complete {json!(pending)} else {Value::Null},
        "next_threshold":if complete {json!(next)} else {Value::Null},
        "note":if complete {"Completed hourly samples only; weather-based estimate, not actual hanging-room temperature. The host sends notifications and persists acknowledgements."} else {"Coverage is incomplete or unavailable. Pending thresholds are unknown, not empty. Preserve the notification cursor and retry later."}
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(temp: f64) -> (Value, Query, DateTime<Utc>) {
        let now = "2026-10-27T00:30:00Z".parse().unwrap();
        let q = Query {
            start: "2026-10-26".parse().unwrap(),
            end: "2026-10-26".parse().unwrap(),
            latitude: 64.1,
            longitude: -21.9,
            last_notified: 30,
        };
        let rows: Vec<Value> = (0..24)
            .map(|h| json!({"t":format!("2026-10-26T{h:02}:00:00Z"),"temp":temp,"origin":"vedur"}))
            .collect();
        (
            json!({"startDate":"2026-10-26","endDate":"2026-10-26","location":{"latitude":64.1,"longitude":-21.9},"total":temp.max(1.0),"hourly":rows,
            "quality":{"source":"vedur","expectedHours":24,"observedHours":24,"interpolatedHours":0,"modeledHours":0},"station":null}),
            q,
            now,
        )
    }

    #[test]
    fn thresholds_catch_up_do_not_repeat_and_do_not_round_up() {
        assert_eq!(milestones(34.999 * 24.0, 30), (vec![], 35));
        assert_eq!(milestones(35.0 * 24.0, 30), (vec![35], 40));
        assert_eq!(milestones(50.0 * 24.0, 30), (vec![35, 40, 45, 50], 55));
        assert_eq!(milestones(50.0 * 24.0, 40), (vec![45, 50], 55));
        assert_eq!(milestones(40.0 * 24.0, 50), (vec![], 55));
    }

    #[test]
    fn negative_and_cold_hours_count_as_one() {
        let (body, q, now) = sample(-5.0);
        let out = shape(&body, &q, now, now).unwrap();
        assert_eq!(out["degree_days"], 1.0);
        assert_eq!(out["pending_thresholds"], json!([]));
    }

    #[test]
    fn complete_coverage_and_exact_thresholds() {
        let (body, q, now) = sample(40.0);
        let out = shape(&body, &q, now, now).unwrap();
        assert_eq!(out["pending_thresholds"], json!([35, 40]));
        assert_eq!(out["next_threshold"], 45);
        assert_eq!(out["coverage"]["complete"], true);
    }

    #[test]
    fn missing_stale_and_inconsistent_coverage_never_claim_no_reminders() {
        for remove in [0, 12, 23] {
            let (mut body, q, now) = sample(40.0);
            body["hourly"].as_array_mut().unwrap().remove(remove);
            body["total"] = json!(40.0 * 23.0 / 24.0);
            let out = shape(&body, &q, now, now).unwrap();
            assert_eq!(out["reminders_ready"], false);
            assert!(out["pending_thresholds"].is_null());
        }
        let (mut body, q, now) = sample(40.0);
        body["quality"]["observedHours"] = json!(23);
        assert_eq!(
            shape(&body, &q, now, now).unwrap()["reminders_ready"],
            false
        );
    }

    #[test]
    fn current_and_future_hours_cannot_trigger_early_alerts() {
        let (body, q, _) = sample(40.0);
        let now = "2026-10-26T20:30:00Z".parse().unwrap();
        let out = shape(&body, &q, now, now).unwrap();
        assert_eq!(out["coverage"]["excluded_unfinished_or_future_hours"], 4);
        assert_eq!(out["pending_thresholds"], json!([]));
        assert!(out["degree_days"].as_f64().unwrap() < 35.0);
    }

    #[test]
    fn malformed_responses_and_request_mismatches_fail_closed() {
        for key in [
            "total",
            "hourly",
            "quality",
            "location",
            "startDate",
            "endDate",
        ] {
            let (mut body, q, now) = sample(40.0);
            body[key] = Value::Null;
            assert!(shape(&body, &q, now, now).is_err(), "{key}");
        }
        let (mut body, q, now) = sample(40.0);
        body["hourly"][1] = body["hourly"][0].clone();
        assert!(shape(&body, &q, now, now).is_err());
        let (mut body, q, now) = sample(40.0);
        body["hourly"][0]["temp"] = Value::Null;
        assert!(shape(&body, &q, now, now).is_err());
    }

    #[test]
    fn fallback_and_interpolation_remain_labeled_as_estimates() {
        let (mut body, q, now) = sample(40.0);
        body["quality"] = json!({"source":"vedur-open-meteo","expectedHours":24,"observedHours":20,"interpolatedHours":2,"modeledHours":2});
        let out = shape(&body, &q, now, now).unwrap();
        assert_eq!(out["quality"]["estimated"], true);
        assert_eq!(out["reminders_ready"], true);
    }
}
