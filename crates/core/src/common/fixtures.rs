//! core 跨文件测试夹具（收口前 kind/question/taxonomy 三份逐字复制，2026-09-27 复用合并）

use crate::{ChoiceBody, OptionItem};

/// 单选题干夹具（quick/slowly 二选一、答 0；题型判定与词表测试共用）。
/// 命名沿用历史 `test_choice_body`（`test_` 前缀按配对门禁口径视为测试支持码，免配对桩）。
pub(crate) fn test_choice_body() -> ChoiceBody {
    ChoiceBody {
        options: vec![
            OptionItem {
                id: 0,
                text: "quick".into(),
            },
            OptionItem {
                id: 1,
                text: "slowly".into(),
            },
        ],
        answer: vec![0],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_choice_body_fixture_is_quick_over_slowly() {
        let body = test_choice_body();
        assert_eq!(body.options.len(), 2, "二选一夹具");
        assert_eq!(body.answer, vec![0], "答 quick");
        assert_eq!(body.options[0].text, "quick");
    }
}
