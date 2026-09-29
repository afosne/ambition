//! 游戏数据模型（细分域，docs/13-gamify-growth §数据）：效果与状态

/// 游戏效果（engine 应用后落 game_state）
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GameEffect {
    Xp(i32),
    Coins(i32),
    ComboUp,
    ComboBreak,
    UnlockBadge(String),
    GiveItem(String, u32),
    MapProgress(String),
    Notify(String),
}

/// 游戏状态（投影器持有；xp 连击落 game_state，docs/13）
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GameState {
    pub xp: i64,
    /// 当前连击（ComboUp 累加、ComboBreak 清零）
    pub streak: i64,
    pub combo_best: i64,
    pub coins: i64,
    pub badges: std::collections::BTreeSet<String>,
    pub inventory: std::collections::BTreeMap<String, u32>,
    pub map_progress: std::collections::BTreeMap<String, u32>,
    pub freeze_charges: u8,
    pub double_xp: bool,
    pub enabled: bool,
}

impl Default for GameState {
    fn default() -> Self {
        Self {
            xp: 0,
            streak: 0,
            combo_best: 0,
            coins: 0,
            badges: Default::default(),
            inventory: Default::default(),
            map_progress: Default::default(),
            freeze_charges: 0,
            double_xp: false,
            enabled: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameItem {
    Hint,
    Eliminate,
    DoubleXp,
    StreakFreeze,
    Makeup,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_game_state_is_enabled_and_empty() {
        let state = GameState::default();
        assert!(state.enabled);
        assert_eq!(state.xp, 0);
        assert!(state.inventory.is_empty());
    }

    #[test]
    fn test_as_str_uses_stable_item_keys() {
        assert_eq!(GameItem::StreakFreeze.as_str(), "streak_freeze");
    }

    #[test]
    fn test_fmt_game_error_is_readable() {
        let error = GameError::InvalidRound;
        assert!(!error.to_string().is_empty());
    }
}

impl GameItem {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Hint => "hint",
            Self::Eliminate => "eliminate",
            Self::DoubleXp => "double_xp",
            Self::StreakFreeze => "streak_freeze",
            Self::Makeup => "makeup",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GameError {
    ItemUnavailable(String),
    InvalidRound,
}

impl std::fmt::Display for GameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ItemUnavailable(i) => write!(f, "道具库存不足：{i}"),
            Self::InvalidRound => write!(f, "小游戏局面无效"),
        }
    }
}
impl std::error::Error for GameError {}
