//! Time texts: `{when}` ("5 min ago"), `{time}` ("Tue 14:05 (UTC+1)"), `{left}`
//! ("about 22 hours"), `{secs}` ("0.4") and the heartbeat age ("now"). The
//! rules and the words (`time.*` in strings.en.json) are the same in the
//! macOS, Windows and Linux apps.
//!
//! No date library: the GTK app reads the local UTC offset from GLib and
//! stores it with [`set_local_offset_secs`]; the functions here are pure and
//! take the offset explicitly where a test needs it.

use crate::strings::{tr, tr_list, trf};
use std::sync::atomic::{AtomicI32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static LOCAL_OFFSET_SECS: AtomicI32 = AtomicI32::new(0);

/// Set the local UTC offset (seconds east of UTC). Called by the GUI at
/// startup and once a minute (daylight saving changes).
pub fn set_local_offset_secs(secs: i32) {
    LOCAL_OFFSET_SECS.store(secs, Ordering::Relaxed);
}

pub fn local_offset_secs() -> i32 {
    LOCAL_OFFSET_SECS.load(Ordering::Relaxed)
}

/// Milliseconds since the Unix epoch.
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

const MIN: u64 = 60_000;
const HOUR: u64 = 60 * MIN;
const DAY: u64 = 24 * HOUR;

/// `{when}` for something in the past: "just now", "5 min ago", "3 h ago",
/// "1 day ago", "2 days ago" (the same rules in all three desktop apps).
pub fn relative_past(now_ms: u64, at_ms: u64) -> String {
    let age = now_ms.saturating_sub(at_ms);
    if age < MIN {
        tr("time.justNow").to_string()
    } else if age < HOUR {
        trf("time.minutesAgo", &[("n", &(age / MIN).to_string())])
    } else if age < DAY {
        trf("time.hoursAgo", &[("n", &(age / HOUR).to_string())])
    } else if age / DAY == 1 {
        tr("time.dayAgo").to_string()
    } else {
        trf("time.daysAgo", &[("n", &(age / DAY).to_string())])
    }
}

/// The heartbeat age in "node ok · {when}": "now" for under 3 s, then
/// seconds, then minutes.
pub fn heartbeat_age(now_ms: u64, last_ok_ms: u64) -> String {
    let age = now_ms.saturating_sub(last_ok_ms);
    if age < 3_000 {
        tr("time.now").to_string()
    } else if age < MIN {
        trf("time.seconds", &[("n", &(age / 1000).to_string())])
    } else {
        relative_past(now_ms, last_ok_ms)
    }
}

/// Days since the epoch and minutes into that day, in local time.
fn local_parts(at_ms: u64, offset_secs: i32) -> (i64, u32) {
    let local_secs = (at_ms / 1000) as i64 + offset_secs as i64;
    let days = local_secs.div_euclid(86_400);
    let minute_of_day = (local_secs.rem_euclid(86_400) / 60) as u32;
    (days, minute_of_day)
}

/// Monday = 0. 1970-01-01 was a Thursday.
fn weekday(days_since_epoch: i64) -> usize {
    (days_since_epoch + 3).rem_euclid(7) as usize
}

/// This computer's zone as the CLI names it after a time (`ash` ext.rs
/// `zone_label`): "UTC", "UTC+1", "UTC-5", "UTC+5:30".
pub fn zone_label(offset_secs: i32) -> String {
    if offset_secs == 0 {
        return "UTC".into();
    }
    let sign = if offset_secs < 0 { '-' } else { '+' };
    let abs = offset_secs.unsigned_abs();
    let (h, m) = (abs / 3600, abs % 3600 / 60);
    if m == 0 {
        format!("UTC{sign}{h}")
    } else {
        format!("UTC{sign}{h}:{m:02}")
    }
}

/// `time` with this computer's zone after it, as every time the CLI and the
/// terminal app show: "Tue 14:05 (UTC+1)" (E2 r38: times name their zone).
pub fn zoned(time: &str, offset_secs: i32) -> String {
    format!("{time} ({})", zone_label(offset_secs))
}

/// The day and the 24-hour clock, "Tue 14:05", without the zone.
fn day_clock(at_ms: u64, offset_secs: i32) -> String {
    let (day, minute) = local_parts(at_ms, offset_secs);
    let names = tr_list("time.weekdays");
    let name = names.get(weekday(day)).copied().unwrap_or("");
    format!("{name} {:02}:{:02}", minute / 60, minute % 60)
}

/// `{time}`: always the day and the 24-hour clock with the zone, "Tue 14:05
/// (UTC+1)" (day names from `time.weekdays`, as in the other desktop apps).
pub fn clock_at(at_ms: u64, offset_secs: i32) -> String {
    zoned(&day_clock(at_ms, offset_secs), offset_secs)
}

/// `{time}` with the stored local offset.
pub fn clock(at_ms: u64) -> String {
    clock_at(at_ms, local_offset_secs())
}

/// The short time a message had something happen: "13:26 (UTC+1)" today,
/// else the day too ("Tue 13:26 (UTC+1)"), as the macOS and Windows apps
/// write it under a bubble.
pub fn short_clock_at(now_ms: u64, at_ms: u64, offset_secs: i32) -> String {
    let (day, minute) = local_parts(at_ms, offset_secs);
    if day == local_parts(now_ms, offset_secs).0 {
        zoned(
            &format!("{:02}:{:02}", minute / 60, minute % 60),
            offset_secs,
        )
    } else {
        clock_at(at_ms, offset_secs)
    }
}

/// `{time}` of a deadline seen at `now_ms`: [`clock_at`] from today to 5
/// days ahead (each weekday names one day), else with the date too, "Sun 1
/// Nov 17:15" (month from `time.months`), as `raven contact list` says it:
/// a deadline a week away must not read like today.
pub fn deadline_at(now_ms: u64, until_ms: u64, offset_secs: i32) -> String {
    let (day, minute) = local_parts(until_ms, offset_secs);
    if (0..=5).contains(&(day - local_parts(now_ms, offset_secs).0)) {
        return clock_at(until_ms, offset_secs);
    }
    let (month, date) = month_and_day(day);
    let names = tr_list("time.weekdays");
    let months = tr_list("time.months");
    let name = names.get(weekday(day)).copied().unwrap_or("");
    let month = months.get(month as usize - 1).copied().unwrap_or("");
    zoned(
        &format!(
            "{name} {date} {month} {:02}:{:02}",
            minute / 60,
            minute % 60
        ),
        offset_secs,
    )
}

/// [`deadline_at`] with the stored local offset.
pub fn deadline(now_ms: u64, until_ms: u64) -> String {
    deadline_at(now_ms, until_ms, local_offset_secs())
}

/// The month (1-12) and day of the month of a day count since 1970-01-01
/// (proleptic Gregorian calendar; H. Hinnant's `civil_from_days`).
fn month_and_day(days_since_epoch: i64) -> (u32, u32) {
    let (_, month, day) = civil(days_since_epoch);
    (month, day)
}

/// Year, month (1-12) and day of a day count since 1970-01-01.
fn civil(days_since_epoch: i64) -> (i64, u32, u32) {
    let z = days_since_epoch + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// The local day (days since 1970-01-01) of a moment.
pub fn local_day_at(at_ms: u64, offset_secs: i32) -> i64 {
    local_parts(at_ms, offset_secs).0
}

/// [`local_day_at`] with the stored local offset.
pub fn local_day(at_ms: u64) -> i64 {
    local_day_at(at_ms, local_offset_secs())
}

/// The day line between messages: "Today", "Yesterday", the weekday for
/// the last week ("Wednesday"), else the date ("Mon 26 Oct"), with the year
/// when it is another year ("26 Oct 2025").
pub fn day_line_at(now_ms: u64, at_ms: u64, offset_secs: i32) -> String {
    let today = local_parts(now_ms, offset_secs).0;
    let day = local_parts(at_ms, offset_secs).0;
    match today - day {
        0 => return tr("chat.today").to_string(),
        1 => return tr("chat.yesterday").to_string(),
        2..=6 => {
            let names = tr_list("time.weekdaysLong");
            if let Some(n) = names.get(weekday(day)) {
                return n.to_string();
            }
        }
        _ => {}
    }
    let (year, month, date) = civil(day);
    let (this_year, _, _) = civil(today);
    let months = tr_list("time.months");
    let month = months.get(month as usize - 1).copied().unwrap_or("");
    if year == this_year {
        let names = tr_list("time.weekdays");
        let name = names.get(weekday(day)).copied().unwrap_or("");
        trf(
            "chat.date",
            &[
                ("weekday", name),
                ("day", &date.to_string()),
                ("month", month),
            ],
        )
    } else {
        trf(
            "chat.dateYear",
            &[
                ("day", &date.to_string()),
                ("month", month),
                ("year", &year.to_string()),
            ],
        )
    }
}

/// [`day_line_at`] with the stored local offset.
pub fn day_line(now_ms: u64, at_ms: u64) -> String {
    day_line_at(now_ms, at_ms, local_offset_secs())
}

/// [`short_clock_at`] with the stored local offset.
pub fn short_clock(now_ms: u64, at_ms: u64) -> String {
    short_clock_at(now_ms, at_ms, local_offset_secs())
}

/// `{left}`: "about 35 min" under an hour, "about 1 hour", "about 22 hours"
/// under two days, then "about 3 days".
pub fn time_left(now_ms: u64, until_ms: u64) -> String {
    let left = until_ms.saturating_sub(now_ms);
    if left < HOUR {
        let minutes = left.div_ceil(MIN).max(1);
        return trf("time.leftMinutes", &[("n", &minutes.to_string())]);
    }
    let hours = (left + HOUR / 2) / HOUR;
    if hours == 1 {
        tr("time.leftHour").to_string()
    } else if hours < 48 {
        trf("time.leftHours", &[("n", &hours.to_string())])
    } else {
        trf(
            "time.leftDays",
            &[("n", &((left + DAY / 2) / DAY).to_string())],
        )
    }
}

/// `{secs}`: seconds with one decimal.
pub fn secs_one_decimal(ms: u64) -> String {
    format!("{:.1}", ms as f64 / 1000.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    // 2026-10-20 was a Tuesday. 14:05 UTC that day:
    const TUE_1405_UTC: u64 = 1_792_505_100_000;

    #[test]
    fn weekday_and_clock() {
        assert_eq!(weekday(0), 3, "1970-01-01 was a Thursday");
        assert_eq!(
            clock_at(TUE_1405_UTC, 0),
            "Tue 14:05 (UTC)",
            "today shows the day too"
        );
        assert_eq!(clock_at(TUE_1405_UTC + DAY, 0), "Wed 14:05 (UTC)");
        // Local offset +3:30 moves both the hour and, near midnight, the day.
        assert_eq!(clock_at(TUE_1405_UTC, 12_600), "Tue 17:35 (UTC+3:30)");
        assert_eq!(
            clock_at(TUE_1405_UTC + 10 * HOUR, 12_600),
            "Wed 03:35 (UTC+3:30)"
        );
        assert_eq!(clock_at(TUE_1405_UTC, -15 * 3600), "Mon 23:05 (UTC-15)");
        assert_eq!(clock_at(TUE_1405_UTC + 5 * DAY, 0), "Sun 14:05 (UTC)");
    }

    /// A deadline's `{time}` (E1 round 4): short from today to 5 days ahead,
    /// with the date from 6 days ahead (a week away read on the same weekday
    /// looked like now) and for a past one, as `raven contact list` says it.
    #[test]
    fn a_deadline_has_its_date_from_six_days_ahead() {
        // 2026-10-25 17:15 UTC, a Sunday.
        let sun = TUE_1405_UTC + 5 * DAY + 3 * HOUR + 10 * MIN;
        assert_eq!(deadline_at(sun, sun, 0), "Sun 17:15 (UTC)");
        assert_eq!(deadline_at(sun, sun + DAY, 0), "Mon 17:15 (UTC)");
        assert_eq!(deadline_at(sun, sun + 5 * DAY, 0), "Fri 17:15 (UTC)");
        assert_eq!(deadline_at(sun, sun + 6 * DAY, 0), "Sat 31 Oct 17:15 (UTC)");
        assert_eq!(deadline_at(sun, sun + 7 * DAY, 0), "Sun 1 Nov 17:15 (UTC)");
        assert_eq!(deadline_at(sun, sun - DAY, 0), "Sat 24 Oct 17:15 (UTC)");
        // Days, not hours: 23:59 five days on is still short, 00:01 on the
        // sixth is not.
        assert_eq!(
            deadline_at(sun, sun + 5 * DAY + 6 * HOUR + 44 * MIN, 0),
            "Fri 23:59 (UTC)"
        );
        assert_eq!(
            deadline_at(sun, sun + 5 * DAY + 6 * HOUR + 46 * MIN, 0),
            "Sat 31 Oct 00:01 (UTC)"
        );
        // Local days: at +3:30, Sunday 23:00 local and Sat 31 Oct local.
        assert_eq!(
            deadline_at(sun + 2 * HOUR + 15 * MIN, sun + 6 * DAY, 12_600),
            "Sat 31 Oct 20:45 (UTC+3:30)"
        );
        for (days, want) in [
            (0, (1, 1)),
            (59, (3, 1)),
            (20_751, (10, 25)),
            (11_016, (2, 29)),
        ] {
            assert_eq!(month_and_day(days), want, "{days}");
        }
    }

    #[test]
    fn relative_times() {
        let now = TUE_1405_UTC;
        assert_eq!(relative_past(now, now), "just now");
        assert_eq!(
            relative_past(now, now + 5_000),
            "just now",
            "clock skew is not the future"
        );
        assert_eq!(relative_past(now, now - 5 * MIN), "5 min ago");
        assert_eq!(relative_past(now, now - 3 * HOUR), "3 h ago");
        assert_eq!(relative_past(now, now - 59 * MIN), "59 min ago");
        assert_eq!(relative_past(now, now - 23 * HOUR), "23 h ago");
        assert_eq!(relative_past(now, now - DAY - 5 * HOUR), "1 day ago");
        assert_eq!(relative_past(now, now - 2 * DAY), "2 days ago");
        assert_eq!(heartbeat_age(now, now - 1_000), "now");
        assert_eq!(heartbeat_age(now, now - 2_999), "now");
        assert_eq!(heartbeat_age(now, now - 7_000), "7 s");
        assert_eq!(heartbeat_age(now, now - 59_000), "59 s");
        assert_eq!(heartbeat_age(now, now - 2 * MIN), "2 min ago");
    }

    #[test]
    fn left_and_seconds() {
        let now = TUE_1405_UTC;
        assert_eq!(time_left(now, now + 22 * HOUR), "about 22 hours");
        assert_eq!(time_left(now, now + 22 * HOUR - 10 * MIN), "about 22 hours");
        assert_eq!(time_left(now, now + 35 * MIN), "about 35 min");
        assert_eq!(
            time_left(now, now + 35 * MIN - 30_000),
            "about 35 min",
            "rounds up"
        );
        assert_eq!(time_left(now, now + 59 * MIN), "about 59 min");
        assert_eq!(time_left(now, now + HOUR), "about 1 hour");
        assert_eq!(time_left(now, now + 89 * MIN), "about 1 hour");
        assert_eq!(time_left(now, now + 90 * MIN), "about 2 hours");
        assert_eq!(time_left(now, now + 47 * HOUR), "about 47 hours");
        assert_eq!(time_left(now, now + 48 * HOUR), "about 2 days");
        assert_eq!(time_left(now, now + 80 * HOUR), "about 3 days");
        assert_eq!(time_left(now, now - 1), "about 1 min", "never zero");
        assert_eq!(secs_one_decimal(412), "0.4");
        assert_eq!(secs_one_decimal(1_000), "1.0");
    }

    /// Every clock time names this computer's zone, as the CLI and the
    /// terminal app do (E2 r38); day lines don't (they name no time).
    #[test]
    fn clock_times_name_the_zone() {
        assert_eq!(zone_label(0), "UTC");
        assert_eq!(zone_label(3_600), "UTC+1");
        assert_eq!(zone_label(-5 * 3_600), "UTC-5");
        assert_eq!(zone_label(19_800), "UTC+5:30");
        assert_eq!(zone_label(-12_600), "UTC-3:30");
        let now = TUE_1405_UTC;
        assert_eq!(short_clock_at(now, now, 0), "14:05 (UTC)");
        assert_eq!(short_clock_at(now, now, 3_600), "15:05 (UTC+1)");
        assert_eq!(short_clock_at(now, now - DAY, 3_600), "Mon 15:05 (UTC+1)");
        assert_eq!(short_clock_at(now, now, -12_600), "10:35 (UTC-3:30)");
        assert_eq!(deadline_at(now, now + DAY, 3_600), "Wed 15:05 (UTC+1)");
        assert_eq!(day_line_at(now, now, 3_600), "Today");
    }

    #[test]
    fn day_lines() {
        // 2026-10-20 14:05 UTC was a Tuesday.
        let now = 1_792_505_100_000;
        let day = 86_400_000;
        assert_eq!(day_line_at(now, now - 3_600_000, 0), "Today");
        assert_eq!(day_line_at(now, now - day, 0), "Yesterday");
        assert_eq!(day_line_at(now, now - 2 * day, 0), "Sunday");
        assert_eq!(day_line_at(now, now - 6 * day, 0), "Wednesday");
        assert_eq!(day_line_at(now, now - 7 * day, 0), "Tue 13 Oct");
        assert_eq!(day_line_at(now, now - 300 * day, 0), "24 Dec 2025");
        // A local offset moves midnight.
        assert_eq!(day_line_at(now, now - 14 * 3_600_000, 0), "Today");
        assert_eq!(day_line_at(now, now - 14 * 3_600_000, -3_600), "Yesterday");
        assert_eq!(local_day_at(0, 0), 0);
        assert_eq!(civil(0), (1970, 1, 1));
        assert_eq!(civil(-1), (1969, 12, 31));
        assert_eq!(civil(20_746), (2026, 10, 20));
    }
}
