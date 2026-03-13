use chrono::{DateTime, Datelike, Duration, FixedOffset, Local, NaiveDate, NaiveTime, Offset, TimeZone};

use crate::models::{
    AppSettings, EffectiveDimMode, EffectiveDimState, ManualOverrideSession, ScheduleEvaluation,
    SchedulePoint,
};

pub fn clamp_dim_precise(value: f64) -> f64 {
    ((value * 100.0).round() / 100.0).clamp(0.0, 99.0)
}

pub fn normalize_settings(settings: AppSettings) -> AppSettings {
    let mut normalized = settings;
    normalized.version = crate::models::CURRENT_VERSION;
    normalized.dim_step_percent = normalized.dim_step_percent.clamp(1.0, 25.0);
    normalized.schedule_points = normalize_points(normalized.schedule_points);
    normalized
}

pub fn normalize_points(points: Vec<SchedulePoint>) -> Vec<SchedulePoint> {
    let mut points = points
        .into_iter()
        .map(|mut point| {
            point.target_dim_percent = clamp_dim_precise(point.target_dim_percent);
            point.transition_minutes = point.transition_minutes.clamp(0, 1_439);
            point
        })
        .collect::<Vec<_>>();
    points.sort_by(|left, right| left.time_of_day.cmp(&right.time_of_day));
    points
}

pub fn validate_points(points: &[SchedulePoint]) -> Result<(), String> {
    let mut enabled_points = points
        .iter()
        .filter(|point| point.enabled)
        .cloned()
        .collect::<Vec<_>>();

    enabled_points.sort_by(|left, right| left.time_of_day.cmp(&right.time_of_day));
    if enabled_points.is_empty() {
        return Ok(());
    }

    if enabled_points
        .iter()
        .any(|point| point.transition_minutes < 0 || point.transition_minutes > 1_439)
    {
        return Err("Transition duration must be between 0 and 1439 minutes.".into());
    }

    let baseline = NaiveDate::from_ymd_opt(2030, 1, 1).unwrap();
    let offset = FixedOffset::east_opt(0).unwrap();
    let mut occurrences = Vec::new();

    for day_offset in 0..=1 {
        let date = baseline + Duration::days(day_offset);
        for point in &enabled_points {
            let naive_target = date.and_time(parse_time(&point.time_of_day)?);
            let target_time: DateTime<FixedOffset> = offset.from_utc_datetime(&naive_target);
            let transition_start = target_time - Duration::minutes(point.transition_minutes as i64);
            occurrences.push((target_time, transition_start));
        }
    }

    occurrences.sort_by_key(|item| item.0);
    for pair in occurrences.windows(2) {
        let current = &pair[0];
        let next = &pair[1];
        if next.1 < current.0 {
            return Err(format!(
                "The transition starting at {} overlaps the point at {}.",
                next.1.format("%H:%M"),
                current.0.format("%H:%M")
            ));
        }
    }

    Ok(())
}

pub fn get_effective_dim(
    settings: &AppSettings,
    now: DateTime<FixedOffset>,
) -> Result<ScheduleEvaluation, String> {
    if !settings.schedule_enabled {
        return Ok(ScheduleEvaluation {
            current_dim_percent: 0.0,
            next_transition_start: None,
        });
    }

    let normalized = normalize_points(settings.schedule_points.clone())
        .into_iter()
        .filter(|point| point.enabled)
        .collect::<Vec<_>>();

    if normalized.is_empty() {
        return Ok(ScheduleEvaluation {
            current_dim_percent: 0.0,
            next_transition_start: None,
        });
    }

    validate_points(&normalized)?;

    let mut occurrences = build_occurrences(&normalized, now)?;
    occurrences.sort_by_key(|item| item.target_time);

    let active = occurrences
        .iter()
        .filter(|occurrence| occurrence.transition_start <= now && now <= occurrence.target_time)
        .max_by_key(|occurrence| occurrence.target_time);

    let current_dim_percent = if let Some(active) = active {
        let duration = (active.target_time - active.transition_start).num_seconds() as f64;
        let elapsed = (now - active.transition_start).num_seconds() as f64;
        let progress = if duration <= 0.0 {
            1.0
        } else {
            (elapsed / duration).clamp(0.0, 1.0)
        };
        active.start_dim_percent + ((active.target_dim_percent - active.start_dim_percent) * progress)
    } else {
        occurrences
            .iter()
            .filter(|occurrence| occurrence.target_time <= now)
            .max_by_key(|occurrence| occurrence.target_time)
            .map(|occurrence| occurrence.target_dim_percent)
            .unwrap_or(0.0)
    };

    let next_transition_start = occurrences
        .iter()
        .filter(|occurrence| occurrence.transition_start > now)
        .min_by_key(|occurrence| occurrence.transition_start)
        .map(|occurrence| occurrence.transition_start);

    Ok(ScheduleEvaluation {
        current_dim_percent: clamp_dim_precise(current_dim_percent),
        next_transition_start,
    })
}

pub fn resolve_state(settings: &AppSettings, session: &ManualOverrideSession, now: DateTime<FixedOffset>) -> EffectiveDimState {
    if session.is_paused {
        return EffectiveDimState {
            mode: EffectiveDimMode::Paused,
            current_dim_percent: clamp_dim_precise(session.paused_dim_percent),
            manual_override_until: None,
        };
    }

    if let Some(manual) = session.manual_dim_percent {
        if session.manual_override_until.map(|until| now < until).unwrap_or(true) {
            return EffectiveDimState {
                mode: EffectiveDimMode::Manual,
                current_dim_percent: clamp_dim_precise(manual),
                manual_override_until: session.manual_override_until,
            };
        }
    }

    let scheduled = get_effective_dim(settings, now).unwrap_or(ScheduleEvaluation {
        current_dim_percent: 0.0,
        next_transition_start: None,
    });
    EffectiveDimState {
        mode: EffectiveDimMode::Auto,
        current_dim_percent: scheduled.current_dim_percent,
        manual_override_until: None,
    }
}

pub fn now_fixed() -> DateTime<FixedOffset> {
    let now = Local::now();
    now.with_timezone(&now.offset().fix())
}

fn parse_time(value: &str) -> Result<NaiveTime, String> {
    NaiveTime::parse_from_str(value, "%H:%M:%S")
        .or_else(|_| NaiveTime::parse_from_str(value, "%H:%M"))
        .map_err(|_| format!("Invalid time value '{value}'."))
}

#[derive(Clone)]
struct Occurrence {
    transition_start: DateTime<FixedOffset>,
    target_time: DateTime<FixedOffset>,
    start_dim_percent: f64,
    target_dim_percent: f64,
}

fn build_occurrences(
    points: &[SchedulePoint],
    now: DateTime<FixedOffset>,
) -> Result<Vec<Occurrence>, String> {
    let today = NaiveDate::from_ymd_opt(now.year(), now.month(), now.day()).unwrap();
    let mut occurrences = Vec::new();

    for day_offset in -2..=2 {
        let date = today + Duration::days(day_offset);
        for point in points {
            let naive_target = date.and_time(parse_time(&point.time_of_day)?);
            let target_time = now.offset().from_local_datetime(&naive_target).single().unwrap();
            occurrences.push((point.clone(), target_time, clamp_dim_precise(point.target_dim_percent)));
        }
    }

    occurrences.sort_by_key(|item| item.1);
    let mut final_occurrences = Vec::new();
    for index in 0..occurrences.len() {
        let previous_index = if index == 0 { occurrences.len() - 1 } else { index - 1 };
        let current = &occurrences[index];
        let previous = &occurrences[previous_index];
        final_occurrences.push(Occurrence {
            transition_start: current.1 - Duration::minutes(current.0.transition_minutes as i64),
            target_time: current.1,
            start_dim_percent: previous.2,
            target_dim_percent: current.2,
        });
    }

    Ok(final_occurrences)
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, FixedOffset};
    use uuid::Uuid;

    use super::*;
    use crate::models::{AppSettings, ManualOverrideSession, SchedulePoint};

    fn point(time: &str, target: f64, transition: i32) -> SchedulePoint {
        SchedulePoint {
            id: Uuid::new_v4(),
            time_of_day: time.to_string(),
            target_dim_percent: target,
            transition_minutes: transition,
            enabled: true,
        }
    }


    #[test]
    fn clamp_dim_precise_rounds_to_hundredths_and_caps_at_ninety_nine() {
        assert_eq!(clamp_dim_precise(-1.0), 0.0);
        assert_eq!(clamp_dim_precise(99.999), 99.0);
        assert_eq!(clamp_dim_precise(12.3456), 12.35);
    }
    #[test]
    fn interpolates_within_transition_window() {
        let settings = AppSettings {
            schedule_points: vec![point("07:00:00", 0.0, 30), point("23:00:00", 50.0, 60)],
            ..AppSettings::default()
        };
        let offset = FixedOffset::east_opt(3600).unwrap();
        let now = DateTime::parse_from_rfc3339("2026-03-12T22:30:00+01:00").unwrap().with_timezone(&offset);
        let result = get_effective_dim(&settings, now).unwrap();
        assert!((result.current_dim_percent - 25.0).abs() < 0.01);
    }

    #[test]
    fn rejects_overlapping_transitions() {
        let points = vec![point("22:00:00", 20.0, 120), point("23:00:00", 50.0, 90)];
        assert!(validate_points(&points).is_err());
    }

    #[test]
    fn manual_override_expires_at_next_transition_start() {
        let settings = AppSettings {
            schedule_points: vec![point("07:00:00", 0.0, 30), point("23:00:00", 50.0, 60)],
            ..AppSettings::default()
        };
        let offset = FixedOffset::east_opt(3600).unwrap();
        let schedule = get_effective_dim(
            &settings,
            DateTime::parse_from_rfc3339("2026-03-12T21:30:00+01:00").unwrap().with_timezone(&offset),
        )
        .unwrap();
        let session = ManualOverrideSession {
            is_paused: false,
            manual_dim_percent: Some(10.0),
            manual_override_until: schedule.next_transition_start,
            paused_dim_percent: 0.0,
        };
        let before = resolve_state(
            &settings,
            &session,
            DateTime::parse_from_rfc3339("2026-03-12T21:45:00+01:00").unwrap().with_timezone(&offset),
        );
        let at_start = resolve_state(
            &settings,
            &session,
            DateTime::parse_from_rfc3339("2026-03-12T22:00:00+01:00").unwrap().with_timezone(&offset),
        );
        assert_eq!(before.mode, EffectiveDimMode::Manual);
        assert_eq!(at_start.mode, EffectiveDimMode::Auto);
    }
}


