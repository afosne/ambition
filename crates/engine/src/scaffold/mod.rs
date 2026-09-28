//! 模板作文门面（M5-W4，docs/10 §1、docs/03 写作（S3））：内置+个人合并、三阶段脚手架、套槽渲染、草稿自查
//! 合并口径：个人模板同 id 覆盖内置（DB 行优先）；无 AI key 时自查按离线矩阵 → `EngineError::Offline`。

use std::collections::BTreeMap;

use crate::*;
use ambition_writing::{
    ScaffoldCheck, ScaffoldStage, ScaffoldView, Template, builtin, builtin_templates, render,
    scaffold,
};

impl Engine {
    /// 模板列表（`track=Some` 按轨过滤、`None` 全部；内置 + DB 个人模板、同 id 个人覆盖内置）
    pub async fn templates(&self, track: Option<&str>) -> Result<Vec<Template>, EngineError> {
        let mut out: Vec<Template> = builtin_templates()
            .iter()
            .filter(|t| track.is_none_or(|tr| t.track == tr))
            .cloned()
            .collect();
        for t in ambition_store::templates(&self.pool).await? {
            if let Some(tr) = track
                && t.track != tr
            {
                continue;
            }
            out.retain(|b| b.template_id != t.template_id);
            out.push(t);
        }
        Ok(out)
    }

    /// 存/覆盖个人模板（同 id 覆盖，02 §2 EssayRepo::upsert_template）
    pub async fn save_template(&self, t: &Template) -> Result<(), EngineError> {
        ambition_store::upsert_template(&self.pool, t).await?;
        Ok(())
    }

    /// 三阶段脚手架视图（Full 句模板 → Half 段落功能 → Free 空骨架）
    pub async fn scaffold(
        &self,
        template_id: &str,
        stage: ScaffoldStage,
    ) -> Result<ScaffoldView, EngineError> {
        let t = self.template_by_id(template_id).await?;
        Ok(scaffold(&t, stage))
    }

    /// 套槽成文（缺槽保留 `{{id}}` 并 tracing::warn，writing::render 口径）
    pub async fn render_template(
        &self,
        template_id: &str,
        fills: &BTreeMap<String, String>,
    ) -> Result<String, EngineError> {
        let t = self.template_by_id(template_id).await?;
        Ok(render(&t, fills))
    }

    /// 草稿自查（Jev noul×2；LLM 痕迹检测随 W5）：无 key → 离线矩阵配置提示
    pub async fn check_scaffold(
        &self,
        template_id: &str,
        essay: &str,
    ) -> Result<ScaffoldCheck, EngineError> {
        let t = self.template_by_id(template_id).await?;
        self.grader
            .check_scaffold(essay, &t)
            .await
            .map_err(from_jev)
    }

    /// 按 id 取模板：DB 个人行优先（覆盖内置）→ 内置 → NotFound
    async fn template_by_id(&self, template_id: &str) -> Result<Template, EngineError> {
        ambition_store::templates(&self.pool)
            .await?
            .into_iter()
            .find(|t| t.template_id == template_id)
            .or_else(|| builtin(template_id).cloned())
            .ok_or_else(|| EngineError::NotFound(format!("template:{template_id}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ambition_writing::TemplateSource;

    fn personal_template(template_id: &str) -> Template {
        Template {
            template_id: template_id.into(),
            track: "cet4".into(),
            genre: "email".into(),
            skeleton: vec![],
            slots: vec![],
            phrases: vec![],
            anti_patterns: vec![],
            source: TemplateSource::Personal,
        }
    }

    #[tokio::test]
    async fn test_templates_filters_track_and_merges_personal_overrides() {
        let engine = Engine::open_memory().await.expect("打开内存库");
        let builtin_list = engine.templates(Some("cet4")).await.expect("内置列表");
        assert!(!builtin_list.is_empty(), "内置 cet4 模板在列");
        assert!(builtin_list.iter().all(|t| t.track == "cet4"), "按轨过滤");

        let victim = builtin_list[0].clone();
        let mut override_t = victim.clone();
        override_t.genre = "email".to_string();
        override_t.source = TemplateSource::Personal;
        engine.save_template(&override_t).await.expect("存覆盖");

        let after = engine.templates(Some("cet4")).await.expect("合并列表");
        assert_eq!(after.len(), builtin_list.len(), "同 id 覆盖不增行");
        let merged = after
            .iter()
            .find(|t| t.template_id == victim.template_id)
            .expect("覆盖项在列");
        assert_eq!(merged.genre, "email", "个人覆盖内置字段");
        assert_eq!(merged.source, TemplateSource::Personal);

        let other = engine.templates(Some("gaokao")).await.expect("高考轨");
        assert!(other.iter().all(|t| t.track == "gaokao"), "异轨不串");
    }

    #[tokio::test]
    async fn test_save_template_roundtrips_personal_row() {
        let engine = Engine::open_memory().await.expect("打开内存库");
        let t = personal_template("t-save-test");
        engine.save_template(&t).await.expect("保存");
        let list = engine.templates(Some("cet4")).await.expect("列表");
        assert!(
            list.iter()
                .any(|x| x.template_id == "t-save-test" && x.genre == "email"),
            "个人模板入库可查"
        );
    }

    #[tokio::test]
    async fn test_scaffold_cycles_three_stages_from_builtin() {
        let engine = Engine::open_memory().await.expect("打开内存库");
        let full = engine
            .scaffold("cet4_argument_procon", ScaffoldStage::Full)
            .await
            .expect("全模板");
        assert!(!full.paragraphs.is_empty(), "Full 含骨架段落");
        assert!(
            full.paragraphs.iter().any(|p| !p.lines.is_empty()),
            "Full 含句模板"
        );
        let half = engine
            .scaffold("cet4_argument_procon", ScaffoldStage::Half)
            .await
            .expect("半模板");
        assert!(!half.paragraphs.is_empty(), "Half 保留段落功能");
        assert!(
            half.paragraphs.iter().all(|p| p.lines.is_empty()),
            "Half 隐藏句模板"
        );
        let free = engine
            .scaffold("cet4_argument_procon", ScaffoldStage::Free)
            .await
            .expect("脱模板");
        assert!(free.paragraphs.is_empty(), "Free 空骨架");
    }

    #[tokio::test]
    async fn test_render_template_fills_all_slots() {
        let engine = Engine::open_memory().await.expect("打开内存库");
        let mut fills = BTreeMap::new();
        let source = ambition_writing::builtin("gaokao_letter_invite").expect("内置模板");
        for slot in &source.slots {
            fills.insert(slot.id.clone(), slot.options_en[0].clone());
        }
        let text = engine
            .render_template("gaokao_letter_invite", &fills)
            .await
            .expect("渲染");
        assert!(!text.contains("{{"), "全槽填充无残留: {text}");
        assert!(text.contains("Dear "), "书信称呼成文: {text}");

        let missing = engine
            .render_template("gaokao_letter_invite", &BTreeMap::new())
            .await
            .expect("空槽渲染（保留占位）");
        assert!(missing.contains("{{"), "缺槽保留占位不静默: {missing}");
    }

    #[tokio::test]
    async fn test_template_by_id_prefers_builtin_then_not_found() {
        let engine = Engine::open_memory().await.expect("打开内存库");
        let t = engine
            .template_by_id("cet4_argument_procon")
            .await
            .expect("内置可取");
        assert_eq!(t.track, "cet4");
        let err = engine
            .template_by_id("no-such-template")
            .await
            .expect_err("未知 id → NotFound");
        assert!(
            matches!(err, EngineError::NotFound(_)),
            "缺失走 NotFound 口径: {err}"
        );
    }

    #[tokio::test]
    async fn test_check_scaffold_maps_offline_to_engine_offline() {
        // open_memory 无 AI key → Grader offline（离线矩阵：提示配置而非伪造结果）
        let engine = Engine::open_memory().await.expect("打开内存库");
        let err = engine
            .check_scaffold("cet4_argument_procon", "draft essay")
            .await
            .expect_err("离线 → Err");
        assert!(
            matches!(err, EngineError::Offline),
            "映射 EngineError::Offline: {err}"
        );
    }
}
