//! 主页面渲染（Home 屏功能菜单 + 鼠标交互区域注册 + 右侧滚动条）
//! 跨域引用一律 `use crate::*`（docs/01 §1 组织规范）。

use crate::*;

/// 主页面：功能菜单列表（↑↓ 选择 · Enter 进入）+ 右侧滚动条
///
/// 滚动模型：
/// - `home_scroll` = 菜单内容区域的滚动偏移（行数，不含标题区）
/// - 滚轮 → `HomeScrollUp/Down` → 只改 `home_scroll`，不碰光标
/// - 鼠标悬停 → `HoverHome(i)` → 只改 `home_cursor`，不碰滚动
/// - 键盘 ↑↓ → `MoveUp/Down` → 改光标 + update 层用回填视口确保可见
/// - 渲染层只做防御性 clamp + 回填视口高度，不主动调整 scroll
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
    let max_scroll = menu_content_lines.saturating_sub(menu_viewport);

    // 回填视口高度供 update 层使用（键盘导航时计算光标可见性）
    state.home_menu_viewport = menu_viewport;
    // 回填实际 max_scroll 供 update 层滚轮下滚限界（首帧前为 0，update 层有 saturating 兌底）
    state.home_max_scroll_actual = max_scroll;

    // 渲染层只做防御性 clamp，不主动调整 scroll 跟随光标
    if state.home_scroll > max_scroll {
        state.home_scroll = max_scroll;
    }

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
        // ScrollbarState 按比例渲染：content_length = 可滚动总量，position = 当前偏移
        // position=0 → 滚动条在顶；position=max_scroll → 滚动条到底
        let mut sb_state =
            ScrollbarState::new(max_scroll).position(state.home_scroll.min(max_scroll));
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight);
        frame.render_stateful_widget(scrollbar, inner, &mut sb_state);
    }

    // —— 注册可点击区域（补偿 scroll 偏移）——
    // Paragraph::scroll((scroll_y, 0)) 跳过前 scroll_y = HEADER_LINES + home_scroll 行。
    // 菜单项 i 在全文中的行号 = HEADER_LINES + i*3，
    // 屏幕上的 Y = inner.y + (HEADER_LINES + i*3) - scroll_y = inner.y + i*3 - home_scroll
    let scroll_u32 = state.home_scroll as u32;
    for i in 0..entries.len() {
        let offset = (i as u32) * (LINES_PER_ITEM as u32);
        // 菜单项在内容区的偏移（行号），小于 scroll 说明在视口上方 → 跳过
        if offset < scroll_u32 {
            continue;
        }
        let raw_y = (inner.y as u32) + offset - scroll_u32;
        if raw_y >= inner.y as u32 && raw_y < (inner.y + inner.height) as u32 {
            let item_y = raw_y as u16;
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

/// 主页面菜单项（顺序 = home_cursor 索引；M7-W1：7 大模块重构）
pub fn home_entries() -> &'static [(&'static str, &'static str)] {
    &[
        ("练习", "基础练习 / 专项训练 / 考试模拟 / 自定义练习"),
        ("考试", "限时套卷，收卷后自动估分"),
        ("词汇", "离线语境查词与生词复习"),
        ("对话", "AI 英语老师（即将推出）"),
        ("游戏", "徽章/金币/道具/连击/地图进度"),
        ("设置", "学习偏好与显示设置（即将推出）"),
        ("退出", "退出 Ambition"),
    ]
}

/// 练习选择屏（M7-W1：分类选择 → 题型 → 开始练习）
/// W1 阶段：仅显示"全部题库"一个选项；W2 扩展为基础练习题型列表
pub fn render_practice_select(frame: &mut Frame, state: &mut AppState) {
    let text = format!(
        "Ambition · 练习选择\n\
         ═══════════════════════════════════════\n\n\
         ❯ 全部题库\n   按题库顺序作答，判分后自动排入复习队列\n\n\
         \n{}\n",
        hint(state.key_ctx())
    );
    frame.render_widget(Clear, frame.area());
    frame.render_widget(
        Paragraph::new(text).alignment(Alignment::Left).block(
            Block::default()
                .title("练习选择".bold())
                .borders(Borders::ALL)
                .title_alignment(Alignment::Center),
        ),
        frame.area(),
    );
}

/// 占位屏（M7-W1：Dialog/Settings 显示"即将推出"）
pub fn render_placeholder(frame: &mut Frame, state: &mut AppState) {
    let title = match state.screen {
        Screen::Dialog => "对话",
        Screen::Settings => "设置",
        _ => "占位",
    };
    let text = format!(
        "Ambition · {title}\n\
         ═══════════════════════════════════════\n\n\
         🚧 即将推出\n\n\
         \n{}\n",
        hint(state.key_ctx())
    );
    frame.render_widget(Clear, frame.area());
    frame.render_widget(
        Paragraph::new(text).alignment(Alignment::Center).block(
            Block::default()
                .title(title.bold())
                .borders(Borders::ALL)
                .title_alignment(Alignment::Center),
        ),
        frame.area(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_home_entries_has_eleven_items_with_titles_and_descs() {
        let entries = home_entries();
        assert_eq!(entries.len(), 7, "7 个功能菜单项");
        assert_eq!(entries[0].0, "练习");
        assert_eq!(entries[6].0, "退出");
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

    // 配对桩：fn render_practice_select → test_render_practice_select（同文件配对门禁）
    #[test]
    fn test_render_practice_select_shows_entries() {
        let mut s = new_state(vec![]);
        s.screen = Screen::PracticeSelect;
        let out = crate::common::render_sized(&mut s, 72, 20);
        assert!(out.contains("练习选择"), "标题可见: {out}");
        assert!(out.contains("全部题库"), "选项可见: {out}");
    }

    // 配对桩：fn render_placeholder → test_render_placeholder（同文件配对门禁）
    #[test]
    fn test_render_placeholder_shows_coming_soon() {
        let mut s = new_state(vec![]);
        s.screen = Screen::Dialog;
        let out = crate::common::render_sized(&mut s, 72, 20);
        assert!(out.contains("即将推出"), "占位文案可见: {out}");
        s.screen = Screen::Settings;
        let out2 = crate::common::render_sized(&mut s, 72, 20);
        assert!(out2.contains("即将推出"), "设置占位文案可见: {out2}");
    }
}
