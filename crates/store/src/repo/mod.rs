//! 持久化仓储（细分域，docs/02-core-store §3-§4）：作答事务 / 裁决桶 / 题库 / 复习卡 / 键值 / 用量
//! 跨域引用一律 `use crate::*`（组内共享导入留在 lib.rs，docs/01 §1 组织规范）

pub mod arbitration;
pub mod attempts;
pub mod cards;
pub mod essay;
pub mod kv;
pub mod question_repo;
pub mod usage;

pub use arbitration::*;
pub use attempts::*;
pub use cards::*;
pub use essay::*;
pub use kv::*;
pub use question_repo::*;
pub use usage::*;

/// 载荷 JSON 解码：坏行 = sqlx `Decode` 错误显式上抛，不静默丢行（P-4 显式兜底口径）。
/// `what` = 上下文（如 "attempt 载荷损坏（a1）"）；错误串 = "{what}: {原错误}"。
/// 收口前 attempts/arbitration/cards/question_repo 四文件六处 `from_str+map_err` 逐处重写（2026-09-27 复用合并）。
fn decode_payload<T: serde::de::DeserializeOwned>(
    payload: &str,
    what: &str,
) -> Result<T, sqlx::Error> {
    serde_json::from_str(payload)
        .map_err(|err| sqlx::Error::Decode(format!("{what}: {err}").into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_payload_roundtrips_valid_json() {
        let value: String = decode_payload(r#""hi""#, "测试载荷").expect("合法 JSON 可解");
        assert_eq!(value, "hi");
    }

    #[test]
    fn test_decode_payload_maps_broken_row_to_decode_error_with_context() {
        let err = decode_payload::<String>("{broken", "测试载荷损坏").expect_err("坏行必须报错");
        assert!(matches!(err, sqlx::Error::Decode(_)), "显式 Decode 不静默");
        assert!(
            err.to_string().contains("测试载荷损坏"),
            "错误带上下文: {err}"
        );
    }
}
