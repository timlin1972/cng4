use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Local, NaiveDateTime};
use sysinfo::System;

pub fn ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("System time is before UNIX epoch!")
        .as_secs()
}

pub fn ts_str(ts: u64) -> String {
    let datetime_local: DateTime<Local> = DateTime::from_timestamp(ts as i64, 0)
        .unwrap_or_else(|| panic!("Failed to parse ts ({ts})"))
        .with_timezone(&Local);

    datetime_local.format("%H:%M:%S").to_string()
}

pub fn ts_str_full(ts: u64) -> String {
    let datetime_local: DateTime<Local> = DateTime::from_timestamp(ts as i64, 0)
        .unwrap_or_else(|| panic!("Failed to parse ts ({ts})"))
        .with_timezone(&Local);

    datetime_local.format("%Y-%m-%d %H:%M:%S %:z").to_string()
}

pub fn ts_str_local(ts: u64) -> String {
    let datetime_local: DateTime<Local> = DateTime::from_timestamp(ts as i64, 0)
        .unwrap_or_else(|| panic!("Failed to parse ts ({ts})"))
        .with_timezone(&Local);

    datetime_local.format("%Y-%m-%d %H:%M:%S").to_string()
}

// pub fn ts_str_no_tz_no_sec(ts: u64) -> String {
//     let datetime_local: DateTime<Local> = DateTime::from_timestamp(ts as i64, 0)
//         .unwrap_or_else(|| panic!("Failed to parse ts ({ts})"))
//         .with_timezone(&Local);

//     datetime_local.format("%Y-%m-%d %H:%M").to_string()
// }

pub fn datetime_str_to_ts(datetime_str: &str) -> i64 {
    let naive_datetime = NaiveDateTime::parse_from_str(datetime_str, "%Y-%m-%dT%H:%M")
        .expect("解析日期時間字串失敗");
    naive_datetime.and_utc().timestamp()
}

//
// uptime
//

pub fn uptime() -> u64 {
    System::uptime()
}

pub fn uptime_str(uptime: u64) -> String {
    let mut uptime = uptime;
    let days = uptime / 86400;
    uptime -= days * 86400;
    let hours = uptime / 3600;
    uptime -= hours * 3600;
    let minutes = uptime / 60;
    let seconds = uptime % 60;

    format!("{days}d {hours:02}:{minutes:02}:{seconds:02}")
}
