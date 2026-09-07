use crate::clause::ClauseValue;
use crate::schema::SeekType;
use crate::Timestamp;

use super::{ParseError, ParseResult};

pub(super) fn parse_timestamp(value: &str, field: &str) -> ParseResult<ClauseValue> {
    if let Ok(ms) = value.parse::<i64>() {
        return Ok(ClauseValue::Timestamp(Timestamp(ms)));
    }
    if let Some(ts) = parse_date_only(value) {
        return Ok(ClauseValue::Timestamp(ts));
    }
    if let Some(ts) = parse_datetime(value) {
        return Ok(ClauseValue::Timestamp(ts));
    }
    if value.len() == 4 {
        if let Ok(year) = value.parse::<i32>() {
            let days_since_epoch = days_from_year(year);
            let ms = days_since_epoch * 24 * 60 * 60 * 1000;
            return Ok(ClauseValue::Timestamp(Timestamp(ms)));
        }
    }
    Err(ParseError::InvalidValue {
        field: field.to_string(),
        value: value.to_string(),
        expected: SeekType::Timestamp,
        reason: "expected Unix timestamp (ms), ISO date (YYYY-MM-DD), or datetime".to_string(),
    })
}
fn parse_date_only(value: &str) -> Option<Timestamp> {
    let parts: Vec<&str> = value.split('-').collect();
    if parts.len() != 3 {
        return None;
    }
    let year: i32 = parts[0].parse().ok()?;
    let month: u32 = parts[1].parse().ok()?;
    let day: u32 = parts[2].parse().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let days = days_from_ymd(year, month, day)?;
    let ms = days * 24 * 60 * 60 * 1000;
    Some(Timestamp(ms))
}
fn parse_datetime(value: &str) -> Option<Timestamp> {
    let value = value.trim_end_matches('Z');
    let parts: Vec<&str> = value.split('T').collect();
    if parts.len() != 2 {
        return None;
    }
    let date_parts: Vec<&str> = parts[0].split('-').collect();
    let time_parts: Vec<&str> = parts[1].split(':').collect();
    if date_parts.len() != 3 || time_parts.len() != 3 {
        return None;
    }
    let year: i32 = date_parts[0].parse().ok()?;
    let month: u32 = date_parts[1].parse().ok()?;
    let day: u32 = date_parts[2].parse().ok()?;
    let hour: u32 = time_parts[0].parse().ok()?;
    let minute: u32 = time_parts[1].parse().ok()?;
    let second_str = time_parts[2].split('.').next()?;
    let second: u32 = second_str.parse().ok()?;
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour >= 24
        || minute >= 60
        || second >= 60
    {
        return None;
    }
    let days = days_from_ymd(year, month, day)?;
    let seconds = hour * 3600 + minute * 60 + second;
    let ms = days * 24 * 60 * 60 * 1000 + seconds as i64 * 1000;
    Some(Timestamp(ms))
}
fn days_from_year(year: i32) -> i64 {
    let mut days: i64 = 0;
    if year >= 1970 {
        for y in 1970..year {
            days += if is_leap_year(y) { 366 } else { 365 };
        }
    } else {
        for y in year..1970 {
            days -= if is_leap_year(y) { 366 } else { 365 };
        }
    }
    days
}
fn days_from_ymd(year: i32, month: u32, day: u32) -> Option<i64> {
    let days_in_months = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let leap = is_leap_year(year);
    let max_day = if month == 2 && leap {
        29
    } else {
        *days_in_months.get(month as usize - 1)?
    };
    if day > max_day {
        return None;
    }
    let mut days = days_from_year(year);
    for m in 1..month {
        days += days_in_months[m as usize - 1] as i64;
        if m == 2 && leap {
            days += 1;
        }
    }
    days += day as i64 - 1;
    Some(days)
}
fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

#[cfg(test)]
mod tests {
    use super::super::{parse_value, tests::TestTask};
    use super::*;
    use crate::Op;

    #[test]
    fn test_parse_timestamp_unix_ms() {
        let val =
            parse_value::<TestTask>("1705312200000", "created-at", SeekType::Timestamp, Op::Eq)
                .unwrap();
        assert!(matches!(
            val,
            ClauseValue::Timestamp(Timestamp(1705312200000))
        ));
    }

    #[test]
    fn test_parse_timestamp_date_only() {
        let val = parse_value::<TestTask>("2024-01-15", "created-at", SeekType::Timestamp, Op::Eq)
            .unwrap();
        if let ClauseValue::Timestamp(ts) = val {
            assert!(ts.0 > 0);
        } else {
            panic!("Expected Timestamp");
        }
    }

    #[test]
    fn test_parse_timestamp_datetime() {
        let val = parse_value::<TestTask>(
            "2024-01-15T10:30:00Z",
            "created-at",
            SeekType::Timestamp,
            Op::Eq,
        )
        .unwrap();
        if let ClauseValue::Timestamp(ts) = val {
            assert!(ts.0 > 0);
        } else {
            panic!("Expected Timestamp");
        }
    }

    #[test]
    fn test_parse_timestamp_year_only() {
        let val =
            parse_value::<TestTask>("2024", "created-at", SeekType::Timestamp, Op::Eq).unwrap();
        if let ClauseValue::Timestamp(ts) = val {
            assert!(ts.0 > 0);
        } else {
            panic!("Expected Timestamp");
        }
    }

    #[test]
    fn test_parse_timestamp_invalid() {
        let result =
            parse_value::<TestTask>("not-a-date", "created-at", SeekType::Timestamp, Op::Eq);
        assert!(matches!(result, Err(ParseError::InvalidValue { .. })));
    }

    #[test]
    fn test_is_leap_year() {
        assert!(!is_leap_year(1900));
        assert!(is_leap_year(2000));
        assert!(is_leap_year(2024));
        assert!(!is_leap_year(2023));
    }

    #[test]
    fn test_days_from_year() {
        assert_eq!(days_from_year(1970), 0);
        assert_eq!(days_from_year(1971), 365);
        assert_eq!(days_from_year(1972), 365 * 2);
    }

    #[test]
    fn test_days_from_ymd_epoch() {
        assert_eq!(days_from_ymd(1970, 1, 1), Some(0));
    }

    #[test]
    fn test_days_from_ymd_next_day() {
        assert_eq!(days_from_ymd(1970, 1, 2), Some(1));
    }

    #[test]
    fn test_days_from_ymd_invalid() {
        assert_eq!(days_from_ymd(2024, 2, 30), None);
        assert_eq!(days_from_ymd(2024, 13, 1), None);
    }
}
