//! update 状态机测试（从 update.rs 拆出，docs/01 §组织：文件 ≤500 行；P-16 测试随被测物走）

use super::*;
use ambition_core::Verdict;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::*;

    #[test]
    fn test_update_moves_selection_with_wrap() {
        let mut s = new_state(vec![sample_question()]);
        assert_eq!(update(&mut s, Msg::MoveDown), Cmd::None);
        assert_eq!(s.sel, 1);
        assert_eq!(update(&mut s, Msg::MoveUp), Cmd::None);
        assert_eq!(s.sel, 0);
        assert_eq!(update(&mut s, Msg::MoveUp), Cmd::None, "向上环绕到末项");
        assert_eq!(s.sel, 3);
        update(&mut s, Msg::MoveDown);
        assert_eq!(s.sel, 0, "向下环绕回首项");
        assert!(s.revisions >= 3, "移动计入修订次数");
    }

    #[test]
    fn test_update_submits_answer_and_ignores_after_grade() {
        let mut s = new_state(vec![sample_question()]);
        update(&mut s, Msg::MoveDown);
        assert_eq!(
            update(&mut s, Msg::Submit),
            Cmd::Submit(Answer::Choice(vec![1]))
        );
        apply_grade(&mut s, grade(Verdict::Correct));
        // 已判分：提交/移动无效
        assert_eq!(update(&mut s, Msg::Submit), Cmd::None);
        assert_eq!(update(&mut s, Msg::MoveDown), Cmd::None);
        assert_eq!(s.sel, 1, "判分后选择不移动");
    }

    #[test]
    fn test_update_fill_input_builds_answer_and_counts_revisions() {
        let mut s = new_state(vec![fill_question(1)]);
        assert_eq!(s.inputs.len(), 1, "单词一位");
        assert_eq!(update(&mut s, Msg::Char('a')), Cmd::None);
        assert_eq!(update(&mut s, Msg::Char('m')), Cmd::None);
        assert_eq!(s.inputs[0], "am");
        assert_eq!(s.revisions, 2, "逐字符计入修订");
        assert_eq!(
            update(&mut s, Msg::Submit),
            Cmd::Submit(Answer::Fill(vec!["am".to_string()])),
            "单空整串提交"
        );
        assert_eq!(update(&mut s, Msg::Backspace), Cmd::None, "判分后不删改");
    }

    #[test]
    fn test_update_cycles_blank_fields_and_submits_each() {
        let mut s = new_state(vec![fill_question(2)]);
        assert_eq!(s.inputs.len(), 2, "两空两位");
        assert_eq!(s.blank_sel, 0);
        for c in "am".chars() {
            update(&mut s, Msg::Char(c));
        }
        update(&mut s, Msg::MoveDown);
        assert_eq!(s.blank_sel, 1, "下一空");
        for c in "by".chars() {
            update(&mut s, Msg::Char(c));
        }
        update(&mut s, Msg::MoveUp);
        assert_eq!(s.blank_sel, 0, "上一空");
        update(&mut s, Msg::Backspace);
        assert_eq!(s.inputs[0], "a", "退格只作用当前空");
        assert_eq!(s.inputs[1], "by", "其它空不受影响");
        assert_eq!(
            update(&mut s, Msg::Submit),
            Cmd::Submit(Answer::Fill(vec!["a".to_string(), "by".to_string()])),
            "每位 trim 后按序提交（当前空已退格到 a）"
        );
        // 单空：切换原地不动、不计修订
        let mut one = new_state(vec![fill_question(1)]);
        let rev = one.revisions;
        update(&mut one, Msg::MoveDown);
        assert_eq!(one.blank_sel, 0, "单空原地");
        assert_eq!(one.revisions, rev, "单空切换不计修订");
        // 判分后切换无效
        apply_grade(&mut s, grade(Verdict::Correct));
        update(&mut s, Msg::MoveDown);
        assert_eq!(s.blank_sel, 0, "判分后不切空");
    }

    #[test]
    fn test_update_fill_rejects_wrong_blank_count_with_error() {
        let mut s = new_state(vec![fill_question(2)]);
        // 人为制造输入位与空数不齐（状态异常路径，防御性不提交）
        s.inputs.push(String::new());
        assert_eq!(update(&mut s, Msg::Submit), Cmd::None, "输入位不齐不提交");
        let err = s.error.clone().expect("错误行被设置");
        assert!(err.contains("个空"), "错误提示空数: {err}");
        // 修回后正常提交，且进入提交即清错误
        s.inputs.pop();
        update(&mut s, Msg::Char('a'));
        update(&mut s, Msg::MoveDown);
        update(&mut s, Msg::Char('b'));
        assert_eq!(
            update(&mut s, Msg::Submit),
            Cmd::Submit(Answer::Fill(vec!["a".to_string(), "b".to_string()]))
        );
        assert!(s.error.is_none(), "进入提交即清错误");
    }

    #[test]
    fn test_update_next_advances_and_finishes() {
        let mut s = new_state(vec![sample_question(), sample_question()]);
        update(&mut s, Msg::Submit);
        apply_grade(&mut s, grade(Verdict::Correct));
        assert_eq!(update(&mut s, Msg::Next), Cmd::None);
        assert_eq!(s.idx, 1, "进入第二题");
        assert!(s.grade.is_none() && s.sel == 0, "状态复位");
        assert!(s.inputs.iter().all(|f| f.is_empty()), "输入位复位");
        assert_eq!(s.blank_sel, 0, "空位复位");
        assert_eq!(s.revisions, 0, "修订计数复位");
        // 未判分时 Next 无效
        assert_eq!(update(&mut s, Msg::Next), Cmd::None);
        assert_eq!(s.idx, 1);
        update(&mut s, Msg::Submit);
        apply_grade(&mut s, grade(Verdict::Wrong));
        update(&mut s, Msg::Next);
        assert!(s.finished, "最后一题后结束");
        assert_eq!(update(&mut s, Msg::MoveDown), Cmd::None, "结束后移动无效");
        assert_eq!(update(&mut s, Msg::Quit), Cmd::Quit);
        assert_eq!(update(&mut s, Msg::Next), Cmd::None, "结束后 Next 无效");
    }

    #[test]
    fn test_update_arbitrate_only_fires_on_pending_grade() {
        let mut s = new_state(vec![sample_question()]);
        // 未判分：确认裁决无效
        assert_eq!(update(&mut s, Msg::ConfirmArbitrate), Cmd::None);
        // 非 Pending 判分：忽略
        apply_grade(&mut s, grade(Verdict::Correct));
        assert_eq!(
            update(&mut s, Msg::ConfirmArbitrate),
            Cmd::None,
            "已定判不裁决"
        );
        // Pending：默认高亮判对，箭头切换，Enter 确认
        apply_grade(&mut s, grade(Verdict::Pending));
        assert_eq!(s.arbitration_sel, 0, "默认高亮判对");
        assert_eq!(
            update(&mut s, Msg::ConfirmArbitrate),
            Cmd::Arbitrate(true),
            "确认判对"
        );
        update(&mut s, Msg::MoveDown);
        assert_eq!(s.arbitration_sel, 1, "箭头切到判错");
        update(&mut s, Msg::MoveUp);
        assert_eq!(s.arbitration_sel, 0, "箭头切回判对");
        update(&mut s, Msg::MoveDown);
        assert_eq!(
            update(&mut s, Msg::ConfirmArbitrate),
            Cmd::Arbitrate(false),
            "确认判错"
        );
        // 判分后高亮复位（apply_grade）
        apply_grade(&mut s, grade(Verdict::Correct));
        assert_eq!(s.arbitration_sel, 0, "新判分复位高亮");
    }

    #[test]
    fn test_update_confidence_records_pre_submit_and_resets_on_next() {
        let mut s = new_state(vec![sample_question()]);
        assert_eq!(update(&mut s, Msg::Confidence(3)), Cmd::None);
        assert_eq!(
            s.self_confidence,
            Some(3),
            "自评落状态（→ Attempt.self_confidence）"
        );
        update(&mut s, Msg::Confidence(9));
        assert_eq!(s.self_confidence, Some(4), "越界钳到 4");
        update(&mut s, Msg::Confidence(0));
        assert_eq!(s.self_confidence, Some(1), "越界钳到 1");
        // 判分后不再改（防误触）
        apply_grade(&mut s, grade(Verdict::Wrong));
        update(&mut s, Msg::Confidence(2));
        assert_eq!(s.self_confidence, Some(1), "判分后锁定");
        // 下一题重置
        update(&mut s, Msg::Next);
        assert_eq!(s.self_confidence, None, "新题重新自评");
        // 填空输入态不吞自评（键位层已路由为字符，状态机双保险）
        let mut fill = new_state(vec![fill_question(1)]);
        update(&mut fill, Msg::Confidence(2));
        assert_eq!(fill.self_confidence, None, "填空输入态不自评");
    }

    #[test]
    fn test_update_select_toggles_and_guards_empty_submission() {
        // 多选：Space 切换选中集，提交选中集优先
        let mut s = new_state(vec![multi_question()]);
        update(&mut s, Msg::Select);
        assert_eq!(s.checked, vec![0], "Space 选中光标位");
        update(&mut s, Msg::MoveDown);
        update(&mut s, Msg::Select);
        assert_eq!(s.checked, vec![0, 1], "多选连按累积");
        update(&mut s, Msg::Select);
        assert_eq!(s.checked, vec![0], "再按取消");
        assert_eq!(
            update(&mut s, Msg::Submit),
            Cmd::Submit(Answer::Choice(vec![0])),
            "选中集优先于光标位"
        );
        // 多选空选提交 → 报错不提交（2026-09-27 修订：不再回退光标位）
        let mut bare = new_state(vec![multi_question()]);
        update(&mut bare, Msg::MoveDown);
        assert_eq!(update(&mut bare, Msg::Submit), Cmd::None, "空选不提交");
        assert!(
            bare.error.as_deref().unwrap_or("").contains("Space 选中"),
            "空选错误行: {:?}",
            bare.error
        );
        // 单选：Space 勾选（选择状态可见），提交勾选项优先
        let mut single = new_state(vec![sample_question()]);
        update(&mut single, Msg::Select);
        assert_eq!(single.checked, vec![0], "单选勾选光标位");
        update(&mut single, Msg::MoveDown);
        assert_eq!(
            update(&mut single, Msg::Submit),
            Cmd::Submit(Answer::Choice(vec![0])),
            "提交勾选项而非光标位"
        );
        // 同位再按 = 取消勾选 → 提交回退光标位
        let mut single = new_state(vec![sample_question()]);
        update(&mut single, Msg::Select);
        update(&mut single, Msg::Select);
        assert!(single.checked.is_empty(), "同位再按取消");
        update(&mut single, Msg::MoveDown);
        assert_eq!(
            update(&mut single, Msg::Submit),
            Cmd::Submit(Answer::Choice(vec![1])),
            "未勾选按光标位"
        );
        // 判分后 Select 忽略 + 下一题清空选中集
        apply_grade(&mut s, grade(Verdict::Correct));
        update(&mut s, Msg::Select);
        assert_eq!(s.checked, vec![0], "判分后不改选中");
        update(&mut s, Msg::Next);
        assert!(s.checked.is_empty(), "下一题清空选中集");
    }

    #[test]
    fn test_update_stop_edit_keeps_input_and_char_resumes() {
        let mut s = new_state(vec![fill_question(1)]);
        for c in "am".chars() {
            update(&mut s, Msg::Char(c));
        }
        assert!(s.editing, "打字即编辑");
        update(&mut s, Msg::StopEdit);
        assert!(!s.editing && s.inputs[0] == "am", "退出编辑保留输入");
        // 浏览态删改/打字直接恢复编辑
        update(&mut s, Msg::Backspace);
        assert!(s.editing && s.inputs[0] == "a", "删改即恢复编辑");
        // 判分后不再收字、StopEdit 忽略
        apply_grade(&mut s, grade(Verdict::Correct));
        update(&mut s, Msg::Char('x'));
        assert_eq!(s.inputs[0], "a", "判分后不收字符");
        update(&mut s, Msg::StopEdit);
        assert!(s.editing, "判分后 StopEdit 忽略（标志不变）");
        assert_eq!(s.key_ctx(), KeyCtx::Graded, "编辑标志不影响已判分 ctx");
    }
}
