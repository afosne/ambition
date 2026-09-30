//! 键位唯一入口：`KeyCtx` 路由 `key_to_msg` + 同源提示行 `hint`（docs/03 §5.3，2026-09-27 统一重构）
//! 两层规则：①恒定层 `Ctrl+Q` 任何上下文退出（raw mode 无 SIGINT）；②上下文层按 `KeyCtx` 选表。
//! Esc 统一返回主页面（仅 Home 屏 Esc 退出）；Ctrl+A 练习直达、Ctrl+R 复习直达保留。
//! 其余功能屏通过主页菜单 Enter 进入，不再设 Ctrl+字母直达（全键无冲）。
//! `KeyCtx` 由 `AppState::key_ctx()` 计算；main 不做任何键位改写（Enter→下一题也在表内）；
//! render 提示行必须取 `hint()`——键位文案与路由同一来源，禁止硬编码。

use super::*;

mod hints;
pub use hints::hint;

/// 键位上下文（键位表只此一份；新增屏 = 新增变体）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyCtx {
    /// 主页面（功能菜单选择，↑↓ 导航 · Enter 进入）
    Home,
    /// 选择题作答（未判分）：↑/↓/Space（单字母已废，docs/03 §5.3）
    Choice,
    /// 填空待答：一切可打印字符进输入行；`editing`=是否编辑中（决定 Esc 语义）
    Fill {
        editing: bool,
    },
    /// 已判分 · 待人工裁定：y/n 裁决，Enter=下一题
    Pending,
    /// 已判分：Enter=下一题
    Graded,
    /// 全部完成
    Finished,
    /// 复习屏（FSRS 翻卡；docs/03 §3 M4）：Space/Enter 翻面 · 1-4 评分 · Tab/Esc 返回
    Review,
    /// 选题面板（F1；docs/03 §5.3 2026-09-27）：↑↓ 选择 · Enter 跳转 · Esc/F1 返回
    Picker,
    /// 模板作文屏（F2，M5-W4b）：↑↓选模板 · Tab换槽 · Space阶段 · Enter成文 · Ctrl+S自查 · Esc 返回
    Scaffold,
    /// 翻译工作台主屏（M5-W7：源文输入与译文动作）
    Tx,
    /// 对照对齐屏（只读切段）
    TxAlign,
    /// 我来翻屏（输入我的译文）
    TxMyTurn,
    /// 翻译历史屏（选择/收藏/转练习）
    TxHistory,
    /// M6-W1 笔记/日记/TODO 编辑工作区
    Workspace,
    Profile,
    Game,
    Wordbook,
    /// 成本面板（#18：Ctrl+S 直达；只读展示 LLM 台账）
    Cost,
    /// 题包管理（#18：Ctrl+O 导入 / Ctrl+X 导出）
    Qpack,
    /// 练习选择屏（M7-W1：↑↓ 导航 · Enter 选择 · Esc 返回）
    PracticeSelect,
    /// 对话模块（M7-W1 占位）
    Dialog,
    /// 设置模块（M7-W1 占位）
    Settings,
}

/// 键位 → 消息（唯一键位表；docs/03 §5.3）
pub fn key_to_msg(ctx: KeyCtx, code: KeyCode, mods: KeyModifiers) -> Option<Msg> {
    // ① 恒定层：Ctrl 组合（任何上下文）——Ctrl+Q 退出；其余 Ctrl+字母 = 屏幕直达
    //    （docs/03 §5.3：Ctrl+A/Ctrl+R 直达已落地，其余屏随后续阶段）；非 Ctrl 的字母永远不进命令层
    //    ? 键位帮助浮层（#24，任何上下文可用，非字母键不违反禁单字母总则）
    if code == KeyCode::Char('?') {
        return Some(Msg::ToggleHelp);
    }
    if mods.contains(KeyModifiers::CONTROL) {
        return match code {
            KeyCode::Char('q') => Some(Msg::Quit),
            // 屏幕直达（仅保留 Ctrl+A 练习、Ctrl+R 复习——核心导航；其余功能屏走主页菜单）
            KeyCode::Char('a') => Some(Msg::Screen(Screen::Practice)),
            KeyCode::Char('r') => Some(Msg::Screen(Screen::Review)),
            // Ctrl+S 上下文语义：Workspace 保存 / Wordbook 入生词本
            KeyCode::Char('s') if ctx == KeyCtx::Workspace => Some(Msg::WorkspaceSave),
            KeyCode::Char('s') if ctx == KeyCtx::Wordbook => Some(Msg::VocabEnroll),
            // M5-W6：开模考（上下文层语义——Ctrl+M 仅答题/完成态可开卷）
            KeyCode::Char('m')
                if matches!(ctx, KeyCtx::Choice | KeyCtx::Graded | KeyCtx::Finished) =>
            {
                Some(Msg::MockStart)
            }
            _ => None,
        };
    }
    // ② Esc / Enter：语义按上下文分化
    //    Esc 统一返回主页面（仅 Home 屏 Esc 退出）；填空编辑中 Esc=退出编辑保留文本
    match code {
        KeyCode::Esc => {
            return match ctx {
                KeyCtx::Home => Some(Msg::Quit),
                KeyCtx::Fill { editing: true } => Some(Msg::StopEdit),
                KeyCtx::Pending => Some(Msg::Next), // 跳过本题（保持待裁定桶）
                KeyCtx::Review => Some(Msg::Screen(Screen::Home)),
                KeyCtx::Picker => Some(Msg::TogglePicker),
                KeyCtx::Scaffold => Some(Msg::Screen(Screen::Home)),
                KeyCtx::Tx => Some(Msg::Screen(Screen::Home)),
                KeyCtx::TxAlign | KeyCtx::TxMyTurn | KeyCtx::TxHistory => {
                    Some(Msg::Screen(Screen::Tx))
                }
                KeyCtx::Workspace => Some(Msg::Screen(Screen::Home)),
                KeyCtx::Profile => Some(Msg::Screen(Screen::Home)),
                KeyCtx::Game => Some(Msg::Screen(Screen::Home)),
                KeyCtx::Wordbook => Some(Msg::Screen(Screen::Home)),
                KeyCtx::Cost => Some(Msg::Screen(Screen::Home)),
                KeyCtx::Qpack => Some(Msg::Screen(Screen::Home)),
                KeyCtx::PracticeSelect => Some(Msg::Screen(Screen::Home)),
                KeyCtx::Dialog => Some(Msg::Screen(Screen::Home)),
                KeyCtx::Settings => Some(Msg::Screen(Screen::Home)),
                _ => Some(Msg::Screen(Screen::Home)),
            };
        }
        KeyCode::Enter => {
            return Some(match ctx {
                KeyCtx::Home => Msg::HomeEnter,
                KeyCtx::Choice | KeyCtx::Fill { .. } => Msg::Submit,
                KeyCtx::Pending => Msg::ConfirmArbitrate,
                KeyCtx::Graded | KeyCtx::Finished => Msg::Next,
                KeyCtx::Review => Msg::Flip, // 复习屏 Enter=翻面
                KeyCtx::Picker => Msg::Jump, // 面板 Enter=跳转到选中题
                KeyCtx::Scaffold => Msg::ScaffoldRender, // 模板屏 Enter=套槽成文
                KeyCtx::TxAlign => Msg::TxAlign, // 对齐视图 Enter=重对齐
                KeyCtx::Tx => Msg::TxTranslate, // 工作台 Enter=翻译
                KeyCtx::TxMyTurn => Msg::TxMyTurnSubmit, // 我来翻 Enter=对照评分
                KeyCtx::TxHistory => Msg::TxToPractice, // 历史 Enter=转练习题
                KeyCtx::Workspace => Msg::WorkspaceSave,
                KeyCtx::Profile => Msg::Screen(Screen::Home),
                KeyCtx::Game => Msg::Screen(Screen::Home),
                KeyCtx::Wordbook => Msg::VocabLookup,
                KeyCtx::Cost => Msg::Screen(Screen::Home),
                KeyCtx::Qpack => Msg::Screen(Screen::Home),
                KeyCtx::PracticeSelect => Msg::HomeEnter, // 练习选择屏 Enter=选择条目
                KeyCtx::Dialog => Msg::Screen(Screen::Home),
                KeyCtx::Settings => Msg::Screen(Screen::Home),
            });
        }
        _ => {}
    }
    // ③ 填空待答：一切可打印字符（含空格/;/q/数字）都是文本——打字即恢复编辑；
    //    空位导航：Tab/→/↓ 下一空，Shift+Tab/←/↑ 上一空（单空时 update 原地）
    if matches!(ctx, KeyCtx::Fill { .. }) {
        return match code {
            KeyCode::Backspace => Some(Msg::Backspace),
            KeyCode::Char(c) => Some(Msg::Char(c)),
            KeyCode::Tab | KeyCode::Right | KeyCode::Down => Some(Msg::MoveDown),
            KeyCode::BackTab | KeyCode::Left | KeyCode::Up => Some(Msg::MoveUp),
            _ => None,
        };
    }
    // ④ 作答 / 裁决 / 收尾（**仅非字母键：方向/Space/数字**；单字母快捷键全废，docs/03 §5.3 总则）
    match (ctx, code) {
        (KeyCtx::Home, KeyCode::Up) => Some(Msg::MoveUp),
        (KeyCtx::Home, KeyCode::Down) => Some(Msg::MoveDown),
        (KeyCtx::PracticeSelect, KeyCode::Up) => Some(Msg::MoveUp),
        (KeyCtx::PracticeSelect, KeyCode::Down) => Some(Msg::MoveDown),
        (KeyCtx::Choice, KeyCode::Down) => Some(Msg::MoveDown),
        (KeyCtx::Choice, KeyCode::Up) => Some(Msg::MoveUp),
        (KeyCtx::Choice, KeyCode::Char(' ')) => Some(Msg::Select),
        (KeyCtx::Pending, KeyCode::Down | KeyCode::Right) => Some(Msg::MoveDown),
        (KeyCtx::Pending, KeyCode::Up | KeyCode::Left) => Some(Msg::MoveUp),
        // 复习屏（docs/03 §3 M4）：Space 翻面 · 1-4=Again/Hard/Good/Easy · Tab 返回
        (KeyCtx::Review, KeyCode::Char(' ')) => Some(Msg::Flip),
        (KeyCtx::Review, KeyCode::Char(c @ '1'..='4')) => {
            Some(Msg::Rate(c.to_digit(10).unwrap_or(1) as u8))
        }
        (KeyCtx::Review, KeyCode::Tab) => Some(Msg::Screen(Screen::Home)),
        // 翻题：→ 跳过/下一题（不判分）、← 上一题（Choice/Graded，docs/03 §5.3 2026-09-27）
        (KeyCtx::Choice | KeyCtx::Graded, KeyCode::Right) => Some(Msg::Skip),
        (KeyCtx::Choice | KeyCtx::Graded, KeyCode::Left) => Some(Msg::Prev),
        // F1 选题面板（Choice/Graded/Finished 可呼出；面板内 F1 关闭）
        (KeyCtx::Choice | KeyCtx::Graded | KeyCtx::Finished, KeyCode::F(1)) => {
            Some(Msg::TogglePicker)
        }
        // F2 模板作文屏（Choice/Graded/Finished 可呼出，M5-W4b docs/03 §5.3）
        (KeyCtx::Choice | KeyCtx::Graded | KeyCtx::Finished, KeyCode::F(2)) => {
            Some(Msg::Screen(Screen::Scaffold))
        }
        (KeyCtx::Picker, KeyCode::F(1)) => Some(Msg::TogglePicker),
        // 面板内列表导航
        (KeyCtx::Picker, KeyCode::Down) => Some(Msg::MoveDown),
        (KeyCtx::Picker, KeyCode::Up) => Some(Msg::MoveUp),
        // —— 翻译工作台族（M5-W7，docs/11 §2；Enter/Esc 在②中心分化）——
        (KeyCtx::Tx, KeyCode::F(5)) => Some(Msg::TxAlign),
        (KeyCtx::Tx, KeyCode::F(6)) => Some(Msg::TxMyTurnOpen),
        (KeyCtx::Tx, KeyCode::F(7)) => Some(Msg::TxHistoryOpen),
        (KeyCtx::Tx, KeyCode::Char(c)) => Some(Msg::Char(c)),
        (KeyCtx::Tx, KeyCode::Backspace) => Some(Msg::Backspace),
        (KeyCtx::Workspace, KeyCode::Char(c)) => Some(Msg::Char(c)),
        (KeyCtx::Workspace, KeyCode::Backspace) => Some(Msg::Backspace),
        (KeyCtx::Game, KeyCode::F(n @ 1..=12)) => Some(Msg::GameAction(n)),
        (KeyCtx::Wordbook, KeyCode::Char(c)) => Some(Msg::Char(c)),
        (KeyCtx::Wordbook, KeyCode::Backspace) => Some(Msg::Backspace),
        (KeyCtx::Wordbook, KeyCode::F(3)) => Some(Msg::VocabEnroll),
        (KeyCtx::TxMyTurn, KeyCode::Char(c)) => Some(Msg::Char(c)),
        (KeyCtx::TxMyTurn, KeyCode::Backspace) => Some(Msg::Backspace),
        (KeyCtx::TxHistory, KeyCode::Up) => Some(Msg::TxHistoryUp),
        (KeyCtx::TxHistory, KeyCode::Down) => Some(Msg::TxHistoryDown),
        (KeyCtx::TxHistory, KeyCode::Char(' ')) => Some(Msg::TxToggleFavorite),
        // 判分后批注（M5-W5，F4——动作键走 F 区；Ctrl+E 保留 Writing 直达，docs/03 §5.3）
        (KeyCtx::Graded, KeyCode::F(4)) => Some(Msg::Annotate),
        // #17 讲解层①②（F5——动作键走 F 区；判分后结构化讲解 + 文字讲解）
        (KeyCtx::Graded, KeyCode::F(5)) => Some(Msg::ToggleExplain),
        // 模板作文屏（M5-W4b，docs/03 §5.3 scaffold 族）
        (KeyCtx::Scaffold, KeyCode::Up) => Some(Msg::ScaffoldUp),
        (KeyCtx::Scaffold, KeyCode::Down) => Some(Msg::ScaffoldDown),
        (KeyCtx::Scaffold, KeyCode::Tab | KeyCode::Right) => Some(Msg::ScaffoldSlotNext),
        (KeyCtx::Scaffold, KeyCode::BackTab | KeyCode::Left) => Some(Msg::ScaffoldSlotPrev),
        // M5-W4b：草稿自查（F3——动作键走 F 区；Ctrl+S 保留 Settings 直达，docs/03 §5.3）
        (KeyCtx::Scaffold, KeyCode::F(3)) => Some(Msg::ScaffoldCheck),
        (KeyCtx::Scaffold, KeyCode::Char(' ')) => Some(Msg::ScaffoldStage),
        (KeyCtx::Scaffold, KeyCode::Char(c)) => Some(Msg::Char(c)),
        (KeyCtx::Scaffold, KeyCode::Backspace) => Some(Msg::Backspace),
        _ => None,
    }
}

// 配对桩：fn key_to_msg → test_key_to_msg（同文件配对门禁，主体测试在 keys_tests.rs）
#[cfg(test)]
#[test]
fn test_key_to_msg_paired_stub() {
    // 门禁配对桩：证明 key_to_msg 被引用（完整测试在 keys_tests 子模块）
    let _ = key_to_msg(
        KeyCtx::Choice,
        crossterm::event::KeyCode::Char('q'),
        crossterm::event::KeyModifiers::CONTROL,
    );
}

#[cfg(test)]
mod keys_tests;
