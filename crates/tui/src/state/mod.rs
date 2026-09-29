//! 界面状态机数据（细分域根，docs/03 §5.1）：Screen/AppState/Msg/Cmd + 构造与视图辅助
//! 本域含子组件：`state::review`（复习屏状态，M4）。
//! 跨域引用一律 `use crate::*`（组内共享导入留在 lib.rs，docs/01 §1 组织规范）。

mod key_ctx;
pub mod review;
pub use review::*;

use crate::*;
use ambition_core::Translation;
use ambition_translate::{Alignment, CompareResult};
use ambition_writing::{Comments, ScaffoldCheck, ScaffoldStage, Template};

/// 屏幕（M4：Practice + Review；完整 Screen 集见 docs/03-engine-ui）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    /// 主页面（功能菜单选择，启动首屏）
    Home,
    Practice,
    /// 复习屏（FSRS 翻卡，docs/03 §3 M4）
    Review,
    /// 模板作文屏（M5-W4b，docs/03 §5.3 scaffold 族）
    Scaffold,
    /// 模考屏（M5-W6，docs/03 §5.3 Ctrl+M 直达；Practice 同流作答 + 屏顶倒计时）
    Mock,
    /// 翻译工作台主屏（M5-W7，docs/11；Ctrl+W 直达）
    Tx,
    /// 对照对齐屏（source↔reference 切段视图）
    TxAlign,
    /// 我来翻屏（输入我的译文 → 差异+评分）
    TxMyTurn,
    /// 翻译历史屏（检索 + 收藏 + 转练习）
    TxHistory,
    /// 笔记工作区（M6-W1，Ctrl+N）
    Notes,
    /// 日记工作区（M6-W1，Ctrl+D）
    Diary,
    /// 待办工作区（M6-W1，Ctrl+K）
    Todos,
    /// 学习画像屏（M6-W2，Ctrl+P）
    Profile,
    /// 游戏成长与道具屏（M6-W6，Ctrl+G）
    Games,
    /// 离线语境查词与生词本（M6-W5，Ctrl+V）
    Wordbook,
}

/// 模考 run 元数据（M5-W6：屏顶倒计时/收卷后得分展示；行数据在 store::paper_runs）
#[derive(Debug, Clone, PartialEq)]
pub struct MockRun {
    pub run_id: String,
    pub title: String,
    pub deadline: i64,
}

/// 应用状态
#[derive(Debug, Clone, PartialEq)]
pub struct AppState {
    pub screen: Screen,
    pub questions: Vec<Question>,
    pub idx: usize,
    pub sel: usize,
    pub grade: Option<GradeResult>,
    pub finished: bool,
    /// 每空一位的输入文本（与题面 blanks 等长；多空分行作答，废除 `;` 分隔）
    pub inputs: Vec<String>,
    /// 当前编辑的空位（0 基；Tab/→/↓ 切换，blank_sel 仅填空待答时有效）
    pub blank_sel: usize,
    /// 判分/提交失败的独立错误行（P-8：错误 ≠ Pending）
    pub error: Option<String>,
    /// 本题起始时刻（提交计时 → Timing，假对检测输入）
    pub started_at: Instant,
    /// 修订次数（光标移动/输入删改，Timing.revision_count）
    pub revisions: u32,
    /// 自评把握 1-4（提交前键入 → Attempt.self_confidence，假对检测输入，改进 #10）
    pub self_confidence: Option<u8>,
    /// 填空编辑态（打字即进入；Esc 退出保留文本——keys.rs KeyCtx::Fill{editing}）
    pub editing: bool,
    /// 选择题已选中集合（Space 切换；空 = 未显式选中，提交回退光标位）
    pub checked: Vec<u32>,
    /// 复合选择题逐槽作答记录（七选五=逐缺口、阅读=逐子题；Space 记录当前槽并前进，M4 题型补全）
    pub sub_answers: Vec<u32>,
    /// 判分后 SRS 提示行（错词本入册/假对进卡/FSRS 已安排；docs/03 §3 M4）
    pub srs_hint: Option<String>,
    /// 复习屏状态（M4）
    pub review: ReviewState,
    /// 选题面板开合（F1；docs/03 §5.3 2026-09-27）
    pub picker_open: bool,
    /// 面板光标（题库下标）
    pub picker_cursor: usize,
    /// 键位帮助浮层开/关（? 键切换，#24）
    pub help_open: bool,
    /// 主页面菜单光标（↑↓ 选择，Enter 进入）
    pub home_cursor: usize,
    /// 鼠标可点击区域注册表（每帧渲染时 clear → register → main 查表）
    pub click_targets: Vec<crate::mouse::ClickTarget>,
    /// 待裁定裁决高亮（0=判对 1=判错；←/→ 切换，Enter 确认）
    pub arbitration_sel: u8,
    /// 模板列表（当前轨；F2 进屏时经 Cmd::ScaffoldLoad 异步装载，M5-W4b）
    pub scaffold_templates: Vec<Template>,
    /// 选中模板游标
    pub scaffold_idx: usize,
    /// 脚手架三阶段（Space 循环）
    pub scaffold_stage: ScaffoldStage,
    /// 槽位输入（与选中模板 slots 对齐；换模板即重置）
    pub scaffold_fills: Vec<String>,
    /// 当前槽位游标
    pub scaffold_slot: usize,
    /// 最近一次成文（Enter 回填；自查以其为输入）
    pub scaffold_result: Option<String>,
    /// 最近一次自查结果（Ctrl+S 回填）
    pub scaffold_check: Option<ScaffoldCheck>,
    /// 模考元数据（开卷写入；收卷后保留供完成页展示，M5-W6）
    pub mock: Option<MockRun>,
    /// 模考得分（engine::mock_finish 回填：(实得, 满分)）
    pub mock_result: Option<(f32, f32)>,
    // —— 翻译工作台（M5-W7，docs/11）——
    /// 主屏源文输入（Enter 发 translate）
    pub tx_source: String,
    /// 我来翻的输入译文
    pub tx_mine: String,
    /// 当前译文（translate 结果；align/my_turn 的参考）
    pub tx_result: Option<Translation>,
    /// 对齐切段视图（TxAlign 展示）
    pub tx_align: Option<Alignment>,
    /// 我来翻对照（参考 + 差异 + 评分）
    pub tx_compare: Option<CompareResult>,
    /// 历史列表（TxHistory；limit 倒序）
    pub tx_history: Vec<Translation>,
    /// 收藏 id 集（history 载入时一并回填，Space 切换乐观更新）
    pub tx_favorites: Vec<String>,
    /// 历史选中下标
    pub tx_sel: usize,
    /// 判分后批注（Ctrl+E 回填；M5-W5 docs/10 §2，随题清空）
    pub comments: Option<Comments>,
    /// 最近一次提交的文本作答（作文/翻译批注输入；随题清空）
    pub submitted_text: Option<String>,
    /// M6-W1 笔记/日记/TODO 工作区草稿与最近列表
    pub workspace_draft: String,
    pub workspace_items: Vec<String>,
}

mod msg;

pub use msg::{Cmd, Msg};

/// 题目空位数（填空类 = blanks 数；其它题型 = 0——inputs 长度据此重建；M4 题型补全含完形/语法填空）
pub fn blank_count(kind: &QuestionKind) -> usize {
    kind.blank_count()
}

/// 构造初始状态（空题库直接 finished）
pub fn new_state(questions: Vec<Question>) -> AppState {
    let finished = questions.is_empty();
    let inputs = questions
        .first()
        .map(|q| vec![String::new(); blank_count(&q.kind)])
        .unwrap_or_default();
    AppState {
        screen: Screen::Practice,
        questions,
        idx: 0,
        sel: 0,
        grade: None,
        finished,
        inputs,
        blank_sel: 0,
        error: None,
        started_at: Instant::now(),
        revisions: 0,
        self_confidence: None,
        editing: false,
        checked: Vec::new(),
        sub_answers: Vec::new(),
        srs_hint: None,
        review: ReviewState::default(),
        picker_open: false,
        picker_cursor: 0,
        help_open: false,
        home_cursor: 0,
        click_targets: Vec::new(),
        arbitration_sel: 0,
        scaffold_templates: Vec::new(),
        scaffold_idx: 0,
        scaffold_stage: ScaffoldStage::Full,
        scaffold_fills: Vec::new(),
        scaffold_slot: 0,
        scaffold_result: None,
        scaffold_check: None,
        mock: None,
        mock_result: None,
        tx_source: String::new(),
        tx_mine: String::new(),
        tx_result: None,
        tx_align: None,
        tx_compare: None,
        tx_history: Vec::new(),
        tx_favorites: Vec::new(),
        tx_sel: 0,
        comments: None,
        submitted_text: None,
        workspace_draft: String::new(),
        workspace_items: Vec::new(),
    }
}

impl AppState {
    /// 当前题是否等待填空作答（有题、未判分、题型为填空）
    pub fn is_fill_pending(&self) -> bool {
        !self.finished && self.grade.is_none() && self.questions[self.idx].kind.is_text_input()
    }

    /// 采集本题计时（提交时调用；毫秒）
    pub fn timing(&self) -> Timing {
        Timing {
            total_ms: self.started_at.elapsed().as_millis() as u64,
            revision_count: self.revisions,
        }
    }
}

/// 写入判分结果（main 在 submit 成功后调用；裁决高亮复位到默认档）
pub fn apply_grade(state: &mut AppState, grade: GradeResult) {
    state.grade = Some(grade);
    state.arbitration_sel = 0;
}

/// 写入独立错误行（main 在 submit 失败后调用；P-8：绝不降级成 Pending 假判分）
pub fn apply_error(state: &mut AppState, message: String) {
    state.error = Some(message);
}

/// 判分后 SRS 提示（入卡处置回传 UI，docs/03 §2 M4；None = 无处置清空提示）
pub fn apply_srs_hint(state: &mut AppState, action: Option<CardAction>) {
    state.srs_hint = action.map(|act| match act {
        CardAction::Create => "📚 已进错词复习队列".to_string(),
        CardAction::Bump => "📚 错词本 +1（错词提权，尽快复习）".to_string(),
        CardAction::Schedule(_) => " FSRS 已安排下次复习".to_string(),
        CardAction::Skip => String::new(),
    });
}

/// 内置示例题（测试夹具；运行时题库来自 seed，docs/16-config-i18n §seed）
pub fn sample_question() -> Question {
    Question::new(
        "She is ___ university student.".into(),
        QuestionKind::SingleChoice(ambition_core::ChoiceBody {
            options: vec![
                ambition_core::OptionItem {
                    id: 0,
                    text: "a".into(),
                },
                ambition_core::OptionItem {
                    id: 1,
                    text: "an".into(),
                },
                ambition_core::OptionItem {
                    id: 2,
                    text: "the".into(),
                },
                ambition_core::OptionItem {
                    id: 3,
                    text: "two".into(),
                },
            ],
            answer: vec![1],
        }),
        1,
        vec!["article".into()],
        ambition_core::Provenance::Seed,
    )
}

/// 填空题各空的可接受答案表（渲染正确答案用；非填空 → 空；M4 题型补全含完形/语法填空）
pub fn fill_answers(kind: &QuestionKind) -> Vec<Vec<String>> {
    match kind {
        QuestionKind::FillBlank(fill) => fill.blanks.iter().map(|b| b.acceptable.clone()).collect(),
        QuestionKind::Cloze(cloze) => cloze.blanks.iter().map(|b| b.acceptable.clone()).collect(),
        QuestionKind::GrammarFill(gf) => gf.blanks.iter().map(|b| b.acceptable.clone()).collect(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::*;
    use ambition_core::{Provenance, Verdict};

    #[test]
    fn test_new_state_initializes_position() {
        let s = new_state(vec![sample_question()]);
        assert_eq!(s.idx, 0);
        assert_eq!(s.sel, 0);
        assert!(s.grade.is_none());
        assert!(!s.finished);
        assert!(s.inputs.is_empty(), "选择题无输入位");
        assert_eq!(s.blank_sel, 0);
        assert!(s.error.is_none());
        assert_eq!(s.revisions, 0);
        assert!(new_state(vec![]).finished, "空题库直接结束");
        assert_eq!(s.screen, Screen::Practice);
    }

    #[test]
    fn test_blank_count_by_kind() {
        assert_eq!(blank_count(&sample_question().kind), 0, "选择题 0 位");
        assert_eq!(blank_count(&fill_question(1).kind), 1, "单词 1 位");
        assert_eq!(blank_count(&fill_question(2).kind), 2, "多空 2 位");
        assert_eq!(
            new_state(vec![fill_question(2)]).inputs.len(),
            2,
            "构造时按空数建输入位"
        );
    }

    #[test]
    fn test_is_fill_pending_gates_char_input() {
        let choice = new_state(vec![sample_question()]);
        assert!(!choice.is_fill_pending(), "选择题不吞字符");
        let mut fill = new_state(vec![fill_question(1)]);
        assert!(fill.is_fill_pending(), "填空未判分 = 待答");
        assert_eq!(
            update(&mut fill, Msg::Char('a')),
            Cmd::None,
            "打字直接进编辑"
        );
        assert!(fill.editing, "首键进入编辑态");
        assert_eq!(update(&mut fill, Msg::StopEdit), Cmd::None, "Esc 退出编辑");
        assert!(!fill.editing && fill.inputs[0] == "a", "退出编辑保留输入");
        update(&mut fill, Msg::Char('b'));
        assert!(fill.editing && fill.inputs[0] == "ab", "打字直接恢复编辑");
        apply_grade(&mut fill, grade(Verdict::Correct));
        assert!(!fill.is_fill_pending(), "判分后退出待答");
        fill.finished = true;
        assert!(!fill.is_fill_pending(), "结束后非待答");
    }
    #[test]
    fn test_apply_error_keeps_grade_none_and_is_overwritten() {
        let mut s = new_state(vec![sample_question()]);
        assert!(s.grade.is_none() && s.error.is_none());
        apply_error(&mut s, "store: disk full".into());
        assert!(s.grade.is_none(), "错误绝不落成 Pending 判分（P-8）");
        assert_eq!(s.error.as_deref(), Some("store: disk full"));
        // 成功路径不写错误
        apply_grade(&mut s, grade(Verdict::Correct));
        assert_eq!(s.grade.as_ref().map(|g| g.verdict), Some(Verdict::Correct));
        assert!(s.error.is_some(), "错误行独立存活直到下一次提交清除");
        apply_error(&mut s, "second failure".into());
        assert_eq!(s.error.as_deref(), Some("second failure"), "后写覆盖");
    }

    #[test]
    fn test_timing_collects_elapsed_and_revisions() {
        let mut s = new_state(vec![sample_question()]);
        update(&mut s, Msg::MoveDown);
        update(&mut s, Msg::MoveUp);
        let t = s.timing();
        assert!(t.total_ms < 60_000, "耗时合理: {}", t.total_ms);
        assert_eq!(t.revision_count, 2, "修订次数来自移动/输入");
        // 复位后归零
        update(&mut s, Msg::Submit);
        apply_grade(&mut s, grade(Verdict::Correct));
        update(&mut s, Msg::Next);
        let t2 = s.timing();
        assert_eq!(t2.revision_count, 0, "下一题重新计数");
        assert!(t2.total_ms <= t.total_ms, "计时重置");
    }

    #[test]
    fn test_sample_question_is_single_choice_with_stable_qid() {
        let q = sample_question();
        assert!(matches!(q.kind, QuestionKind::SingleChoice(_)));
        assert_eq!(q.qid, sample_question().qid, "同内容同 qid");
        assert!(q.tags.contains(&"article".to_string()));
        assert_eq!(q.provenance, Provenance::Seed);
    }

    #[test]
    fn test_apply_grade_sets_and_overrides_grade() {
        let mut s = new_state(vec![sample_question()]);
        assert!(s.grade.is_none());
        apply_grade(&mut s, grade(Verdict::Wrong));
        assert_eq!(s.grade.as_ref().map(|g| g.verdict), Some(Verdict::Wrong));
        apply_grade(&mut s, grade(Verdict::Correct));
        assert_eq!(
            s.grade.as_ref().map(|g| g.verdict),
            Some(Verdict::Correct),
            "可覆盖"
        );
        assert!(s.error.is_none(), "成功路径不写错误行");
    }

    #[test]
    fn test_apply_srs_hint_maps_actions_and_clears_on_none() {
        let mut s = new_state(vec![sample_question()]);
        apply_srs_hint(&mut s, Some(CardAction::Create));
        assert_eq!(s.srs_hint.as_deref(), Some("📚 已进错词复习队列"));
        apply_srs_hint(&mut s, Some(CardAction::Bump));
        assert!(s.srs_hint.as_deref().unwrap_or("").contains("错词本 +1"));
        apply_srs_hint(
            &mut s,
            Some(CardAction::Schedule(ambition_engine::Rating::Good)),
        );
        assert!(s.srs_hint.as_deref().unwrap_or("").contains("FSRS"));
        apply_srs_hint(&mut s, Some(CardAction::Skip));
        assert_eq!(s.srs_hint, Some(String::new()), "Skip 空串（渲染跳过）");
        apply_srs_hint(&mut s, None);
        assert!(s.srs_hint.is_none(), "None 清空");
    }

    #[test]
    fn test_fill_answers_exposes_acceptable_sets() {
        let single = fill_answers(&fill_question(1).kind);
        assert_eq!(single.len(), 1);
        assert_eq!(single[0], vec!["am".to_string(), "by".to_string()]);
        let multi = fill_answers(&fill_question(2).kind);
        assert_eq!(multi.len(), 2, "两空两组答案");
        assert!(
            fill_answers(&sample_question().kind).is_empty(),
            "非填空为空"
        );
        // M4 题型补全：完形/语法填空同路
        let cloze = Question::new(
            "c".into(),
            QuestionKind::Cloze(ambition_core::ClozeBody {
                passage: "I ___ a.".into(),
                blanks: vec![ambition_core::BlankItem {
                    acceptable: vec!["am".into()],
                }],
            }),
            2,
            vec![],
            Provenance::Seed,
        );
        assert_eq!(fill_answers(&cloze.kind), vec![vec!["am".to_string()]]);
        let gf = Question::new(
            "g".into(),
            QuestionKind::GrammarFill(ambition_core::GrammarFillBody {
                passage: "She ___ .".into(),
                blanks: vec![ambition_core::BlankItem {
                    acceptable: vec!["goes".into()],
                }],
            }),
            2,
            vec![],
            Provenance::Seed,
        );
        assert_eq!(fill_answers(&gf.kind), vec![vec!["goes".to_string()]]);
    }
}
