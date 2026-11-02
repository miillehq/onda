//! Statistics derived only from content-free daily counts.
//!
//! A day is the local calendar day on which a dictation was typed, as recorded at the time; a
//! later time-zone change does not move earlier days. Only completed dictations count: edits,
//! previews and dictations that could not be typed are not part of these numbers.
use crate::contracts::UsageSummary;

/// A minute of audio before a pace is shown; shorter samples are mostly pauses.
pub const MIN_PACE_AUDIO_MS: f64 = 60_000.0;

/// Running totals kept beside the daily rows, which are pruned after 400 active days, so none
/// of these numbers stops growing or forgets a streak once the oldest rows go.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LifetimeUsage {
    pub since: Option<String>,
    pub dictations: u32,
    pub words: u32,
    pub audio_ms: f64,
    pub active_days: u32,
    /// The latest day with a dictation and the run of consecutive days ending on it.
    pub last_day: Option<String>,
    pub current_run: u32,
    pub longest_run: u32,
}

impl LifetimeUsage {
    /// Counts the first dictation on `day`. A day earlier than the latest one (after a clock or
    /// time-zone change) is an active day but leaves the streaks alone.
    pub fn count_active_day(&mut self, day: &str) {
        let Some(number) = day_number(day) else {
            return;
        };
        self.active_days = self.active_days.saturating_add(1);
        let last = self.last_day.as_deref().and_then(day_number);
        if last.is_some_and(|last| number <= last) {
            return;
        }
        self.current_run = if last == Some(number - 1) {
            self.current_run.saturating_add(1)
        } else {
            1
        };
        self.longest_run = self.longest_run.max(self.current_run);
        self.last_day = Some(day.into());
    }
}

/// Lifetime totals, pace and streaks as of `today` (a `YYYY-MM-DD` local day).
///
/// Pace is words recognised per minute of recorded audio, so pauses while the shortcut is held
/// lower it. The current streak counts back from today, or from yesterday when nothing has been
/// dictated yet today, so a streak is not shown as broken in the morning.
pub fn summarize(lifetime: &LifetimeUsage, today: &str) -> UsageSummary {
    let words_per_minute = (lifetime.audio_ms >= MIN_PACE_AUDIO_MS && lifetime.words > 0)
        .then(|| f64::from(lifetime.words) / (lifetime.audio_ms / 60_000.0));
    let last = lifetime.last_day.as_deref().and_then(day_number);
    let current_streak = match (day_number(today), last) {
        (Some(today), Some(last)) if last == today || last == today - 1 => lifetime.current_run,
        _ => 0,
    };
    UsageSummary {
        dictations: lifetime.dictations,
        words: lifetime.words,
        audio_ms: lifetime.audio_ms,
        since: lifetime.since.clone(),
        words_per_minute,
        current_streak,
        longest_streak: lifetime.longest_run,
        active_days: lifetime.active_days,
    }
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

    fn counted(days: &[&str]) -> LifetimeUsage {
        let mut lifetime = LifetimeUsage::default();
        for day in days {
            lifetime.count_active_day(day);
        }
        lifetime
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
        let lifetime = counted(&["2026-10-20", "2026-10-26", "2026-10-27", "2026-10-28"]);
        // Nothing yet today: yesterday's streak still counts.
        assert_eq!(summarize(&lifetime, "2026-10-28").current_streak, 3);
        assert_eq!(summarize(&lifetime, "2026-10-29").current_streak, 3);
        // Today counts once something is dictated.
        let mut with_today = lifetime.clone();
        with_today.count_active_day("2026-10-29");
        assert_eq!(summarize(&with_today, "2026-10-29").current_streak, 4);
        // A whole missed day ends it.
        let summary = summarize(&lifetime, "2026-10-30");
        assert_eq!(summary.current_streak, 0);
        assert_eq!(summary.longest_streak, 3);
        assert_eq!(summary.active_days, 4);
    }

    #[test]
    fn streaks_cross_month_and_year_boundaries_and_ignore_earlier_days() {
        let mut lifetime = counted(&["2025-12-31", "2026-01-01"]);
        assert_eq!((lifetime.current_run, lifetime.longest_run), (2, 2));
        // A day before the latest one, after the clock went back, is still an active day.
        lifetime.count_active_day("2025-12-20");
        lifetime.count_active_day("not a day");
        let summary = summarize(&lifetime, "2026-01-02");
        assert_eq!((summary.current_streak, summary.longest_streak), (2, 2));
        assert_eq!(summary.active_days, 3);
    }

    #[test]
    fn active_days_and_the_longest_streak_keep_growing_past_a_year() {
        let mut lifetime = LifetimeUsage::default();
        let first = day_number("2024-01-01").unwrap();
        for offset in 0..800 {
            let (year, month, date) = civil(first + offset);
            lifetime.count_active_day(&format!("{year:04}-{month:02}-{date:02}"));
        }
        assert_eq!((lifetime.active_days, lifetime.longest_run), (800, 800));
        assert_eq!(lifetime.last_day.as_deref(), Some("2026-03-10"));
    }

    /// Inverse of `day_number`, for building long runs of days in tests.
    fn civil(days: i64) -> (i64, i64, i64) {
        let days = days + 719_468;
        let era = days.div_euclid(146_097);
        let day_of_era = days - era * 146_097;
        let year_of_era =
            (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
        let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
        let month_index = (5 * day_of_year + 2) / 153;
        let date = day_of_year - (153 * month_index + 2) / 5 + 1;
        let month = if month_index < 10 {
            month_index + 3
        } else {
            month_index - 9
        };
        let year = year_of_era + era * 400 + i64::from(month <= 2);
        (year, month, date)
    }

    #[test]
    fn pace_needs_a_minute_of_audio_and_comes_from_lifetime_totals() {
        let mut lifetime = LifetimeUsage {
            since: Some("2026-10-01".into()),
            dictations: 3,
            words: 75,
            audio_ms: 59_999.0,
            ..LifetimeUsage::default()
        };
        assert_eq!(summarize(&lifetime, "2026-10-29").words_per_minute, None);
        lifetime.audio_ms = 30_000.0 * 3.0;
        let summary = summarize(&lifetime, "2026-10-29");
        assert_eq!(summary.words_per_minute, Some(50.0));
        assert_eq!((summary.dictations, summary.words), (3, 75));
        assert_eq!(summary.since.as_deref(), Some("2026-10-01"));
    }

    #[test]
    fn empty_history_is_all_zero_without_inventing_days() {
        let summary = summarize(&LifetimeUsage::default(), "2026-10-29");
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
