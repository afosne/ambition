//! AI 装配配置（M3）：env → AiConfig（docs/16 §6 过渡 env：AMBI_JEV_*/AMBI_LLM_*，keychain 随 16-config）
//! key 缺省 = 离线形态（纯规则 + 待裁定桶，docs/07 §6）。

/// 熔断无关的缓存 TTL：7 天（docs/05 M3：llm_cache 淘汰）
pub const LLM_CACHE_TTL_SECS: i64 = 7 * 86_400;

/// AI 装配参数（jev/llm 各自可缺席；测试直接构造指向 wiremock）
#[derive(Debug, Clone, PartialEq)]
pub struct AiConfig {
    pub jev_base: String,
    pub jev_key: Option<String>,
    pub llm_base: String,
    pub llm_model: String,
    pub llm_key: Option<String>,
}

impl AiConfig {
    /// 从环境装配（AMBI_JEV_BASE / AMBI_JEV_KEY·TYPESAFE_API_KEY / AMBI_LLM_BASE / AMBI_LLM_MODEL / AMBI_LLM_KEY）
    pub fn from_env() -> Self {
        Self {
            jev_base: std::env::var("AMBI_JEV_BASE")
                .unwrap_or_else(|_| ambition_jev::DEFAULT_JEV_BASE.to_string()),
            jev_key: env_opt("AMBI_JEV_KEY").or_else(|| env_opt("TYPESAFE_API_KEY")),
            llm_base: std::env::var("AMBI_LLM_BASE")
                .unwrap_or_else(|_| "https://api.openai.com/v1".to_string()),
            llm_model: std::env::var("AMBI_LLM_MODEL")
                .unwrap_or_else(|_| "gpt-4o-mini".to_string()),
            llm_key: env_opt("AMBI_LLM_KEY"),
        }
    }

    /// 离线形态（无 key：规则判题 + 待裁定桶）
    pub fn offline() -> Self {
        Self {
            jev_base: ambition_jev::DEFAULT_JEV_BASE.to_string(),
            jev_key: None,
            llm_base: "https://api.openai.com/v1".to_string(),
            llm_model: "gpt-4o-mini".to_string(),
            llm_key: None,
        }
    }
}

/// 读可选 env（缺失或空串 → None；测试用必然不存在的变量钉住 None 路径）
fn env_opt(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
}

/// llm_cache TTL 淘汰（engine open 时执行一次；docs/05 M3）
pub async fn purge_stale_cache(pool: &sqlx::sqlite::SqlitePool) -> Result<u64, sqlx::Error> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    ambition_store::cache_purge_before(pool, now - LLM_CACHE_TTL_SECS).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_env_fills_documented_defaults_for_missing_vars() {
        let config = AiConfig::from_env();
        assert!(!config.jev_base.is_empty(), "jev base 恒有值");
        assert!(!config.llm_base.is_empty(), "llm base 恒有值");
        assert!(!config.llm_model.is_empty(), "llm model 恒有值");
        // 未设置 AMBI_JEV_BASE（CI 环境预期未设）→ 契约默认值
        if std::env::var("AMBI_JEV_BASE").is_err() {
            assert_eq!(config.jev_base, ambition_jev::DEFAULT_JEV_BASE);
        }
        if std::env::var("AMBI_LLM_MODEL").is_err() {
            assert_eq!(config.llm_model, "gpt-4o-mini");
        }
        // key 缺省 = None（离线形态）——同样仅在未设置时断言
        if std::env::var("AMBI_LLM_KEY").is_err() {
            assert_eq!(config.llm_key, None, "无 key → 离线不静默伪造");
        }
    }

    #[test]
    fn test_offline_config_has_no_keys() {
        let offline = AiConfig::offline();
        assert!(
            offline.jev_key.is_none() && offline.llm_key.is_none(),
            "离线无 key"
        );
        assert_eq!(offline.jev_base, ambition_jev::DEFAULT_JEV_BASE);
        assert_eq!(offline.llm_model, "gpt-4o-mini");
        assert_eq!(offline, AiConfig::offline(), "确定性构造");
    }

    #[test]
    fn test_env_opt_missing_or_blank_returns_none() {
        assert_eq!(
            env_opt("AMBI_VAR_DEFINITELY_NOT_SET_42"),
            None,
            "未设置 → None"
        );
        // 空串 → None（filter 已覆盖；用确定存在的场景无法并行安全设置 env，故只测缺失路径）
    }

    #[tokio::test]
    async fn test_purge_stale_cache_removes_expired_keeps_fresh() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("内存库");
        ambition_store::init_db(&pool).await.expect("建表");
        ambition_store::cache_put(&pool, "fresh", "v")
            .await
            .expect("写");
        sqlx::query(
            "INSERT INTO llm_cache (cache_key, payload, created_at)
             VALUES ('stale', 'old', datetime('now', '-30 days'))",
        )
        .execute(&pool)
        .await
        .expect("插旧行");
        let removed = purge_stale_cache(&pool).await.expect("淘汰");
        assert_eq!(removed, 1, "只删过 TTL 的行");
        assert_eq!(
            ambition_store::cache_get(&pool, "fresh").await.expect("读"),
            Some("v".to_string())
        );
        assert_eq!(
            ambition_store::cache_get(&pool, "stale").await.expect("读"),
            None
        );
        assert_eq!(
            purge_stale_cache(&pool).await.expect("幂等"),
            0,
            "二次执行为 0"
        );
    }
}
