use chrono::{DateTime, Datelike, Local, TimeZone, Timelike, Utc};
use rquickjs::{class::Trace, function::Rest, Ctx, Exception, JsLifetime, Value};

/// JavaScript Date implementation using chrono
#[derive(Debug, Clone, Trace, JsLifetime)]
#[rquickjs::class]
pub struct Date {
    /// Internal timestamp in milliseconds since Unix epoch
    #[qjs(skip_trace)]
    timestamp_ms: i64,
}

#[rquickjs::methods]
impl Date {
    // ==================== Constructors ====================

    /// new Date() - current time
    /// new Date(value) - milliseconds since epoch
    /// new Date(dateString) - ISO string parsing (simplified)
    /// new Date(year, month, day, hours, minutes, seconds, ms)
    #[qjs(constructor)]
    pub fn new(ctx: Ctx<'_>, args: Rest<Value<'_>>) -> rquickjs::Result<Self> {
        let timestamp_ms = if args.is_empty() {
            // Current time
            Utc::now().timestamp_millis()
        } else if args.len() == 1 {
            // Single argument - timestamp or string
            let arg = &args[0];
            if let Some(num) = arg.as_number() {
                num as i64
            } else if let Some(s) = arg.as_string() {
                // Simplified ISO parsing - try to parse common formats
                Self::parse_date_string(s.to_string()?.as_str())
                    .map_err(|_| Exception::throw_message(&ctx, "Invalid Date"))?
            } else {
                return Err(Exception::throw_message(&ctx, "Invalid Date"));
            }
        } else {
            // Multiple arguments: year, month, day, etc.
            let year = args
                .get(0)
                .and_then(|v| v.as_int().map(|i| i as i32))
                .unwrap_or(0);
            let month = args
                .get(1)
                .and_then(|v| v.as_int().map(|i| i as u32))
                .unwrap_or(0);
            let day = args
                .get(2)
                .and_then(|v| v.as_int().map(|i| i as u32))
                .unwrap_or(1);
            let hour = args
                .get(3)
                .and_then(|v| v.as_int().map(|i| i as u32))
                .unwrap_or(0);
            let minute = args
                .get(4)
                .and_then(|v| v.as_int().map(|i| i as u32))
                .unwrap_or(0);
            let second = args
                .get(5)
                .and_then(|v| v.as_int().map(|i| i as u32))
                .unwrap_or(0);
            let ms = args
                .get(6)
                .and_then(|v| v.as_int().map(|i| i as u32))
                .unwrap_or(0);

            // JS months are 0-indexed, chrono is 1-indexed
            let datetime = Utc
                .with_ymd_and_hms(year, month + 1, day, hour, minute, second)
                .latest()
                .ok_or_else(|| Exception::throw_message(&ctx, "Invalid Date"))?;

            // Add milliseconds (convert to nanoseconds)
            let datetime = datetime.with_nanosecond(ms * 1_000_000).unwrap_or(datetime); // Fallback to original if ms invalid

            datetime.timestamp_millis()
        };

        Ok(Self { timestamp_ms })
    }

    // ==================== Static Methods ====================

    /// Date.now() - current timestamp in milliseconds
    #[qjs(static)]
    pub fn now() -> i64 {
        Utc::now().timestamp_millis()
    }

    /// Date.parse() - parse date string (simplified)
    #[qjs(static)]
    pub fn parse(ctx: Ctx<'_>, date_string: String) -> rquickjs::Result<i64> {
        Self::parse_date_string(&date_string)
            .map_err(|_| Exception::throw_message(&ctx, "Date Error"))
    }

    /// Date.UTC() - create timestamp from UTC components
    #[qjs(static, rename = "UTC")]
    pub fn utc(ctx: Ctx<'_>, args: Rest<Value<'_>>) -> rquickjs::Result<i64> {
        if args.is_empty() {
            return Err(Exception::throw_message(
                &ctx,
                "Date.UTC requires at least one argument",
            ));
        }

        let year = args
            .get(0)
            .and_then(|v| v.as_int().map(|i| i as i32))
            .unwrap_or(0);
        let month = args
            .get(1)
            .and_then(|v| v.as_int().map(|i| i as u32))
            .unwrap_or(0);
        let day = args
            .get(2)
            .and_then(|v| v.as_int().map(|i| i as u32))
            .unwrap_or(1);
        let hour = args
            .get(3)
            .and_then(|v| v.as_int().map(|i| i as u32))
            .unwrap_or(0);
        let minute = args
            .get(4)
            .and_then(|v| v.as_int().map(|i| i as u32))
            .unwrap_or(0);
        let second = args
            .get(5)
            .and_then(|v| v.as_int().map(|i| i as u32))
            .unwrap_or(0);
        let ms = args
            .get(6)
            .and_then(|v| v.as_int().map(|i| i as u32))
            .unwrap_or(0);

        // Handle 2-digit years (0-99 become 1900-1999)
        let year = if year >= 0 && year < 100 {
            1900 + year
        } else {
            year
        };

        let datetime = Utc
            .with_ymd_and_hms(year, month + 1, day, hour, minute, second)
            .latest()
            .ok_or_else(|| Exception::throw_message(&ctx, "Invalid Date"))?;

        // Add milliseconds (convert to nanoseconds)
        let datetime = datetime.with_nanosecond(ms * 1_000_000).unwrap_or(datetime); // Fallback to original if ms invalid

        Ok(datetime.timestamp_millis())
    }

    // ==================== Instance Getters ====================

    /// Get the day of the month (1-31)
    #[qjs(rename = "getDate")]
    pub fn date(&self) -> u32 {
        self.to_local().day()
    }

    /// Get the day of the week (0-6, Sunday is 0)
    #[qjs(rename = "getDay")]
    pub fn day(&self) -> u32 {
        self.to_local().weekday().num_days_from_sunday()
    }

    /// Get the full year (4 digits)
    #[qjs(rename = "getFullYear")]
    pub fn full_year(&self) -> i32 {
        self.to_local().year()
    }

    /// Get the hour (0-23)
    #[qjs(rename = "getHours")]
    pub fn hours(&self) -> u32 {
        self.to_local().hour()
    }

    /// Get the milliseconds (0-999)
    #[qjs(rename = "getMilliseconds")]
    pub fn milliseconds(&self) -> i64 {
        self.timestamp_ms.rem_euclid(1000)
    }

    /// Get the minutes (0-59)
    #[qjs(rename = "getMinutes")]
    pub fn minutes(&self) -> u32 {
        self.to_local().minute()
    }

    /// Get the month (0-11)
    #[qjs(rename = "getMonth")]
    pub fn month(&self) -> u32 {
        self.to_local().month() - 1 // JS months are 0-indexed
    }

    /// Get the seconds (0-59)
    #[qjs(rename = "getSeconds")]
    pub fn seconds(&self) -> u32 {
        self.to_local().second()
    }

    /// Get the time value in milliseconds since epoch
    #[qjs(rename = "getTime")]
    pub fn time(&self) -> i64 {
        self.timestamp_ms
    }

    /// Get the timezone offset in minutes from UTC
    #[qjs(rename = "getTimezoneOffset")]
    pub fn timezone_offset(&self) -> i32 {
        // Calculate offset between local and UTC
        self.to_local().offset().local_minus_utc() / 60
    }

    /// Get the day of the month (1-31) - UTC
    #[qjs(rename = "getUTCDate")]
    pub fn utc_date(&self) -> u32 {
        self.to_utc().day()
    }

    /// Get the day of the week (0-6) - UTC
    #[qjs(rename = "getUTCDay")]
    pub fn utc_day(&self) -> u32 {
        self.to_utc().weekday().num_days_from_sunday()
    }

    /// Get the full year - UTC
    #[qjs(rename = "getUTCFullYear")]
    pub fn utc_full_year(&self) -> i32 {
        self.to_utc().year()
    }

    /// Get the hour - UTC
    #[qjs(rename = "getUTCHours")]
    pub fn utc_hours(&self) -> u32 {
        self.to_utc().hour()
    }

    /// Get the milliseconds - UTC
    #[qjs(rename = "getUTCMilliseconds")]
    pub fn utc_milliseconds(&self) -> i64 {
        self.timestamp_ms.rem_euclid(1000)
    }

    /// Get the minutes - UTC
    #[qjs(rename = "getUTCMinutes")]
    pub fn utc_minutes(&self) -> u32 {
        self.to_utc().minute()
    }

    /// Get the month (0-11) - UTC
    #[qjs(rename = "getUTCMonth")]
    pub fn utc_month(&self) -> u32 {
        self.to_utc().month() - 1
    }

    /// Get the seconds - UTC
    #[qjs(rename = "getUTCSeconds")]
    pub fn utc_seconds(&self) -> u32 {
        self.to_utc().second()
    }

    // ==================== Instance Setters ====================

    /// Set the day of the month
    #[qjs(rename = "setDate")]
    pub fn set_date(&mut self, day: u32) -> i64 {
        let current = self.to_local();
        if let Some(new_date) = current.with_day(day) {
            self.timestamp_ms = new_date.timestamp_millis();
        }
        self.timestamp_ms
    }
    /// Set the full year
    #[qjs(rename = "setFullYear")]
    pub fn set_full_year(&mut self, year: i32) -> i64 {
        let current = self.to_local();
        if let Some(new_date) = current.with_year(year) {
            self.timestamp_ms = new_date.timestamp_millis();
        }
        self.timestamp_ms
    }

    /// Set the hours
    #[qjs(rename = "setHours")]
    pub fn set_hours(&mut self, hours: u32) -> i64 {
        let current = self.to_local();
        if let Some(new_date) = current.with_hour(hours) {
            self.timestamp_ms = new_date.timestamp_millis();
        }
        self.timestamp_ms
    }

    /// Set the milliseconds
    #[qjs(rename = "setMilliseconds")]
    pub fn set_milliseconds(&mut self, ms: i64) -> i64 {
        let current = self.to_local();
        // Convert ms to nanoseconds, handling modulo for valid range
        let ns = (ms.rem_euclid(1000) * 1_000_000) as u32;
        if let Some(new_date) = current.with_nanosecond(ns) {
            self.timestamp_ms = new_date.timestamp_millis();
        }
        self.timestamp_ms
    }

    /// Set the minutes
    #[qjs(rename = "setMinutes")]
    pub fn set_minutes(&mut self, minutes: u32) -> i64 {
        let current = self.to_local();
        if let Some(new_date) = current.with_minute(minutes) {
            self.timestamp_ms = new_date.timestamp_millis();
        }
        self.timestamp_ms
    }

    /// Set the month (0-11)
    #[qjs(rename = "setMonth")]
    pub fn set_month(&mut self, month: u32) -> i64 {
        let current = self.to_local();
        // month is 0-indexed in JS, 1-indexed in chrono
        if let Some(new_date) = current.with_month(month + 1) {
            self.timestamp_ms = new_date.timestamp_millis();
        }
        self.timestamp_ms
    }

    /// Set the seconds
    #[qjs(rename = "setSeconds")]
    pub fn set_seconds(&mut self, seconds: u32) -> i64 {
        let current = self.to_local();
        if let Some(new_date) = current.with_second(seconds) {
            self.timestamp_ms = new_date.timestamp_millis();
        }
        self.timestamp_ms
    }

    /// Set the time value (milliseconds since epoch)
    #[qjs(rename = "setTime")]
    pub fn set_time(&mut self, timestamp: i64) -> i64 {
        self.timestamp_ms = timestamp;
        self.timestamp_ms
    }

    // UTC setters
    #[qjs(rename = "setUTCDate")]
    pub fn set_utc_date(&mut self, day: u32) -> i64 {
        let current = self.to_utc();
        if let Some(new_date) = current.with_day(day) {
            self.timestamp_ms = new_date.timestamp_millis();
        }
        self.timestamp_ms
    }

    #[qjs(rename = "setUTCFullYear")]
    pub fn set_utc_full_year(&mut self, year: i32) -> i64 {
        let current = self.to_utc();
        if let Some(new_date) = current.with_year(year) {
            self.timestamp_ms = new_date.timestamp_millis();
        }
        self.timestamp_ms
    }

    #[qjs(rename = "setUTCHours")]
    pub fn set_utc_hours(&mut self, hours: u32) -> i64 {
        let current = self.to_utc();
        if let Some(new_date) = current.with_hour(hours) {
            self.timestamp_ms = new_date.timestamp_millis();
        }
        self.timestamp_ms
    }

    #[qjs(rename = "setUTCMinutes")]
    pub fn set_utc_minutes(&mut self, minutes: u32) -> i64 {
        let current = self.to_utc();
        if let Some(new_date) = current.with_minute(minutes) {
            self.timestamp_ms = new_date.timestamp_millis();
        }
        self.timestamp_ms
    }

    #[qjs(rename = "setUTCMonth")]
    pub fn set_utc_month(&mut self, month: u32) -> i64 {
        let current = self.to_utc();
        if let Some(new_date) = current.with_month(month + 1) {
            self.timestamp_ms = new_date.timestamp_millis();
        }
        self.timestamp_ms
    }

    #[qjs(rename = "setUTCSeconds")]
    pub fn set_utc_seconds(&mut self, seconds: u32) -> i64 {
        let current = self.to_utc();
        if let Some(new_date) = current.with_second(seconds) {
            self.timestamp_ms = new_date.timestamp_millis();
        }
        self.timestamp_ms
    }

    #[qjs(rename = "setUTCMilliseconds")]
    pub fn set_utc_milliseconds(&mut self, ms: i64) -> i64 {
        let current = self.to_utc();
        // Convert ms to nanoseconds, handling modulo for valid range
        let ns = (ms.rem_euclid(1000) * 1_000_000) as u32;
        if let Some(new_date) = current.with_nanosecond(ns) {
            self.timestamp_ms = new_date.timestamp_millis();
        }
        self.timestamp_ms
    }

    // ==================== Conversion Methods ====================

    /// Convert to ISO 8601 string (UTC)
    #[qjs(rename = "toISOString")]
    pub fn to_iso_string(&self) -> String {
        self.to_utc()
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
    }

    /// Convert to date string (human readable, local timezone)
    #[qjs(rename = "toDateString")]
    pub fn to_date_string(&self) -> String {
        self.to_local().format("%a %b %d %Y").to_string()
    }

    /// Convert to time string (human readable, local timezone)
    #[qjs(rename = "toTimeString")]
    pub fn to_time_string(&self) -> String {
        self.to_local().format("%H:%M:%S GMT%z").to_string()
    }

    /// Convert to UTC string
    #[qjs(rename = "toUTCString")]
    pub fn to_utc_string(&self) -> String {
        self.to_utc()
            .format("%a, %d %b %Y %H:%M:%S GMT")
            .to_string()
    }

    /// Convert to string (alias for toString)
    #[qjs(rename = "toString")]
    pub fn to_js_string(&self) -> String {
        self.to_local()
            .format("%a %b %d %Y %H:%M:%S GMT%z")
            .to_string()
    }

    /// Return primitive value (timestamp)
    #[qjs(rename = "valueOf")]
    pub fn value_of(&self) -> i64 {
        self.timestamp_ms
    }

    /// Convert to JSON (ISO string)
    #[qjs(rename = "toJSON")]
    pub fn to_json(&self) -> String {
        self.to_iso_string()
    }

    // ==================== Non-standard Extensions ====================

    /// Format the date using strftime format string (local time)
    pub fn strftime(&self, format: String) -> String {
        self.to_local().format(&format).to_string()
    }

    /// Format the date using strftime format string (UTC)
    ///
    /// Same format specifiers as strftime(), but uses UTC time.
    pub fn strftime_utc(&self, format: String) -> String {
        self.to_utc().format(&format).to_string()
    }
}

impl Date {
    /// Parse a date string (simplified implementation)
    fn parse_date_string(s: &str) -> Result<i64, String> {
        // Try RFC 3339 / ISO 8601 first
        if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
            return Ok(dt.timestamp_millis());
        }

        // Try common formats
        let formats = [
            "%Y-%m-%d",
            "%Y/%m/%d",
            "%m/%d/%Y",
            "%d-%m-%Y",
            "%Y-%m-%d %H:%M:%S",
            "%Y-%m-%dT%H:%M:%S",
            "%a, %d %b %Y %H:%M:%S GMT", // HTTP date
        ];

        for fmt in &formats {
            if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(s, fmt) {
                return Ok(Utc.from_utc_datetime(&naive).timestamp_millis());
            }
            if let Ok(date) = chrono::NaiveDate::parse_from_str(s, fmt) {
                return Ok(Utc
                    .from_utc_datetime(&date.and_hms_opt(0, 0, 0).unwrap())
                    .timestamp_millis());
            }
        }

        Err("Invalid date format".to_string())
    }

    /// Convert timestamp to local DateTime
    fn to_local(&self) -> DateTime<Local> {
        Local
            .timestamp_millis_opt(self.timestamp_ms)
            .latest()
            .unwrap_or_else(|| Local::now())
    }

    /// Convert timestamp to UTC DateTime
    fn to_utc(&self) -> DateTime<Utc> {
        Utc.timestamp_millis_opt(self.timestamp_ms)
            .latest()
            .unwrap_or_else(|| Utc::now())
    }
}

/// Register the Date class with the runtime
pub fn register_date(ctx: &Ctx) -> rquickjs::Result<()> {
    rquickjs::Class::<Date>::define(&ctx.globals())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rquickjs::{Context, Runtime};

    // Fixed timestamp: 2024-01-15T10:30:45.123Z (Monday)
    const TEST_TS: i64 = 1705314645123;

    fn setup_date(ts: i64) -> Date {
        Date { timestamp_ms: ts }
    }

    #[test]
    fn test_utc_getters() {
        let date = setup_date(TEST_TS);
        assert_eq!(date.time(), TEST_TS);
        assert_eq!(date.utc_full_year(), 2024);
        assert_eq!(date.utc_month(), 0); // January = 0
        assert_eq!(date.utc_date(), 15);
        assert_eq!(date.utc_hours(), 10);
        assert_eq!(date.utc_minutes(), 30);
        assert_eq!(date.utc_seconds(), 45);
        assert_eq!(date.utc_milliseconds(), 123);
        assert_eq!(date.utc_day(), 1); // Monday
    }

    #[test]
    fn test_to_iso_string() {
        let date = setup_date(TEST_TS);
        assert_eq!(date.to_iso_string(), "2024-01-15T10:30:45.123Z");
    }

    #[test]
    fn test_value_of_and_json() {
        let date = setup_date(TEST_TS);
        assert_eq!(date.value_of(), TEST_TS);
        assert_eq!(date.to_json(), "2024-01-15T10:30:45.123Z");
    }

    #[test]
    fn test_static_now() {
        let before = Utc::now().timestamp_millis();
        let now = Date::now();
        let after = Utc::now().timestamp_millis();
        assert!(now >= before && now <= after);
    }

    #[test]
    fn test_js_runtime_integration() {
        let rt = Runtime::new().unwrap();
        let ctx = Context::full(&rt).unwrap();

        ctx.with(|ctx| {
            register_date(&ctx).unwrap();

            let result: i64 = ctx.eval(r#"new Date(1705314645123).getTime();"#).unwrap();
            assert_eq!(result, TEST_TS);

            let year: i32 = ctx
                .eval(r#"new Date(1705314645123).getUTCFullYear();"#)
                .unwrap();
            assert_eq!(year, 2024);

            let month: u32 = ctx
                .eval(r#"new Date(1705314645123).getUTCMonth();"#)
                .unwrap();
            assert_eq!(month, 0);

            let iso: String = ctx
                .eval(r#"new Date(1705314645123).toISOString();"#)
                .unwrap();
            assert_eq!(iso, "2024-01-15T10:30:45.123Z");

            let strftime: String = ctx
                .eval(r#"new Date(1705314645123).strftime("%Y-%m-%d %T");"#)
                .unwrap();
            assert_eq!(strftime, "2024-01-15 10:30:45");
        });
    }
    #[test]
    fn test_constructor_with_components() {
        let rt = Runtime::new().unwrap();
        let ctx = Context::full(&rt).unwrap();

        ctx.with(|ctx| {
            register_date(&ctx).unwrap();

            // Test new Date(year, month, day, hours, minutes, seconds, ms)
            // Note: January 15, 2024 10:30:45.123 UTC
            // Month is 0-indexed in JS, so 0 = January
            let ts: i64 = ctx
                .eval(r#"new Date(2024, 0, 15, 10, 30, 45, 123).getTime()"#)
                .unwrap();
            assert_eq!(ts, TEST_TS);

            // This creates the date in local time, so we need to verify against local expectation
            // For a robust test, check that the components round-trip correctly
            let year: i32 = ctx
                .eval(r#"new Date(2024, 0, 15, 10, 30, 45, 123).getFullYear()"#)
                .unwrap();
            assert_eq!(year, 2024);

            let month: u32 = ctx
                .eval(r#"new Date(2024, 0, 15, 10, 30, 45, 123).getMonth()"#)
                .unwrap();
            assert_eq!(month, 0);

            let day: u32 = ctx
                .eval(r#"new Date(2024, 0, 15, 10, 30, 45, 123).getDate()"#)
                .unwrap();
            assert_eq!(day, 15);

            let hours: u32 = ctx
                .eval(r#"new Date(2024, 0, 15, 10, 30, 45, 123).getHours()"#)
                .unwrap();
            assert_eq!(hours, 10);

            let minutes: u32 = ctx
                .eval(r#"new Date(2024, 0, 15, 10, 30, 45, 123).getMinutes()"#)
                .unwrap();
            assert_eq!(minutes, 30);

            let seconds: u32 = ctx
                .eval(r#"new Date(2024, 0, 15, 10, 30, 45, 123).getSeconds()"#)
                .unwrap();
            assert_eq!(seconds, 45);

            let ms: i64 = ctx
                .eval(r#"new Date(2024, 0, 15, 10, 30, 45, 123).getMilliseconds()"#)
                .unwrap();
            assert_eq!(ms, 123);
        });
    }

    #[test]
    fn test_utc_static_method() {
        let rt = Runtime::new().unwrap();
        let ctx = Context::full(&rt).unwrap();

        ctx.with(|ctx| {
            register_date(&ctx).unwrap();

            // Test Date.UTC(year, month, day, hours, minutes, seconds, ms)
            // Returns timestamp directly
            let ts: i64 = ctx
                .eval(r#"Date.UTC(2024, 0, 15, 10, 30, 45, 123)"#)
                .unwrap();

            // This should equal our test timestamp
            assert_eq!(ts, TEST_TS);

            // Test with minimal arguments (year, month only)
            let ts_minimal: i64 = ctx.eval(r#"Date.UTC(2024, 0)"#).unwrap();

            // Should be 2024-01-01 00:00:00.000 UTC
            let expected_minimal = Utc
                .with_ymd_and_hms(2024, 1, 1, 0, 0, 0)
                .latest()
                .unwrap()
                .timestamp_millis();
            assert_eq!(ts_minimal, expected_minimal);

            // Test 2-digit year handling (0-99 → 1900-1999)
            let ts_2digit: i64 = ctx.eval(r#"Date.UTC(99, 11, 31)"#).unwrap();

            let expected_2digit = Utc
                .with_ymd_and_hms(1999, 12, 31, 0, 0, 0)
                .latest()
                .unwrap()
                .timestamp_millis();
            assert_eq!(ts_2digit, expected_2digit);

            // Test that Date.UTC and new Date(Date.UTC(...)) give consistent results
            let utc_ts: i64 = ctx.eval(r#"Date.UTC(2024, 5, 15, 12, 0, 0, 0)"#).unwrap();

            let from_utc: i64 = ctx
                .eval(r#"new Date(Date.UTC(2024, 5, 15, 12, 0, 0, 0)).getTime()"#)
                .unwrap();

            assert_eq!(utc_ts, from_utc);
        });
    }

    #[test]
    fn test_constructor_utc_roundtrip() {
        let rt = Runtime::new().unwrap();
        let ctx = Context::full(&rt).unwrap();

        ctx.with(|ctx| {
            register_date(&ctx).unwrap();

            // Create date using Date.UTC and verify UTC getters
            let utc_year: i32 = ctx
                .eval(r#"new Date(Date.UTC(2024, 0, 15, 10, 30, 45, 123)).getUTCFullYear()"#)
                .unwrap();
            assert_eq!(utc_year, 2024);

            let utc_month: u32 = ctx
                .eval(r#"new Date(Date.UTC(2024, 0, 15, 10, 30, 45, 123)).getUTCMonth()"#)
                .unwrap();
            assert_eq!(utc_month, 0);

            let utc_day: u32 = ctx
                .eval(r#"new Date(Date.UTC(2024, 0, 15, 10, 30, 45, 123)).getUTCDate()"#)
                .unwrap();
            assert_eq!(utc_day, 15);

            let utc_hours: u32 = ctx
                .eval(r#"new Date(Date.UTC(2024, 0, 15, 10, 30, 45, 123)).getUTCHours()"#)
                .unwrap();
            assert_eq!(utc_hours, 10);

            let utc_ms: i64 = ctx
                .eval(r#"new Date(Date.UTC(2024, 0, 15, 10, 30, 45, 123)).getUTCMilliseconds()"#)
                .unwrap();
            assert_eq!(utc_ms, 123);
        });
    }
}
