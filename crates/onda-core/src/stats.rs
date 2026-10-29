//! Statistics derived only from content-free daily counts.
//!
//! A day is the local calendar day on which a dictation was typed, as recorded at the time; a
//! later time-zone change does not move earlier days. Only completed dictations count: edits,
//! previews and dictations that could not be typed are not part of these numbers.
use crate::contracts::{DailyUsage, UsageSummary};

/// A minute of audio before a pace is shown; shorter samples are mostly pauses.
pub const MIN_PACE_AUDIO_MS: f64 = 60_000.0;

/// Running totals kept beside the daily rows, which are pruned after 400 active days.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LifetimeUsage {
    pub since: Option<String>,
    pub dictations: u32,
    pub words: u32,
    pub audio_ms: f64,
}

/// Lifetime totals, pace and streaks as of `today` (a `YYYY-MM-DD` local day).
///
/// Pace is words recognised per minute of recorded audio, so pauses while the shortcut is held
/// lower it. The current streak counts back from today, or from yesterday when nothing has been
/// dictated yet today, so a streak is not shown as broken in the morning.
pub fn summarize(lifetime: &LifetimeUsage, days: &[DailyUsage], today: &str) -> UsageSummary {
    let today = day_number(today);
    let mut active: Vec<i64> = days
        .iter()
        .filter(|day| day.dictations > 0)
        .filter_map(|day| day_number(&day.day))
        .filter(|day| today.is_none_or(|today| *day <= today))
        .collect();
    active.sort_unstable();
    active.dedup();
    let words_per_minute = (lifetime.audio_ms >= MIN_PACE_AUDIO_MS && lifetime.words > 0)
        .then(|| f64::from(lifetime.words) / (lifetime.audio_ms / 60_000.0));
    UsageSummary {
        dictations: lifetime.dictations,
        words: lifetime.words,
        audio_ms: lifetime.audio_ms,
        since: lifetime.since.clone(),
        words_per_minute,
        current_streak: today.map_or(0, |today| current_streak(&active, today)),
        longest_streak: longest_streak(&active),
        active_days: u32::try_from(active.len()).unwrap_or(u32::MAX),
    }
}

/// `active` is sorted and deduplicated.
fn current_streak(active: &[i64], today: i64) -> u32 {
    let contains = |day: i64| active.binary_search(&day).is_ok();
    let mut day = if contains(today) {
        today
    } else if contains(today - 1) {
        today - 1
    } else {
        return 0;
    };
    let mut length = 0;
    while contains(day) {
        length += 1;
        day -= 1;
    }
    length
}

fn longest_streak(active: &[i64]) -> u32 {
    let mut longest = 0;
    let mut run = 0;
    let mut previous: Option<i64> = None;
    for &day in active {
        run = if previous == Some(day - 1) {
            run + 1
        } else {
            1
        };
        longest = longest.max(run);
        previous = Some(day);
    }
    longest
}

/// Days since 1970-01-01 for a valid `YYYY-MM-DD` key (proleptic Gregorian calendar).
pub fn day_number(day: &str) -> Option<i64> {
    let bytes = day.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }
    let year: i64 = day.get(0..4)?.parse().ok()?;
    let month: i64 = day.get(5..7)?.parse().ok()?;
    let date: i64 = day.get(8..10)?.parse().ok()?;
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let month_length = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return None,
    };
    if !(1..=month_length).contains(&date) {
        return None;
    }
    // Howard Hinnant's days_from_civil.
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + date - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    Some(era * 146_097 + day_of_era - 719_468)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(key: &str, dictations: u32, words: u32) -> DailyUsage {
        DailyUsage {
            day: key.into(),
            dictations,
            words,
            audio_ms: 0,
        }
    }

    #[test]
    fn day_numbers_follow_the_calendar() {
        assert_eq!(day_number("1970-01-01"), Some(0));
        assert_eq!(day_number("2026-10-29"), Some(20_755));
        assert_eq!(
            day_number("2024-03-01").unwrap() - day_number("2024-02-28").unwrap(),
            2
        );
        assert_eq!(
            day_number("2026-03-01").unwrap() - day_number("2026-02-28").unwrap(),
            1
        );
        for invalid in [
            "2026-02-29",
            "2026-13-01",
            "2026-00-10",
            "2026-1-01",
            "26-10-29x",
        ] {
            assert_eq!(day_number(invalid), None, "{invalid}");
        }
    }

    #[test]
    fn streak_survives_the_morning_and_breaks_after_a_missed_day() {
        let days = [
            day("2026-10-28", 2, 40),
            day("2026-10-27", 1, 10),
            day("2026-10-26", 1, 5),
            day("2026-10-20", 3, 90),
        ];
        let lifetime = LifetimeUsage::default();
        // Nothing yet today: yesterday's streak still counts.
        assert_eq!(summarize(&lifetime, &days, "2026-10-29").current_streak, 3);
        // Today counts once something is dictated.
        let mut with_today = days.to_vec();
        with_today.push(day("2026-10-29", 1, 3));
        assert_eq!(
            summarize(&lifetime, &with_today, "2026-10-29").current_streak,
            4
        );
        // A whole missed day ends it.
        let summary = summarize(&lifetime, &days, "2026-10-30");
        assert_eq!(summary.current_streak, 0);
        assert_eq!(summary.longest_streak, 3);
        assert_eq!(summary.active_days, 4);
    }

    #[test]
    fn streaks_cross_month_and_year_boundaries_and_ignore_empty_or_future_days() {
        let days = [
            day("2026-01-01", 1, 1),
            day("2025-12-31", 1, 1),
            day("2025-12-30", 0, 0),
            day("2026-01-05", 4, 4),
        ];
        let summary = summarize(&LifetimeUsage::default(), &days, "2026-01-02");
        assert_eq!((summary.current_streak, summary.longest_streak), (2, 2));
        assert_eq!(
            summary.active_days, 2,
            "zero-dictation and future rows are not activity"
        );
    }

    #[test]
    fn pace_needs_a_minute_of_audio_and_comes_from_lifetime_totals() {
        let mut lifetime = LifetimeUsage {
            since: Some("2026-10-01".into()),
            dictations: 3,
            words: 75,
            audio_ms: 59_999.0,
        };
        assert_eq!(
            summarize(&lifetime, &[], "2026-10-29").words_per_minute,
            None
        );
        lifetime.audio_ms = 30_000.0 * 3.0;
        let summary = summarize(&lifetime, &[], "2026-10-29");
        assert_eq!(summary.words_per_minute, Some(50.0));
        assert_eq!((summary.dictations, summary.words), (3, 75));
        assert_eq!(summary.since.as_deref(), Some("2026-10-01"));
    }

    #[test]
    fn empty_history_is_all_zero_without_inventing_days() {
        let summary = summarize(&LifetimeUsage::default(), &[], "2026-10-29");
        assert_eq!(summary.since, None);
        assert_eq!(
            (
                summary.current_streak,
                summary.longest_streak,
                summary.active_days
            ),
            (0, 0, 0)
        );
        assert_eq!(summary.words_per_minute, None);
    }
}
