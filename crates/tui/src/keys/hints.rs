use super::KeyCtx;

pub fn hint(ctx: KeyCtx) -> &'static str {
    match ctx {
        KeyCtx::Home => " ↑/↓ 选择功能 · Enter 进入 · Esc 退出",
        KeyCtx::Choice => {
            " ↑/↓ 移动 · Space 勾选 · Enter 提交 · → 跳过 · F1 选题 · Ctrl+M 模考 · Esc 返回主页"
        }
        KeyCtx::Fill { editing: true } => {
            " 打字作答 · Enter 提交 · Tab/→ 下一空（Shift+Tab/← 上一空）· Esc 退出编辑（保留输入）· Esc 返回主页"
        }
        KeyCtx::Fill { editing: false } => " 打字恢复编辑（已输入保留）· Enter 提交 · Esc 返回主页",
        KeyCtx::Pending => " ←/→ 选择判对判错 · Enter 确认裁决 · Esc 跳过",
        KeyCtx::Graded => {
            " Enter/→ 下一题 · ← 上一题 · F1 选题 · F4 批注（作文/翻译）· Ctrl+M 模考 · Esc 返回主页"
        }
        KeyCtx::Finished => " Esc 返回主页",
        KeyCtx::Review => " Space/Enter 翻面 · 1-4 评分（Again/Hard/Good/Easy）· Tab/Esc 返回主页",
        KeyCtx::Picker => " ↑/↓ 选择题目 · Enter 跳转 · Esc/F1 返回",
        KeyCtx::Tx => " 打字输入源文 · Enter 翻译 · F5 对齐 · F6 我来翻 · F7 历史 · Esc 返回主页",
        KeyCtx::TxAlign => " 对照切段（literal/idiomatic/reordered/omitted）· Esc 返回工作台",
        KeyCtx::TxMyTurn => "打字输入你的译文 · Enter 对照评分 · Esc 返回工作台",
        KeyCtx::TxHistory => "↑/↓ 选择 · Enter 转练习 · Space 收藏/取消 · Esc 返回工作台",
        KeyCtx::Workspace => " 打字编辑 · Ctrl+S 保存 · Esc 返回主页",
        KeyCtx::Profile => " Esc 返回主页",
        KeyCtx::Game => "F1-F5 使用道具 · F6-F10 购买道具 · F12 开关游戏 · Esc 返回主页",
        KeyCtx::Wordbook => "输入 word | context · Enter 查词 · F3 加入生词本 · Esc 返回主页",
        KeyCtx::Scaffold => {
            " ↑/↓ 选模板 · Tab/←→ 换槽 · Space 切阶段 · Enter 成文 · F3 自查 · Esc 返回主页"
        }
        KeyCtx::Cost => " 成本台账（只读）· Esc 返回主页",
        KeyCtx::Qpack => " 题包管理（主页菜单进入）· Esc 返回主页",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hint_matches_review_actions() {
        let text = hint(KeyCtx::Review);
        assert!(text.contains("翻面"));
        assert!(text.contains("1-4"));
    }
}
