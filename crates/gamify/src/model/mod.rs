//! 游戏数据模型（细分域，docs/13-gamify-growth §数据）：效果与状态

/// 游戏效果（engine 应用后落 game_state）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameEffect {
    Xp(i32),
    ComboUp,
    ComboBreak,
}

/// 游戏状态（投影器持有；xp 连击落 game_state，docs/13）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GameState {
    pub xp: i64,
    /// 当前连击（ComboUp 累加、ComboBreak 清零）
    pub streak: i64,
}
