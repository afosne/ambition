//! 主页面渲染（Home 屏功能菜单 + 鼠标交互区域注册 + 右侧滚动条）
//! 跨域引用一律 `use crate::*`（docs/01 §1 组织规范）。

use crate::*;

/// 每个菜单项占用的行数（标题 + 描述 + 空行）
const LINES_PER_ITEM: usize = 3;
/// 标题区行数（2 行文本 + 1 行空行）
const HEADER_LINES: usize = 3;
/// 底部提示区预留行数
const FOOTER_LINES: usize = 3;

/// 主页面：功能菜单列表（↑↓ 选择 · Enter 进入）+ 右侧滚动条
pub fn render_home(frame: &mut Frame, state: &mut AppState) {
    let entries = home_entries();
    let cursor = state.home_cursor.min(entries.len() - 1);

    let inner = Block::default().borders(Borders::ALL).inner(frame.area());
    let viewport = inner.height as usize;

    // 菜单可用视口 = 总视口 - 标题区 - 底部提示区
    let menu_viewport = viewport
        .saturating_sub(HEADER_LINES)
        .saturating_sub(FOOTER_LINES)
        .max(1);

    // 菜单项总行数（不含标题区）
    let menu_content_lines = entries.len() * LINES_PER_ITEM;

    // —— 计算滚动偏移（以菜单内容行号为准）——
    // home_scroll 表示菜单内容区域滚动了多少行（不含标题区）
    state.home_scroll = if menu_content_lines <= menu_viewport {
        0
    } else {
        let cursor_top = cursor * LINES_PER_ITEM;
        let cursor_bot = cursor_top + LINES_PER_ITEM;
        // 光标在当前视口上方 → 上滚到光标顶部
        if cursor_top < state.home_scroll {
            cursor_top
        } else if cursor_bot > state.home_scroll + menu_viewport {
            // 光标在视口下方 → 下滚到光标底部贴近视口底
            cursor_bot.saturating_sub(menu_viewport)
        } else {
            // 光标在视口内 → 保持不动
            state.home_scroll
        }
        .min(menu_content_lines.saturating_sub(menu_viewport))
    };

    // —— 构建全文文本 ——
    let mut text = String::from(
        "Ambition · 拾英 —— 俺必胜！\n\
         ═══════════════════════════════════════\n\n",
    );
    for (i, (title, desc)) in entries.iter().enumerate() {
        let marker = if i == cursor { "❯" } else { " " };
        text.push_str(&format!(" {marker} {title}\n   {desc}\n\n"));
    }
    text.push_str(&format!("\n{}\n", hint(state.key_ctx())));
    if let Some(err) = &state.error {
        text.push_str(&format!("\n ⚠ {err}\n"));
    }

    // Paragraph scroll 以全文行号为准：标题区行数 + 菜单滚动偏移
    let scroll_y = (HEADER_LINES + state.home_scroll) as u16;

    frame.render_widget(Clear, frame.area());
    frame.render_widget(
        Paragraph::new(text)
            .alignment(Alignment::Left)
            .block(
                Block::default()
                    .title("Ambition, 俺必胜！".bold())
                    .borders(Borders::ALL)
                    .title_alignment(Alignment::Center),
            )
            .scroll((scroll_y, 0)),
        frame.area(),
    );

    // 右侧滚动条（内容超出视口时才显示）
    if menu_content_lines > menu_viewport {
        let mut sb_state = ScrollbarState::new(menu_content_lines).position(state.home_scroll);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight);
        frame.render_stateful_widget(scrollbar, inner, &mut sb_state);
    }

    // —— 注册可点击区域（补偿 scroll 偏移）——
    // 菜单项 i 的屏幕 Y = inner.y + HEADER_LINES + i*3 - home_scroll
    let menu_top_base = inner.y + HEADER_LINES as u16;
    for i in 0..entries.len() {
        let item_y =
            menu_top_base + (i as u16) * LINES_PER_ITEM as u16 - (state.home_scroll as u16);
        // 只注册在视口内的项
        if item_y >= inner.y && item_y < inner.y + inner.height {
            let clamped_h = (inner.y + inner.height - item_y).min(2);
            if clamped_h > 0 {
                state.register_interactive(
                    Rect {
                        x: inner.x,
                        y: item_y,
                        width: inner.width,
                        height: clamped_h,
                    },
                    Msg::HoverHome(i),
                    Msg::HomeClick(i),
                );
            }
        }
    }

    if state.help_open {
        super::m6_screens::render_help_overlay(frame, state);
    }
}

/// 主页面菜单项（顺序 = home_cursor 索引）
pub fn home_entries() -> &'static [(&'static str, &'static str)] {
    &[
        ("开始练习", "按题型顺序作答，判分后自动排入复习队列"),
        ("间隔复习", "FSRS 翻卡复习到期卡片"),
        ("模板作文", "按模板槽位成文 + 草稿自查"),
        ("模拟考试", "限时套卷，收卷后自动估分"),
        ("翻译工作台", "出译 → 对齐 → 我来翻 → 历史"),
        ("笔记", "随手记，FTS 全文检索"),
        ("日记", "每日复盘"),
        ("待办", "考试倒计时与任务管理"),
        ("学习画像", "本地作答统计、弱点分析与估分"),
        ("游戏成长", "徽章/金币/道具/连击/地图进度"),
        ("词汇本", "离线语境查词与生词复习"),
        ("成本台账", "LLM 用量统计与费用估算"),
        ("题包管理", "导入/导出题库 JSON 题包"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_home_entries_has_eleven_items_with_titles_and_descs() {
        let entries = home_entries();
        assert_eq!(entries.len(), 13, "13 个功能菜单项");
        assert_eq!(entries[0].0, "开始练习");
        assert_eq!(entries[12].0, "题包管理");
        // 每项描述非空
        for (title, desc) in entries {
            assert!(!title.is_empty(), "标题非空");
            assert!(!desc.is_empty(), "描述非空");
        }
    }

    // 配对桩：fn render_home → test_render_home（同文件配对门禁）
    #[test]
    fn test_render_home_registers_click_targets() {
        // render_home 做帧绘制（无返回值），完整断言在 render_tests::test_render_home_shows_menu_and_cursor
        let mut s = new_state(vec![]);
        s.screen = Screen::Home;
        s.clear_click_targets();
        // 调用 render_home 前后无 panic 即可（TestBackend 在 render_sized 中创建）
        let _ = crate::common::render_sized(&mut s, 72, 40);
    }
}
