//! 语言阈值带：Thresholds/english/non_english/for_lang（en 0.75/0.35 其余 0.85/0.20，docs/07 §2）

/// 判分阈值（docs/05 §3：英语 0.75/0.35，其它语种更保守 0.85/0.20）
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Thresholds {
    pub accept: f32,
    pub reject: f32,
}

impl Thresholds {
    /// 英语默认阈值
    pub fn english() -> Self {
        Self {
            accept: 0.75,
            reject: 0.35,
        }
    }

    /// 非英语阈值（更多灰区 → 更多复核/待裁定）
    pub fn non_english() -> Self {
        Self {
            accept: 0.85,
            reject: 0.20,
        }
    }

    /// 按目标语选阈值（en → 英语档；其它 → 非英语档）
    pub fn for_lang(lang: &str) -> Self {
        if lang == "en" {
            Self::english()
        } else {
            Self::non_english()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_english_and_non_english_thresholds_match_docs() {
        let en = Thresholds::english();
        assert_eq!((en.accept, en.reject), (0.75, 0.35), "docs/05 §3");
        let other = Thresholds::non_english();
        assert_eq!((other.accept, other.reject), (0.85, 0.20), "非英语更保守");
    }

    #[test]
    fn test_for_lang_switches_between_bands() {
        assert_eq!(Thresholds::for_lang("en"), Thresholds::english());
        assert_eq!(Thresholds::for_lang("ja"), Thresholds::non_english());
        assert_eq!(Thresholds::for_lang("zh-Hans"), Thresholds::non_english());
    }

    #[test]
    fn test_non_english_thresholds_use_conservative_band() {
        let other = Thresholds::non_english();
        assert_eq!(
            (other.accept, other.reject),
            (0.85, 0.20),
            "docs/05 §3：其它语种更保守（灰区更宽）"
        );
        assert!(other.accept > Thresholds::english().accept);
        assert!(other.reject < Thresholds::english().reject);
    }
}
