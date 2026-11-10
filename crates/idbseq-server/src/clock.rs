//! RFC 3339 timestamps for the API.

use std::time::SystemTime;

use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

/// `2026-11-10T08:15:30Z`-style UTC timestamp (whole seconds).
#[must_use]
pub fn rfc3339(at: SystemTime) -> String {
    let at = OffsetDateTime::from(at);
    let at = at.replace_nanosecond(0).unwrap_or(at);
    at.format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_owned())
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
}
