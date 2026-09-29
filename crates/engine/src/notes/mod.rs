//! 笔记/日记/待办门面（M6-W1，docs/03 §6 note_* diary_* todo_* + docs/12 §1 契约）
//! 日期工具无第三方依赖（UTC 口径——HH:MM 另有 AppShell 时区层 docs/05）。

mod diary;
mod note;
mod todo;

///civil→epoch days（Howard Hinnant 算法；inline 于 date_to_unix，免配对对数翻倍）
pub fn date_to_unix(ymd: &str) -> Option<i64> {
    let mut parts = ymd.split('-');
    let y: i64 = parts.next()?.parse().ok()?;
    let m: i64 = parts.next()?.parse().ok()?;
    let d: i64 = parts.next()?.parse().ok()?;
    if parts.next().is_some()
        || ymd.len() != 10
        || ymd.as_bytes().get(4) != Some(&b'-')
        || ymd.as_bytes().get(7) != Some(&b'-')
        || !(1..=12).contains(&m)
        || !(1..=31).contains(&d)
    {
        return None;
    }
    let leap = y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
    let max_day = match m {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    if d > max_day {
        return None;
    }
    let yy = if m <= 2 { y - 1 } else { y };
    let era = if yy >= 0 { yy } else { yy - 399 } / 400;
    let yoe = yy - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * 86_400)
}

/// epoch days→civil + ISO 日期（YYYY-MM-DD，UTC）
pub fn unix_to_date(unix: i64) -> String {
    let days = unix.div_euclid(86_400);
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

/// 今日（UTC ISO——日记默认日期；TUI 经 ambition_engine::today 取用）
pub fn today() -> String {
    unix_to_date(ambition_core::now_unix())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_date_to_unix_roundtrips_iso_dates() {
        let unix = date_to_unix("2026-09-29").expect("合法日期");
        assert_eq!(unix_to_date(unix), "2026-09-29", "civil→epoch→civil 往返");
        assert!(date_to_unix("2024-02-29").is_some(), "闰日合法");
        assert!(
            date_to_unix("2026-02-29").is_none(),
            "非闰年拒绝 2 月 29 日"
        );
        assert!(date_to_unix("2026-13-01").is_none(), "月份越界拒");
        assert!(date_to_unix("nonsense").is_none(), "非日期拒");
    }

    #[test]
    fn test_unix_to_date_handles_epoch() {
        assert_eq!(unix_to_date(0), "1970-01-01", "epoch 起点");
        assert_eq!(
            unix_to_date(date_to_unix("2024-02-29").expect("闰日")),
            "2024-02-29",
            "闰日往返"
        );
    }

    #[test]
    fn test_today_is_iso_date() {
        let t = today();
        assert_eq!(t.len(), 10, "YYYY-MM-DD 十位");
        assert_eq!(
            unix_to_date(date_to_unix(&t).expect("今日可解析")),
            t,
            "今日自洽"
        );
    }
}
