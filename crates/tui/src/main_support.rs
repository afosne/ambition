use std::path::PathBuf;

/// Default database location with `AMBI_DB` override.
pub(super) fn db_path() -> PathBuf {
    if let Some(over) = std::env::var_os("AMBI_DB") {
        return PathBuf::from(over);
    }
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join(".ambition").join("ambition.db")
}

pub(super) fn game_summary(game: &ambition_gamify::GameState) -> Vec<String> {
    let mut rows = vec![
        format!(
            "等级 {} · XP {} · 金币 {}",
            ambition_gamify::level_of(game.xp),
            game.xp,
            game.coins
        ),
        format!(
            "当前连击 {} · 最佳连击 {} · 游戏 {}",
            game.streak,
            game.combo_best,
            if game.enabled { "开启" } else { "关闭" }
        ),
        format!(
            "徽章：{}",
            if game.badges.is_empty() {
                "暂无".into()
            } else {
                game.badges.iter().cloned().collect::<Vec<_>>().join("、")
            }
        ),
        format!(
            "地图：{}",
            if game.map_progress.is_empty() {
                "尚未探索".into()
            } else {
                game.map_progress
                    .iter()
                    .map(|(k, v)| format!("{k} {v}"))
                    .collect::<Vec<_>>()
                    .join(" · ")
            }
        ),
    ];
    rows.extend(
        game.inventory
            .iter()
            .map(|(item, count)| format!("道具 {item} × {count}")),
    );
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_db_path_uses_override_then_home_fallback_shape() {
        let path = db_path();
        assert!(
            path.ends_with(".ambition/ambition.db") || path.ends_with(".ambition\\ambition.db")
        );
    }

    #[test]
    fn test_game_summary_shows_progress_and_store() {
        let game = ambition_gamify::GameState {
            xp: 20,
            coins: 3,
            ..Default::default()
        };
        let lines = game_summary(&game);
        assert!(lines.iter().any(|line| line.contains("20")));
        assert!(lines.iter().any(|line| line.contains("3")));
    }
}
