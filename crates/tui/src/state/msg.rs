//! 消息与命令词汇（从 state 拆出，docs/01 §1 文件 ≤500 行）：keys/update/render/main 共用的 Msg/Cmd

use crate::*;

/// 输入消息
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Msg {
    MoveDown,
    MoveUp,
    Submit,
    Next,
    Quit,
    /// 填空输入行追加字符
    Char(char),
    /// 填空输入行退格
    Backspace,
    /// 人工裁决：确认当前高亮的待裁定裁决（arbitration_sel 0=判对 / 1=判错 → Cmd::Arbitrate）
    ConfirmArbitrate,
    /// 自评把握 1-4
    Confidence(u8),
    /// Space 选中/取消（选择题；单选无意义，多选=切换）
    Select,
    /// Esc 退出编辑（保留已输入；打字直接恢复编辑）
    StopEdit,
    /// 切屏（Ctrl+A/Ctrl+R 直达、Tab 进出复习屏，docs/03 §5.3）
    Screen(Screen),
    /// 复习屏翻面（Space/Enter）
    Flip,
    /// 复习屏评分（1-4 = Again/Hard/Good/Easy）
    Rate(u8),
    /// 跳过/下一题（→，不判分；docs/03 §5.3 2026-09-27 修订）
    Skip,
    /// 上一题（←，状态复位已作答不保留）
    Prev,
    /// F1 选题面板开/关
    TogglePicker,
    /// 面板内跳转（Enter）
    Jump,
    /// 模板屏：上/下一个模板（↑/↓）
    ScaffoldUp,
    ScaffoldDown,
    /// 模板屏：三阶段循环（Space）
    ScaffoldStage,
    /// 模板屏：槽位前/后移（Tab/Shift+Tab、←/→）
    ScaffoldSlotNext,
    ScaffoldSlotPrev,
    /// 模板屏：套槽成文（Enter → Cmd::ScaffoldRender）
    ScaffoldRender,
    /// 模板屏：草稿自查（Ctrl+S → Cmd::ScaffoldCheck）
    ScaffoldCheck,
    /// 批注（Ctrl+E，判分后作文/翻译 → Cmd::Annotate(文本)）
    Annotate,
    /// Enter 主屏：请求出译（源文非空由状态机守卫）
    TxTranslate,
    /// F5：对齐视图（先出参考译文再对齐）
    TxAlign,
    /// F6：进入我来翻
    TxMyTurnOpen,
    /// 我来翻 Enter：对照评分
    TxMyTurnSubmit,
    /// F7：历史屏（拉列表+收藏）
    TxHistoryOpen,
    /// 历史 ↑/↓
    TxHistoryUp,
    /// 历史 ↓
    TxHistoryDown,
    /// 历史 Space：收藏/取消
    TxToggleFavorite,
    /// 历史 Enter：转练习题
    TxToPractice,
    /// 开模考（Ctrl+M；M5-W6，docs/03 §5.3）
    MockStart,
    /// M6-W1 保存当前笔记/日记/TODO 草稿
    WorkspaceSave,
    GameAction(u8),
    VocabLookup,
    VocabEnroll,
}

/// 状态机输出（Quit/Submit 由 main 执行副作用）
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cmd {
    None,
    Quit,
    Submit(Answer),
    /// 人工裁决（main 调 engine.resolve_pending）
    Arbitrate(bool),
    /// 加载复习队列（main 调 engine.due_list）
    ReviewLoad,
    /// 复习评分（main 调 engine.review_answer；u8 = 1-4 档）
    ReviewRate(u8),
    /// 装载模板列表（main 调 engine.templates）
    ScaffoldLoad,
    /// 套槽成文（main 调 engine.render_template）
    ScaffoldRender,
    /// 草稿自查（main 调 engine.check_scaffold）
    ScaffoldCheck,
    /// 批注（main 调 engine.annotate；携带提交文本）
    Annotate(String),
    /// 工作台出译（engine.translate）
    TxTranslate,
    /// 参考译文对齐（translate + translate_align 两跳）
    TxAlign,
    /// 我来翻（translate_my_turn）
    TxMyTurn,
    /// 历史+收藏拉取
    TxHistory,
    /// 收藏开关
    TxFavorite,
    /// 转练习题
    TxToPractice,
    /// 开卷（main → engine::mock_start；回填 apply_mock_start）
    MockStart,
    /// 交卷（末题 Next 触发；main → engine::mock_finish → apply_mock_finish）
    MockFinish,
    /// 读取当前笔记/日记/TODO 列表
    WorkspaceLoad,
    /// 保存当前笔记/日记/TODO 草稿
    WorkspaceSave(String),
    ProfileLoad,
    GameLoad,
    GameAction(u8),
    VocabLoad,
    VocabLookup(String),
    VocabEnroll(String),
}
