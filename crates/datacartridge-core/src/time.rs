//! The three clocks (§48) and the vocabulary of time.
//!
//! * **event time** — when something happened in the world;
//! * **knowledge time** — when the information became available to us;
//! * **processing time** — when some computation acted on it.
//!
//! All instants are UTC nanoseconds. Nothing in the platform reads the wall clock except
//! through a [`Clock`], which is what lets historical replay and backtests run the same
//! code as live operation under a controlled knowledge boundary.

use std::fmt;
use std::str::FromStr;
use std::sync::atomic::{AtomicI64, Ordering};

use chrono::{DateTime, Datelike, NaiveDate, SecondsFormat, TimeZone, Utc};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

pub const NANOS_PER_SECOND: i64 = 1_000_000_000;
pub const NANOS_PER_DAY: i64 = 86_400 * NANOS_PER_SECOND;

/// An instant: nanoseconds since the Unix epoch, UTC.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Timestamp(i64);

impl Timestamp {
    pub const MIN: Timestamp = Timestamp(i64::MIN);
    pub const MAX: Timestamp = Timestamp(i64::MAX);

    pub const fn from_nanos(nanos: i64) -> Self {
        Self(nanos)
    }

    pub const fn as_nanos(self) -> i64 {
        self.0
    }

    pub fn from_millis(millis: i64) -> Result<Self> {
        millis
            .checked_mul(1_000_000)
            .map(Self)
            .ok_or_else(|| Error::invalid("timestamp", format!("{millis}ms is out of range")))
    }

    pub fn from_seconds(seconds: i64) -> Result<Self> {
        seconds
            .checked_mul(NANOS_PER_SECOND)
            .map(Self)
            .ok_or_else(|| Error::invalid("timestamp", format!("{seconds}s is out of range")))
    }

    pub fn from_ymd(year: i32, month: u32, day: u32) -> Result<Self> {
        let date = NaiveDate::from_ymd_opt(year, month, day).ok_or_else(|| {
            Error::invalid("date", format!("{year:04}-{month:02}-{day:02} does not exist"))
        })?;
        Self::from_date(date)
    }

    pub fn from_date(date: NaiveDate) -> Result<Self> {
        let dt = Utc.from_utc_datetime(&date.and_hms_opt(0, 0, 0).expect("midnight exists"));
        Self::from_datetime(dt)
    }

    pub fn from_datetime(dt: DateTime<Utc>) -> Result<Self> {
        dt.timestamp_nanos_opt()
            .map(Self)
            .ok_or_else(|| Error::invalid("timestamp", format!("{dt} is out of range")))
    }

    pub fn to_datetime(self) -> DateTime<Utc> {
        Utc.timestamp_nanos(self.0)
    }

    pub fn date(self) -> NaiveDate {
        self.to_datetime().date_naive()
    }

    /// Parses RFC 3339 (`2030-10-29T20:03:21Z`) or a bare date (`2021-03-14`, midnight UTC).
    pub fn parse(s: &str) -> Result<Self> {
        let s = s.trim();
        if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
            return Self::from_datetime(dt.with_timezone(&Utc));
        }
        if let Ok(date) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
            return Self::from_date(date);
        }
        Err(Error::invalid("timestamp", format!("'{s}' is neither RFC 3339 nor YYYY-MM-DD")))
    }

    pub fn format_date_compact(self) -> String {
        self.to_datetime().format("%Y%m%d").to_string()
    }

    pub fn checked_add(self, d: Duration) -> Result<Self> {
        self.0
            .checked_add(d.0)
            .map(Self)
            .ok_or_else(|| Error::invalid("timestamp", "arithmetic overflow".to_string()))
    }

    pub fn checked_sub(self, d: Duration) -> Result<Self> {
        self.0
            .checked_sub(d.0)
            .map(Self)
            .ok_or_else(|| Error::invalid("timestamp", "arithmetic overflow".to_string()))
    }

    /// Signed duration from `earlier` to `self`.
    pub fn since(self, earlier: Timestamp) -> Duration {
        Duration(self.0.saturating_sub(earlier.0))
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_datetime().to_rfc3339_opts(SecondsFormat::AutoSi, true))
    }
}

impl fmt::Debug for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl FromStr for Timestamp {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self> {
        Self::parse(s)
    }
}

impl Serialize for Timestamp {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Timestamp {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Timestamp::parse(&s).map_err(serde::de::Error::custom)
    }
}

/// A signed span of nanoseconds.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug, Serialize, Deserialize)]
pub struct Duration(i64);

impl Duration {
    pub const ZERO: Duration = Duration(0);

    pub const fn from_nanos(n: i64) -> Self {
        Self(n)
    }
    pub const fn from_micros(n: i64) -> Self {
        Self(n * 1_000)
    }
    pub const fn from_millis(n: i64) -> Self {
        Self(n * 1_000_000)
    }
    pub const fn from_seconds(n: i64) -> Self {
        Self(n * NANOS_PER_SECOND)
    }
    pub const fn from_minutes(n: i64) -> Self {
        Self(n * 60 * NANOS_PER_SECOND)
    }
    pub const fn from_hours(n: i64) -> Self {
        Self(n * 3_600 * NANOS_PER_SECOND)
    }
    pub const fn from_days(n: i64) -> Self {
        Self(n * NANOS_PER_DAY)
    }
    pub const fn as_nanos(self) -> i64 {
        self.0
    }
    pub fn as_seconds_f64(self) -> f64 {
        self.0 as f64 / NANOS_PER_SECOND as f64
    }
    pub fn as_days_f64(self) -> f64 {
        self.0 as f64 / NANOS_PER_DAY as f64
    }
}

/// Half-open validity interval `[from, to)` in event time. `to == None` means open-ended.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub struct Validity {
    pub from: Timestamp,
    pub to: Option<Timestamp>,
}

impl Validity {
    pub fn new(from: Timestamp, to: Option<Timestamp>) -> Result<Self> {
        if let Some(to) = to
            && to <= from
        {
            return Err(Error::invalid(
                "validity",
                format!("interval end {to} must be after start {from}"),
            ));
        }
        Ok(Self { from, to })
    }

    pub fn starting(from: Timestamp) -> Self {
        Self { from, to: None }
    }

    pub fn contains(&self, at: Timestamp) -> bool {
        self.from <= at && self.to.is_none_or(|to| at < to)
    }

    pub fn overlaps(&self, other: &Validity) -> bool {
        let self_ends_before = self.to.is_some_and(|to| to <= other.from);
        let other_ends_before = other.to.is_some_and(|to| to <= self.from);
        !self_ends_before && !other_ends_before
    }
}

/// A reporting period qualifying a fact's subject ("Apple FY2027").
///
/// Fiscal periods are labels assigned by the reporting entity; their actual calendar span
/// lives in the fact's [`Validity`]. Calendar periods have a definite span.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub enum Period {
    FiscalYear { year: i32 },
    FiscalQuarter { year: i32, quarter: u8 },
    CalendarYear { year: i32 },
    CalendarQuarter { year: i32, quarter: u8 },
    Month { year: i32, month: u8 },
    Day { date: NaiveDate },
}

impl Period {
    pub fn fiscal_quarter(year: i32, quarter: u8) -> Result<Self> {
        check_quarter(quarter)?;
        Ok(Period::FiscalQuarter { year, quarter })
    }

    pub fn calendar_quarter(year: i32, quarter: u8) -> Result<Self> {
        check_quarter(quarter)?;
        Ok(Period::CalendarQuarter { year, quarter })
    }

    pub fn month(year: i32, month: u8) -> Result<Self> {
        if !(1..=12).contains(&month) {
            return Err(Error::invalid("period", format!("month {month} is not in 1..=12")));
        }
        Ok(Period::Month { year, month })
    }

    /// The calendar span of the period, when it has a definite one.
    pub fn calendar_span(&self) -> Result<Option<Validity>> {
        let span = |y1: i32, m1: u32, y2: i32, m2: u32| -> Result<Option<Validity>> {
            Ok(Some(Validity::new(
                Timestamp::from_ymd(y1, m1, 1)?,
                Some(Timestamp::from_ymd(y2, m2, 1)?),
            )?))
        };
        match *self {
            Period::FiscalYear { .. } | Period::FiscalQuarter { .. } => Ok(None),
            Period::CalendarYear { year } => span(year, 1, year + 1, 1),
            Period::CalendarQuarter { year, quarter } => {
                let start = (quarter as u32 - 1) * 3 + 1;
                if quarter == 4 { span(year, start, year + 1, 1) } else { span(year, start, year, start + 3) }
            }
            Period::Month { year, month } => {
                if month == 12 {
                    span(year, 12, year + 1, 1)
                } else {
                    span(year, month as u32, year, month as u32 + 1)
                }
            }
            Period::Day { date } => {
                let from = Timestamp::from_date(date)?;
                Ok(Some(Validity::new(from, Some(from.checked_add(Duration::from_days(1))?))?))
            }
        }
    }
}

fn check_quarter(quarter: u8) -> Result<()> {
    if (1..=4).contains(&quarter) {
        Ok(())
    } else {
        Err(Error::invalid("period", format!("quarter {quarter} is not in 1..=4")))
    }
}

impl fmt::Display for Period {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Period::FiscalYear { year } => write!(f, "FY{year}"),
            Period::FiscalQuarter { year, quarter } => write!(f, "FY{year}Q{quarter}"),
            Period::CalendarYear { year } => write!(f, "CY{year}"),
            Period::CalendarQuarter { year, quarter } => write!(f, "CY{year}Q{quarter}"),
            Period::Month { year, month } => write!(f, "{year:04}-{month:02}"),
            Period::Day { date } => {
                write!(f, "{:04}-{:02}-{:02}", date.year(), date.month(), date.day())
            }
        }
    }
}

impl FromStr for Period {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        let bad = || Error::invalid("period", format!("'{s}' is not a recognized period"));
        let s = s.trim();
        let fiscal = s.strip_prefix("FY").map(|r| (true, r));
        let calendar = s.strip_prefix("CY").map(|r| (false, r));
        if let Some((is_fiscal, rest)) = fiscal.or(calendar) {
            let (year_s, quarter_s) = match rest.split_once('Q') {
                Some((y, q)) => (y, Some(q)),
                None => (rest, None),
            };
            if year_s.len() != 4 {
                return Err(bad());
            }
            let year: i32 = year_s.parse().map_err(|_| bad())?;
            return match quarter_s {
                None if is_fiscal => Ok(Period::FiscalYear { year }),
                None => Ok(Period::CalendarYear { year }),
                Some(q) => {
                    let quarter: u8 = q.parse().map_err(|_| bad())?;
                    if is_fiscal {
                        Period::fiscal_quarter(year, quarter)
                    } else {
                        Period::calendar_quarter(year, quarter)
                    }
                }
            };
        }
        if let Ok(date) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
            return Ok(Period::Day { date });
        }
        if let Some((y, m)) = s.split_once('-')
            && y.len() == 4
            && m.len() == 2
        {
            let year: i32 = y.parse().map_err(|_| bad())?;
            let month: u8 = m.parse().map_err(|_| bad())?;
            return Period::month(year, month);
        }
        Err(bad())
    }
}

impl Serialize for Period {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Period {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

/// Source of processing time. Live operation uses [`SystemClock`]; replay and backtests
/// use [`SimClock`], so code under test cannot observe the real present.
pub trait Clock: Send + Sync {
    fn now(&self) -> Timestamp;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Timestamp {
        Timestamp::from_datetime(Utc::now()).expect("system time is within the representable range")
    }
}

/// A manually advanced clock. Time never moves backwards.
#[derive(Debug)]
pub struct SimClock {
    nanos: AtomicI64,
}

impl SimClock {
    pub fn new(start: Timestamp) -> Self {
        Self { nanos: AtomicI64::new(start.as_nanos()) }
    }

    pub fn advance_to(&self, to: Timestamp) -> Result<()> {
        let prev = self.nanos.fetch_max(to.as_nanos(), Ordering::SeqCst);
        if prev > to.as_nanos() {
            return Err(Error::illegal(
                "clock",
                format!("cannot move from {} back to {to}", Timestamp::from_nanos(prev)),
            ));
        }
        Ok(())
    }
}

impl Clock for SimClock {
    fn now(&self) -> Timestamp {
        Timestamp::from_nanos(self.nanos.load(Ordering::SeqCst))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_format_round_trip() {
        let t = Timestamp::parse("2030-10-29T20:03:21Z").unwrap();
        assert_eq!(t.to_string(), "2030-10-29T20:03:21Z");
        let with_offset = Timestamp::parse("2030-10-29T22:03:21+02:00").unwrap();
        assert_eq!(t, with_offset);
        let nanos = Timestamp::parse("2030-10-29T20:03:21.000000084Z").unwrap();
        assert_eq!(nanos.since(t), Duration::from_nanos(84));
        assert_eq!(nanos.to_string().parse::<Timestamp>().unwrap(), nanos);
        assert_eq!(
            Timestamp::parse("2021-03-14").unwrap(),
            Timestamp::parse("2021-03-14T00:00:00Z").unwrap()
        );
        assert!(Timestamp::parse("14/03/2021").is_err());
        assert!(Timestamp::from_ymd(2021, 2, 30).is_err());
    }

    #[test]
    fn validity_is_half_open() {
        let a = Timestamp::parse("2020-01-01").unwrap();
        let b = Timestamp::parse("2021-01-01").unwrap();
        let v = Validity::new(a, Some(b)).unwrap();
        assert!(v.contains(a));
        assert!(!v.contains(b));
        assert!(Validity::new(b, Some(a)).is_err());
        assert!(Validity::new(a, Some(a)).is_err());
        let next = Validity::starting(b);
        assert!(!v.overlaps(&next));
        assert!(v.overlaps(&Validity::starting(a)));
    }

    #[test]
    fn period_round_trip_and_span() {
        for s in ["FY2027", "FY2027Q3", "CY2024", "CY2024Q4", "2027-03", "2027-03-14"] {
            assert_eq!(s.parse::<Period>().unwrap().to_string(), s);
        }
        for s in ["FY27", "FY2027Q5", "2027-13", "Q3", ""] {
            assert!(s.parse::<Period>().is_err(), "{s} should be rejected");
        }
        let q4 = Period::calendar_quarter(2024, 4).unwrap().calendar_span().unwrap().unwrap();
        assert_eq!(q4.from, Timestamp::parse("2024-10-01").unwrap());
        assert_eq!(q4.to, Some(Timestamp::parse("2025-01-01").unwrap()));
        let dec = Period::month(2024, 12).unwrap().calendar_span().unwrap().unwrap();
        assert_eq!(dec.to, Some(Timestamp::parse("2025-01-01").unwrap()));
        assert!(Period::FiscalYear { year: 2027 }.calendar_span().unwrap().is_none());
    }

    #[test]
    fn sim_clock_is_monotonic() {
        let t0 = Timestamp::parse("2008-09-15T09:00:00Z").unwrap();
        let clock = SimClock::new(t0);
        let t1 = t0.checked_add(Duration::from_seconds(5)).unwrap();
        clock.advance_to(t1).unwrap();
        assert_eq!(clock.now(), t1);
        assert!(clock.advance_to(t0).is_err());
        assert_eq!(clock.now(), t1);
    }
}
