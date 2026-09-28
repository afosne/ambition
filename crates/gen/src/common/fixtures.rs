//! gen 跨文件测试夹具（收口前 pipeline 三个文件逐字复制，2026-09-27 复用合并）

use crate::{GenSpec, QuestionKindTag};

/// 出题规格夹具：cet4、单选+填空、3 题（生成编排/模板渲染/规格测试共用）
pub(crate) fn spec() -> GenSpec {
    GenSpec::new(
        "cet4",
        vec![QuestionKindTag::SingleChoice, QuestionKindTag::FillBlank],
        3,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spec_fixture_is_cet4_two_kinds_count_three() {
        let s = spec();
        assert_eq!(s.track.0, "cet4");
        assert_eq!(s.kinds.len(), 2, "单选+填空两型");
        assert_eq!(s.count, 3);
    }
}
