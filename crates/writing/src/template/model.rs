//! 模板类型（docs/10 §1 M5-W4 精确化契约；serde snake_case 对齐 `data/templates/*.json`）

/// 模板来源（内置库 / 用户个人模板）
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TemplateSource {
    Builtin,
    Personal,
}

impl Default for TemplateSource {
    /// 内置文件不写 source 字段 → 默认 Builtin（个人模板在 DB 显式标 Personal）
    fn default() -> Self {
        Self::Builtin
    }
}

/// 段落功能（骨架一环；`lines` = 该段句模板，含 `{{slot_id}}` 占位）
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ParaFunc {
    /// 功能键（hook|thesis|body|conclusion…，英文进 state）
    pub key: String,
    /// 功能中文名（题面/提示展示）
    pub label_zh: String,
    /// 该段句模板（含 `{{slot_id}}` 占位；Full 阶段照抄替换）
    pub lines: Vec<String>,
}

/// 内容槽位（`{{id}}` 出现在 ParaFunc.lines；options_en = 槽位备选高级句）
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Slot {
    pub id: String,
    pub label_zh: String,
    /// 备选高级句（地道替换，防模板痕迹）
    pub options_en: Vec<String>,
}

/// 亮点词汇/句式（语域分档；docs/09 句式联动）
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Phrase {
    pub text: String,
    /// 语域（neutral|formal|academic|informal…）
    pub register: String,
    /// 难度档 1..=5
    pub level: u8,
}

/// 反模板黑名单（模板痕迹句式 → 批改/自查提示）
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AntiPattern {
    pub pattern_en: String,
    pub note_zh: String,
    /// 升级替换句（upgrade_sentence 整句命中即换；docs/10 §1 M5-W5）
    pub replacement_en: String,
}

/// 模板（结构骨架 + 槽位 + 亮点 + 反模板；docs/10 §1）
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Template {
    pub template_id: String,
    pub track: String,
    /// 体裁（letter|notice|continuation|summary|argument|chart|email…）
    pub genre: String,
    pub skeleton: Vec<ParaFunc>,
    pub slots: Vec<Slot>,
    pub phrases: Vec<Phrase>,
    pub anti_patterns: Vec<AntiPattern>,
    /// 缺省 = Builtin（内置文件不写该字段）
    #[serde(default)]
    pub source: TemplateSource,
}

/// 填槽三阶段（脚手架渐退，docs/10 §1.3）
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScaffoldStage {
    /// 全模板：句模板含 `{{}}` 占位，照抄替换
    Full,
    /// 半模板：只给段落功能提示（隐藏句模板）
    Half,
    /// 脱模板：自由写（空骨架，仅模板元信息）
    Free,
}

/// 脚手架视图的一段（Full = 句模板；Half/Free = 只保留段落功能名）
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ScaffoldPara {
    pub label_zh: String,
    pub lines: Vec<String>,
}

/// 脚手架视图（TUI scaffold 屏按 stage 渲染，docs/03 表 scaffold 族）
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ScaffoldView {
    pub template_id: String,
    pub stage: ScaffoldStage,
    pub paragraphs: Vec<ScaffoldPara>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_is_builtin_for_missing_source() {
        assert!(
            matches!(TemplateSource::default(), TemplateSource::Builtin),
            "内置文件缺 source 字段 → 默认 Builtin"
        );
        let json = r#"{"template_id":"t","track":"cet4","genre":"argument","skeleton":[],"slots":[],"phrases":[],"anti_patterns":[]}"#;
        let t: Template = serde_json::from_str(json).expect("缺 source 字段可读");
        assert_eq!(t.source, TemplateSource::Builtin, "serde(default) 兜底");
    }
}

/// 脚手架自查结果（M5-W4，docs/10 §1；Jev noul×2 双 prob、均 ≥0.5 → passed；LLM 模板痕迹检测随 W5）
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ScaffoldCheck {
    /// 草稿保留所填槽位原意的概率（正面表述 noul）
    pub meaning_kept: f32,
    /// 草稿贴合体裁/目的的概率（正面表述 noul）
    pub fits_prompt: f32,
    /// 双项过线 = 通过
    pub passed: bool,
}
