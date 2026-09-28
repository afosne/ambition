//! 卷面模拟纯件（M5-W8，docs/10 §5，原 36-exam-realism §2）：字数红线 + 计时 + 收卷铃 + 纸笔违规
//!
//! 纯函数三件套（`&self` 委托在 engine 门面，docs/10 §5 落地注记）：建卷 → 扫违规 → 打字速度。

use ambition_core::EssayPrompt;

/// 卷面（题面红线 + 作答正文 + 起止时刻 + 粘贴标记；deadline 即收卷铃）
#[derive(Debug, Clone, PartialEq)]
pub struct ExamSheet {
    pub prompt: EssayPrompt,
    pub body: String,
    pub started_at: i64,
    pub deadline: i64,
    pub pasted: bool,
}

/// 卷面违规（Under/Over 字数红线 · Overtime 超时 · Pasted 禁粘贴——docs/10 §5）
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Violation {
    Under,
    Over,
    Overtime,
    Pasted,
}

/// 建卷（红线取 prompt.word_range；计时 deadline = started + minutes×60，收卷铃即此线）
pub fn exam_sheet(prompt: EssayPrompt, body: String, started_at: i64) -> ExamSheet {
    let deadline = started_at + i64::from(prompt.minutes) * 60;
    ExamSheet {
        prompt,
        body,
        started_at,
        deadline,
        pasted: false,
    }
}

/// 违规扫描（字数按空白词计 · 超时按 now≥deadline · pasted 旗标；word_range 边界 0 = 不设限）
pub fn sheet_violations(sheet: &ExamSheet, now: i64) -> Vec<Violation> {
    let mut out = Vec::new();
    let words = sheet.body.split_whitespace().count();
    let (min, max) = sheet.prompt.word_range;
    if min > 0 && words < usize::from(min) {
        out.push(Violation::Under);
    }
    if max > 0 && words > usize::from(max) {
        out.push(Violation::Over);
    }
    if now >= sheet.deadline {
        out.push(Violation::Overtime);
    }
    if sheet.pasted {
        out.push(Violation::Pasted);
    }
    out
}

/// 打字速度（04-typing 复用口径：词/分钟；计时不足 1 分钟按 1 分钟防除零）
pub fn typing_speed(sheet: &ExamSheet, now: i64) -> f32 {
    let elapsed_secs = (now - sheet.started_at).max(60);
    sheet.body.split_whitespace().count() as f32 / (elapsed_secs as f32 / 60.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sheet(body: &str) -> ExamSheet {
        let prompt = ambition_core::EssayPrompt {
            prompt_zh: "以绿色校园为题。".to_string(),
            genre: "argument".to_string(),
            track: "cet4".to_string(),
            rubric_id: "gaokao25".to_string(),
            word_range: (80, 120),
            minutes: 30,
        };
        exam_sheet(prompt, body.to_string(), 1_000)
    }

    #[test]
    fn test_exam_sheet_sets_deadline_from_prompt_minutes() {
        let s = sheet("hi");
        assert_eq!(s.deadline, 1_000 + 30 * 60, "收卷铃 = 起始 + 30 分钟");
        assert!(!s.pasted, "缺省无粘贴");
    }

    #[test]
    fn test_sheet_violations_flags_under_over_overtime_pasted() {
        // 字数不足（80 词红线，仅给2词）
        let short = sheet("one two");
        let v = sheet_violations(&short, 1_500);
        assert!(v.contains(&Violation::Under), "欠字数: {v:?}");
        assert!(
            !v.contains(&Violation::Over) && !v.contains(&Violation::Overtime),
            "此帧不越界"
        );

        // 字数超限（给 130 词）
        let long = sheet(&"w ".repeat(130));
        let v = sheet_violations(&long, 1_500);
        assert!(v.contains(&Violation::Over), "超字数: {v:?}");

        // 超时（deadline = 1000+1800=2800）
        let v = sheet_violations(&sheet("ok words"), 2_800);
        assert!(v.contains(&Violation::Overtime), "超时: {v:?}");

        // 粘贴
        let mut pasted = sheet("fine words here");
        pasted.pasted = true;
        let v = sheet_violations(&pasted, 1_000);
        assert!(v.contains(&Violation::Pasted), "禁粘贴: {v:?}");

        // 干净卷面（红线内 + 时限内 +未粘贴）→ 空
        let clean = sheet(&"word ".repeat(100));
        assert!(sheet_violations(&clean, 2_000).is_empty(), "干净卷面无违规");
    }

    #[test]
    fn test_typing_speed_is_words_per_minute() {
        let s = sheet(&"one two three four ".repeat(10));
        // 60 秒整 → 词数即 WPM
        let speed = typing_speed(&s, 1_060);
        assert!((speed - 40.0).abs() < 0.001, "40 词/分: {speed}");
        // 计时未起步（now < started）→ 按 60s 下限防除零（正数）
        let speed = typing_speed(&s, 900);
        assert!(speed > 0.0, "防除零: {speed}");
    }
}
