//! RFC 3339 timestamps for the API.

use std::time::SystemTime;

use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

/// `2026-11-10T08:15:30Z`-style UTC timestamp (whole seconds).
#[must_use]
pub fn rfc3339(at: SystemTime) -> String {
    // Checked: a damaged file can hold any time, and `OffsetDateTime::from`
    // panics outside the years -9999..=9999.
    let seconds = match at.duration_since(SystemTime::UNIX_EPOCH) {
        Ok(after) => i64::try_from(after.as_secs()).ok(),
        Err(before) => i64::try_from(before.duration().as_secs()).ok().map(|s| -s),
    };
    seconds
        .and_then(|seconds| OffsetDateTime::from_unix_timestamp(seconds).ok())
        .and_then(|at| at.format(&Rfc3339).ok())
        .unwrap_or_else(|| "1970-01-01T00:00:00Z".to_owned())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn formats_utc_seconds() {
        let at = SystemTime::UNIX_EPOCH + Duration::from_millis(1_762_762_530_250);

        assert_eq!(rfc3339(at), "2025-11-10T08:15:30Z");
    }

    #[test]
    fn a_time_out_of_range_does_not_panic() {
        // About the year 17800: past what `time` formats (9999), within what
        // `SystemTime` holds on every platform (Windows stops at 30828).
        let far = SystemTime::UNIX_EPOCH + Duration::from_secs(500_000_000_000);

        assert_eq!(rfc3339(far), "1970-01-01T00:00:00Z");
    }
}
