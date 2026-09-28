//! 题目模型：QuestionKind/QuestionContent/Question 构造与视图（docs/02 §2；M4 题型补全：Cloze/Reading/SevenOfFive/GrammarFill + Answer::Group）

use crate::*;

/// 选项
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct OptionItem {
    pub id: u32,
    pub text: String,
}

/// 选择题体（单选 answer 只有一个下标；多选可多个）
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ChoiceBody {
    pub options: Vec<OptionItem>,
    pub answer: Vec<u32>,
}

/// 填空空位（可接受答案表，规则层直接比对）
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BlankItem {
    pub acceptable: Vec<String>,
}

/// 填空题体
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FillBody {
    pub sentence: String,
    pub blanks: Vec<BlankItem>,
}

/// 完形填空体（语篇 + 逐空可接受表；M4 题型补全，docs/02 §1）
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ClozeBody {
    pub passage: String,
    pub blanks: Vec<BlankItem>,
}

/// 语法填空体（同 Cloze；M4 题型补全）
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GrammarFillBody {
    pub passage: String,
    pub blanks: Vec<BlankItem>,
}

/// 阅读子题（M4 题型补全）
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ReadingItem {
    pub question: String,
    pub options: Vec<OptionItem>,
    pub answer: Vec<u32>,
}

/// 阅读理解体（语篇 + 子题；M4 题型补全，docs/07 §1 子题递归）
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ReadingBody {
    pub passage: String,
    pub items: Vec<ReadingItem>,
}

/// 七选五体（段落 5 缺口 + 7 候选句；M4 题型补全，docs/02 §1）
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SevenOfFiveBody {
    pub passage: String,
    pub options: Vec<String>,
    pub answer: Vec<u32>,
}

/// 翻译方向（from → to；M5-W2，docs/02 §1——源文侧 l1、目标侧 l2，state 只进 l2 侧）
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TranslationDirection {
    pub from: LangTag,
    pub to: LangTag,
}

/// 翻译题体（考试向；工作台在 docs/11，M5-W2）
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TranslationBody {
    pub direction: TranslationDirection,
    /// 源文（l1；仅题面展示，**永不进 Jev state**，docs/05 §3.5）
    pub source_text: String,
    /// 目标语参考译文（l2；进 Jev state 作判分参照）
    pub reference: String,
}

/// 题型（M1 三型 + M4 题型补全：完形/阅读/七选五/语法填空；M5-W2 翻译；DESIGN §13.4）
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuestionKind {
    SingleChoice(ChoiceBody),
    MultiChoice(ChoiceBody),
    FillBlank(FillBody),
    Cloze(ClozeBody),
    Reading(ReadingBody),
    SevenOfFive(SevenOfFiveBody),
    GrammarFill(GrammarFillBody),
    Translation(TranslationBody),
}

/// 题目内容（参与 qid 寻址的字段集 —— 具名结构，schema 演化不致串位，docs/02-core-store §2）
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct QuestionContent {
    pub stem: String,
    pub kind: QuestionKind,
    pub difficulty: u8,
    pub tags: Vec<String>,
}

/// 题目（内容寻址 qid；lang/provenance/quality_score 不参与寻址）
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Question {
    pub qid: Qid,
    pub kind: QuestionKind,
    pub stem: String,
    pub difficulty: u8,
    pub tags: Vec<String>,
    pub lang: LangTag,
    pub provenance: Provenance,
    pub quality_score: f32,
}

impl Question {
    /// 构造题目并计算内容寻址 qid（同内容同 id；lang/provenance/quality 不参与寻址）
    pub fn new(
        stem: String,
        kind: QuestionKind,
        difficulty: u8,
        tags: Vec<String>,
        provenance: Provenance,
    ) -> Self {
        let content = QuestionContent {
            stem,
            kind,
            difficulty,
            tags,
        };
        let qid = Qid::from_content(&serde_json::to_string(&content).unwrap_or_default());
        let QuestionContent {
            stem,
            kind,
            difficulty,
            tags,
        } = content;
        Self {
            qid,
            kind,
            stem,
            difficulty,
            tags,
            lang: LangTag::english(),
            provenance,
            quality_score: 1.0,
        }
    }

    /// 由内容重建题目（qid 重算，provenance 显式指定；导入/生成路径用，P-4 显式逻辑）
    pub fn with_content(content: QuestionContent, provenance: Provenance) -> Self {
        let mut q = Self::new(
            content.stem,
            content.kind,
            content.difficulty,
            content.tags,
            provenance,
        );
        q.provenance = provenance;
        q
    }

    /// 参与寻址的内容视图（存库 payload / qid 校验）
    pub fn content(&self) -> QuestionContent {
        QuestionContent {
            stem: self.stem.clone(),
            kind: self.kind.clone(),
            difficulty: self.difficulty,
            tags: self.tags.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::fixtures::*;

    #[test]
    fn test_new_computes_stable_qid() {
        let a = Question::new(
            "She is ___ student.".into(),
            QuestionKind::FillBlank(FillBody {
                sentence: "She is ___ student.".into(),
                blanks: vec![BlankItem {
                    acceptable: vec!["a".into()],
                }],
            }),
            2,
            vec!["article".into()],
            Provenance::Seed,
        );
        let b = Question::new(
            a.stem.clone(),
            a.kind.clone(),
            2,
            vec!["article".into()],
            Provenance::Seed,
        );
        assert_eq!(a.qid, b.qid);
        let c = Question::new(
            a.stem.clone(),
            a.kind.clone(),
            4,
            vec!["article".into()],
            Provenance::Seed,
        );
        assert_ne!(a.qid, c.qid, "难度参与内容哈希");
        // provenance/lang/quality 不参与寻址：同一内容不同出处同 qid
        let d = Question::new(
            a.stem.clone(),
            a.kind.clone(),
            2,
            vec!["article".into()],
            Provenance::Generated,
        );
        assert_eq!(a.qid, d.qid, "provenance 不参与内容寻址");
        assert_eq!(a.lang, LangTag::english());
        assert_eq!(a.quality_score, 1.0);
    }

    #[test]
    fn test_with_content_rebuilds_and_content_roundtrips() {
        let original = Question::new(
            "Pick one.".into(),
            QuestionKind::SingleChoice(test_choice_body()),
            3,
            vec!["vocab".into()],
            Provenance::Seed,
        );
        let view = original.content();
        assert_eq!(view.stem, original.stem);
        assert_eq!(view.difficulty, 3);
        let rebuilt = Question::with_content(view, Provenance::Imported);
        assert_eq!(rebuilt.qid, original.qid, "同内容重算 qid 稳定");
        assert_eq!(rebuilt.provenance, Provenance::Imported);
    }

    #[test]
    fn test_content_view_matches_addressed_fields() {
        let original = Question::new(
            "Stem.".into(),
            QuestionKind::SingleChoice(test_choice_body()),
            5,
            vec!["t1".into(), "t2".into()],
            Provenance::Seed,
        );
        let view = original.content();
        assert_eq!(view.stem, "Stem.");
        assert_eq!(view.difficulty, 5);
        assert_eq!(view.tags, vec!["t1".to_string(), "t2".to_string()]);
        assert_eq!(view.kind, original.kind);
    }

    #[test]
    fn test_question_kind_holds_choice_body() {
        let body = test_choice_body();
        let q = Question::new(
            "choose".into(),
            QuestionKind::SingleChoice(body),
            1,
            vec!["vocab".into()],
            Provenance::Seed,
        );
        assert_eq!(q.stem, "choose");
        assert_eq!(q.difficulty, 1);
    }
}
