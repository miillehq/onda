use onda_core::{
    persistence::Store,
    stats::{day_number, summarize},
};

#[test]
fn an_earlier_local_day_joins_both_sides_of_a_streak_only_once() {
    let mut store = Store::in_memory().unwrap();
    for day in ["2026-11-01", "2026-11-03", "2026-11-02", "2026-11-02"] {
        store.record_usage(day, 1, 1000, 10, false).unwrap();
    }
    let total = summarize(&store.lifetime_usage().unwrap(), "2026-11-03");
    assert_eq!(
        (
            total.dictations,
            total.active_days,
            total.current_streak,
            total.longest_streak
        ),
        (4, 3, 3, 3)
    );
    assert_eq!(store.usage_summary("2026-11-02").unwrap().current_streak, 2);
}

#[test]
fn calendar_dates_must_be_possible_and_use_ascii_digits() {
    let mut store = Store::in_memory().unwrap();
    for day in ["2026-02-30", "2026-+1-+1", "2026-00-04", "2025-02-29"] {
        assert_eq!(day_number(day), None, "accepted {day}");
        assert!(
            store.record_usage(day, 1, 1000, 10, false).is_err(),
            "stored {day}"
        );
    }
    assert!(store.daily_usage(10).unwrap().is_empty());
    store
        .record_usage("2024-02-29", 1, 1000, 10, false)
        .unwrap();
}

#[test]
fn filling_a_gap_after_a_long_streak_keeps_the_part_before_the_daily_window() {
    let calendar = rusqlite::Connection::open_in_memory().unwrap();
    let date = |offset: i32| -> String {
        calendar
            .query_row(
                "SELECT date('2024-01-01', ?1 || ' days')",
                [offset],
                |row| row.get(0),
            )
            .unwrap()
    };
    let mut store = Store::in_memory().unwrap();
    for offset in 0..450 {
        store
            .record_usage(&date(offset), 1, 1000, 10, false)
            .unwrap();
    }
    for offset in [451, 450] {
        store
            .record_usage(&date(offset), 1, 1000, 10, false)
            .unwrap();
    }
    let total = store.usage_summary(&date(451)).unwrap();
    assert_eq!((total.current_streak, total.longest_streak), (452, 452));
    assert_eq!(store.daily_usage(1000).unwrap().len(), 400);
}
