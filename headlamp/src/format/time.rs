use std::num::NonZeroUsize;
use std::time::{Duration, Instant, SystemTime};

const NANOS_PER_MICROSECOND: u128 = 1_000;
const NANOS_PER_MILLISECOND: u128 = 1_000_000;
const NANOS_PER_SECOND: u128 = 1_000_000_000;
const NANOS_PER_MINUTE: u128 = 60 * NANOS_PER_SECOND;
const NANOS_PER_HOUR: u128 = 60 * NANOS_PER_MINUTE;
const NANOS_PER_DAY: u128 = 24 * NANOS_PER_HOUR;

#[derive(Clone, Copy)]
pub struct PrettyDurationOptions {
    pub max_components: NonZeroUsize,
    pub min_unit: Option<TimeUnit>,
}

impl Default for PrettyDurationOptions {
    fn default() -> Self {
        Self {
            max_components: NonZeroUsize::new(2).expect("non-zero"),
            min_unit: None,
        }
    }
}

#[derive(Clone, Copy)]
pub enum TimeUnit {
    Nanosecond,
    Microsecond,
    Millisecond,
    Second,
    Minute,
    Hour,
    Day,
}

pub fn format_duration(duration: Duration) -> String {
    format_duration_with(duration, PrettyDurationOptions::default())
}

pub fn format_duration_at_least(duration: Duration, min_unit: TimeUnit) -> String {
    format_duration_with(
        duration,
        PrettyDurationOptions {
            min_unit: Some(min_unit),
            ..PrettyDurationOptions::default()
        },
    )
}

pub fn format_duration_with(duration: Duration, options: PrettyDurationOptions) -> String {
    if duration.is_zero() {
        return format_less_than_one(options.min_unit.unwrap_or(TimeUnit::Nanosecond));
    }
    let total_nanos = duration_to_nanos(duration);
    let max_components = options.max_components.get();

    if let Some(min_unit) = options.min_unit {
        if total_nanos < unit_nanos(min_unit) {
            return format_less_than_one(min_unit);
        }
    }

    let (first_unit, first_unit_nanos) = select_largest_unit(total_nanos);
    let first_value = total_nanos / first_unit_nanos;
    let remainder_after_first = total_nanos % first_unit_nanos;

    if max_components <= 1 || remainder_after_first == 0 {
        return format_single_component(first_value, first_unit);
    }

    let (second_unit, second_unit_nanos) = select_largest_unit(remainder_after_first);
    let second_value = remainder_after_first / second_unit_nanos;

    if second_value == 0 {
        return format_single_component(first_value, first_unit);
    }

    if max_components == 2 {
        return format_two_components(first_value, first_unit, second_value, second_unit);
    }

    let remainder_after_second = remainder_after_first % second_unit_nanos;
    if remainder_after_second == 0 {
        return format_two_components(first_value, first_unit, second_value, second_unit);
    }

    let (third_unit, third_unit_nanos) = select_largest_unit(remainder_after_second);
    let third_value = remainder_after_second / third_unit_nanos;

    if third_value == 0 {
        return format_two_components(first_value, first_unit, second_value, second_unit);
    }

    format_three_components(
        first_value,
        first_unit,
        second_value,
        second_unit,
        third_value,
        third_unit,
    )
}

pub fn format_system_time_delta(now: SystemTime, then: SystemTime) -> String {
    match then.duration_since(now) {
        Ok(delta) => format!("in {}", format_duration(delta)),
        Err(error) => format!("{} ago", format_duration(error.duration())),
    }
}

pub fn format_instant_elapsed(start: Instant) -> String {
    format_duration(start.elapsed())
}

fn duration_to_nanos(duration: Duration) -> u128 {
    u128::from(duration.subsec_nanos()) + (duration.as_secs() as u128) * NANOS_PER_SECOND
}

fn unit_nanos(unit: TimeUnit) -> u128 {
    match unit {
        TimeUnit::Nanosecond => 1,
        TimeUnit::Microsecond => NANOS_PER_MICROSECOND,
        TimeUnit::Millisecond => NANOS_PER_MILLISECOND,
        TimeUnit::Second => NANOS_PER_SECOND,
        TimeUnit::Minute => NANOS_PER_MINUTE,
        TimeUnit::Hour => NANOS_PER_HOUR,
        TimeUnit::Day => NANOS_PER_DAY,
    }
}

fn select_largest_unit(total_nanos: u128) -> (TimeUnit, u128) {
    if total_nanos >= NANOS_PER_DAY {
        return (TimeUnit::Day, NANOS_PER_DAY);
    }
    if total_nanos >= NANOS_PER_HOUR {
        return (TimeUnit::Hour, NANOS_PER_HOUR);
    }
    if total_nanos >= NANOS_PER_MINUTE {
        return (TimeUnit::Minute, NANOS_PER_MINUTE);
    }
    if total_nanos >= NANOS_PER_SECOND {
        return (TimeUnit::Second, NANOS_PER_SECOND);
    }
    if total_nanos >= NANOS_PER_MILLISECOND {
        return (TimeUnit::Millisecond, NANOS_PER_MILLISECOND);
    }
    if total_nanos >= NANOS_PER_MICROSECOND {
        return (TimeUnit::Microsecond, NANOS_PER_MICROSECOND);
    }
    (TimeUnit::Nanosecond, 1)
}

fn format_less_than_one(unit: TimeUnit) -> String {
    let mut output = String::with_capacity(8);
    output.push_str("<1");
    output.push_str(unit_suffix(unit));
    output
}

fn format_single_component(value: u128, unit: TimeUnit) -> String {
    let mut output = String::with_capacity(24);
    push_component(&mut output, value, unit);
    output
}

fn format_two_components(
    first_value: u128,
    first_unit: TimeUnit,
    second_value: u128,
    second_unit: TimeUnit,
) -> String {
    let mut output = String::with_capacity(32);
    push_component(&mut output, first_value, first_unit);
    output.push(' ');
    push_component(&mut output, second_value, second_unit);
    output
}

fn format_three_components(
    first_value: u128,
    first_unit: TimeUnit,
    second_value: u128,
    second_unit: TimeUnit,
    third_value: u128,
    third_unit: TimeUnit,
) -> String {
    let mut output = String::with_capacity(40);
    push_component(&mut output, first_value, first_unit);
    output.push(' ');
    push_component(&mut output, second_value, second_unit);
    output.push(' ');
    push_component(&mut output, third_value, third_unit);
    output
}

fn push_component(output: &mut String, value: u128, unit: TimeUnit) {
    output.push_str(&value.to_string());
    output.push_str(unit_suffix(unit));
}

fn unit_suffix(unit: TimeUnit) -> &'static str {
    match unit {
        TimeUnit::Nanosecond => "ns",
        TimeUnit::Microsecond => "µs",
        TimeUnit::Millisecond => "ms",
        TimeUnit::Second => "s",
        TimeUnit::Minute => "m",
        TimeUnit::Hour => "h",
        TimeUnit::Day => "d",
    }
}
