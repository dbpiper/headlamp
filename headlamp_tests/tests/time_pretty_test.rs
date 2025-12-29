use std::num::NonZeroUsize;
use std::time::{Duration, SystemTime};

use headlamp::format::time::{
    PrettyDurationOptions, TimeUnit, format_duration, format_duration_at_least,
    format_duration_with, format_system_time_delta,
};

fn largest_unit_rank(formatted_duration: &str) -> u8 {
    formatted_duration
        .split_whitespace()
        .next()
        .map(unit_rank)
        .unwrap_or(0)
}

fn unit_rank(formatted_part: &str) -> u8 {
    if formatted_part.ends_with("ns") {
        return 0;
    }
    if formatted_part.ends_with("µs") {
        return 1;
    }
    if formatted_part.ends_with("ms") {
        return 2;
    }
    if formatted_part.ends_with('s') {
        return 3;
    }
    if formatted_part.ends_with('d') {
        return 6;
    }
    if formatted_part.ends_with('h') {
        return 5;
    }
    if formatted_part.ends_with('m') {
        return 4;
    }
    0
}

#[test]
fn format_duration_unit_boundaries() {
    assert_eq!(format_duration(Duration::ZERO), "<1ns");
    assert_eq!(format_duration(Duration::from_nanos(999)), "999ns");
    assert_eq!(format_duration(Duration::from_nanos(1_000)), "1µs");
    assert_eq!(format_duration(Duration::from_nanos(1_000_000)), "1ms");
    assert_eq!(format_duration(Duration::from_nanos(1_000_000_000)), "1s");
    assert_eq!(format_duration(Duration::from_secs(60)), "1m");
    assert_eq!(format_duration(Duration::from_secs(3_600)), "1h");
    assert_eq!(format_duration(Duration::from_secs(86_400)), "1d");
}

#[test]
fn format_duration_at_least_seconds_never_shows_subsecond_units() {
    assert_eq!(
        format_duration_at_least(Duration::ZERO, TimeUnit::Second),
        "<1s"
    );
    assert_eq!(
        format_duration_at_least(Duration::from_millis(1), TimeUnit::Second),
        "<1s"
    );
    assert_eq!(
        format_duration_at_least(Duration::from_secs(1), TimeUnit::Second),
        "1s"
    );
}

#[test]
fn format_duration_compact_multi_unit_defaults_to_two_components() {
    assert_eq!(format_duration(Duration::from_millis(1_200)), "1s 200ms");
    assert_eq!(format_duration(Duration::from_secs(61)), "1m 1s");
    assert_eq!(format_duration(Duration::from_secs(3_601)), "1h 1s");
}

#[test]
fn format_duration_with_three_components_includes_more_detail() {
    let options = PrettyDurationOptions {
        max_components: NonZeroUsize::new(3).expect("non-zero"),
        min_unit: None,
    };

    assert_eq!(
        format_duration_with(Duration::from_millis(62_345), options),
        "1m 2s 345ms"
    );
}

#[test]
fn format_duration_monotonic_largest_unit() {
    let options = PrettyDurationOptions {
        max_components: NonZeroUsize::new(2).expect("non-zero"),
        min_unit: None,
    };

    let durations = (0u64..=200_000).step_by(137).map(Duration::from_millis);
    let ranks = durations
        .map(|duration| format_duration_with(duration, options))
        .map(|formatted_duration_string: String| {
            largest_unit_rank(formatted_duration_string.as_str())
        });

    let mut previous = None;
    for current in ranks {
        if let Some(previous) = previous {
            assert!(
                current >= previous,
                "unit regressed: {previous} -> {current}"
            );
        }
        previous = Some(current);
    }
}

#[test]
fn format_system_time_delta_uses_in_and_ago() {
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(100);
    let future = SystemTime::UNIX_EPOCH + Duration::from_secs(160);
    let past = SystemTime::UNIX_EPOCH + Duration::from_secs(40);

    assert_eq!(format_system_time_delta(now, future), "in 1m");
    assert_eq!(format_system_time_delta(now, past), "1m ago");
}
