//! LLM 跨文件测试夹具（收口前 provider/router 两份逐字复制，2026-09-27 复用合并）

/// 结构化出题的成功回包（空题集；请求执行环与路由测试共用）
pub(crate) fn ok_json() -> String {
    r#"{"questions": []}"#.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ok_json_fixture_is_parsable_empty_questions() {
        let value: serde_json::Value = serde_json::from_str(&ok_json()).expect("合法 JSON");
        assert!(
            value["questions"].as_array().is_some_and(|q| q.is_empty()),
            "空题集结构化回包"
        );
    }
}
