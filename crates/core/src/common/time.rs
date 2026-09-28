//! 时间工具（通用，docs/02 §1）：unix 秒时钟
//!
//! 收口前 `now_unix()` 在 tui/main、llm/ledger、engine/srs 三处逐字重复（2026-09-27 复用合并）。

use std::time::{SystemTime, UNIX_EPOCH};

/// 当前 unix 时间（秒）；时钟早于 1970 时回 0（显式兜底，不 panic）
pub fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_now_unix_is_current_reasonable_seconds() {
        assert!(now_unix() > 1_700_000_000, "当前 unix 秒合理");
        assert!(now_unix() >= 0, "unix 秒非负");
    }
}
