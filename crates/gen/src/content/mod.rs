//! qpack 题包：纯 JSON、版本化 schema（docs/06 §6 + docs/14 §1）
//! 导入 = schema 校验 + 内容哈希去重 + 低质拒收；导出携带 quality_score（随包资产骨架）。
//! 本域含错式库子组件（`content::chinglish`，docs/01 §1 细分功能目录）。

pub mod chinglish;

use std::collections::HashSet;

use ambition_core::{ContentHash, Qid, Question, canonical_json_hash};

use crate::GenError;

/// 当前 qpack schema 版本（docs/14 §1：版本化，导入时迁移/拒绝）
pub const QPACK_SCHEMA_VERSION: u32 = 1;
/// 导入低质拒收阈值（quality_score < θ 不入库，docs/06 §6）
pub const IMPORT_QUALITY_FLOOR: f32 = 0.6;

/// 包清单（docs/14 §1 Manifest）
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Manifest {
    pub pack_id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub license: String,
    pub track: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub signature: Option<String>,
}

impl Manifest {
    /// 本地包清单（无签名；签名随 M7 ed25519）
    pub fn local(name: &str, track: &str) -> Self {
        Self {
            pack_id: format!("local.{track}.{name}"),
            name: name.to_string(),
            version: "1.0.0".into(),
            author: "local".into(),
            license: "PolyForm-Noncommercial-1.0".into(),
            track: track.to_string(),
            tags: Vec::new(),
            signature: None,
        }
    }
}

/// 包内单题（qid 独立列 + 内容；随包资产 explanations/quality_score，docs/14 §1）
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PackItem {
    pub qid: String,
    pub question: Question,
    #[serde(default)]
    pub explanations: Vec<String>,
}

impl PackItem {
    /// 从题目装配（qid 一致性由 build_qpack 保证）
    fn from_question(question: &Question) -> Self {
        Self {
            qid: question.qid.0.clone(),
            question: question.clone(),
            explanations: Vec::new(),
        }
    }

    /// 携带讲解的包内题目（导入即得讲解，docs/06 §7）
    pub fn with_explanations(question: Question, explanations: Vec<String>) -> Self {
        Self {
            qid: question.qid.0.clone(),
            question,
            explanations,
        }
    }

    /// 校验 qid 与内容哈希一致（防篡改：内容变了但 qid 没变 = 拒绝）
    pub fn verify(&self) -> Result<(), GenError> {
        let recomputed = Question::new(
            self.question.stem.clone(),
            self.question.kind.clone(),
            self.question.difficulty,
            self.question.tags.clone(),
            self.question.provenance,
        );
        if recomputed.qid != Qid(self.qid.clone()) {
            return Err(GenError::Pack(format!(
                "qid mismatch: declared {} but content hashes to {}",
                self.qid, recomputed.qid.0
            )));
        }
        Ok(())
    }
}

/// 题包（根带 schema_version，docs/01 §2 JSON 协议）
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct QPack {
    pub schema_version: u32,
    pub manifest: Manifest,
    pub questions: Vec<PackItem>,
}

impl QPack {
    /// 内容哈希（manifest+questions 的 canonical hash；PackRef ambi://pack/<hash>）
    pub fn content_hash(&self) -> ContentHash {
        canonical_json_hash(&(self.schema_version, &self.manifest, &self.questions))
    }
}

/// 构建题包（qid 以题目现值为准）
pub fn build_qpack(questions: &[Question], manifest: Manifest) -> QPack {
    QPack {
        schema_version: QPACK_SCHEMA_VERSION,
        manifest,
        questions: questions.iter().map(PackItem::from_question).collect(),
    }
}

/// 解析题包：JSON → schema 版本校验 → 逐题 qid 防篡改校验
pub fn parse_qpack(bytes: &[u8]) -> Result<QPack, GenError> {
    let text = std::str::from_utf8(bytes).map_err(|e| GenError::Pack(format!("not utf-8: {e}")))?;
    let pack: QPack = serde_json::from_str(text)
        .map_err(|e| GenError::Pack(format!("invalid qpack json: {e}")))?;
    if pack.schema_version != QPACK_SCHEMA_VERSION {
        return Err(GenError::Pack(format!(
            "unsupported schema_version {} (supported: {QPACK_SCHEMA_VERSION})",
            pack.schema_version
        )));
    }
    for item in &pack.questions {
        item.verify()?;
    }
    Ok(pack)
}

/// 导入计划（纯函数：去重 + 低质拒收；engine 按计划落库）
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ImportPlan {
    pub accepted: Vec<Question>,
    pub duplicates: Vec<Qid>,
    pub low_quality: Vec<Qid>,
}

/// 按既有 qid 集与质量阈值规划导入（同 qid 跳过；quality < floor 拒收）
pub fn plan_import(pack: &QPack, existing: &HashSet<String>, floor: f32) -> ImportPlan {
    let mut plan = ImportPlan::default();
    for item in &pack.questions {
        if existing.contains(&item.qid) {
            plan.duplicates.push(Qid(item.qid.clone()));
            continue;
        }
        if item.question.quality_score < floor {
            plan.low_quality.push(Qid(item.qid.clone()));
            continue;
        }
        plan.accepted.push(item.question.clone());
    }
    plan
}

#[cfg(test)]
mod tests {
    use super::*;
    use ambition_core::{BlankItem, ChoiceBody, FillBody, OptionItem, Provenance, QuestionKind};

    fn choice_q(difficulty: u8) -> Question {
        Question::new(
            "The cat ___ on the mat.".into(),
            QuestionKind::SingleChoice(ChoiceBody {
                options: vec![
                    OptionItem {
                        id: 0,
                        text: "sit".into(),
                    },
                    OptionItem {
                        id: 1,
                        text: "sits".into(),
                    },
                ],
                answer: vec![1],
            }),
            difficulty,
            vec!["grammar".into()],
            Provenance::Seed,
        )
    }

    fn fill_q() -> Question {
        Question::new(
            "I ___ a student.".into(),
            QuestionKind::FillBlank(FillBody {
                sentence: "I ___ a student.".into(),
                blanks: vec![BlankItem {
                    acceptable: vec!["am".into()],
                }],
            }),
            1,
            vec!["grammar".into()],
            Provenance::Seed,
        )
    }

    #[test]
    fn test_local_manifest_defaults_license_and_pack_id() {
        let m = Manifest::local("starter", "cet4");
        assert_eq!(m.pack_id, "local.cet4.starter");
        assert_eq!(m.license, "PolyForm-Noncommercial-1.0");
        assert_eq!(m.track, "cet4");
        assert!(m.signature.is_none(), "M2 无签名（M7 ed25519）");
        assert_eq!(m.version, "1.0.0");
    }

    #[test]
    fn test_build_qpack_parse_roundtrip_is_hash_stable() {
        let questions = vec![choice_q(2), fill_q()];
        let pack = build_qpack(&questions, Manifest::local("p", "cet4"));
        assert_eq!(pack.schema_version, QPACK_SCHEMA_VERSION);
        assert_eq!(pack.questions.len(), 2);

        let bytes = serde_json::to_vec(&pack).expect("序列化");
        let reparsed = parse_qpack(&bytes).expect("解析");
        assert_eq!(reparsed, pack, "round-trip 无损");
        assert_eq!(
            reparsed.content_hash(),
            pack.content_hash(),
            "同内容同哈希（PackRef 防篡改）"
        );

        let other = build_qpack(&[choice_q(3)], Manifest::local("p", "cet4"));
        assert_ne!(other.content_hash(), pack.content_hash(), "内容变则哈希变");
    }

    #[test]
    fn test_parse_qpack_rejects_bad_version_bad_json_and_tampered_qid() {
        let pack = build_qpack(&[choice_q(2)], Manifest::local("p", "cet4"));
        let mut bytes = serde_json::to_vec(&pack).expect("序列化");
        assert!(parse_qpack(&bytes).is_ok());

        bytes = b"not json at all".to_vec();
        assert!(
            matches!(parse_qpack(&bytes), Err(GenError::Pack(_))),
            "非法 JSON → Pack 错误"
        );

        let mut wrong_version = pack.clone();
        wrong_version.schema_version = 99;
        let bytes = serde_json::to_vec(&wrong_version).expect("序列化");
        assert!(
            matches!(parse_qpack(&bytes), Err(GenError::Pack(_))),
            "版本不支持 → 拒绝（docs/22 题包格式变动对策）"
        );

        let mut tampered = pack.clone();
        tampered.questions[0].question.stem = "Tampered stem.".into();
        let bytes = serde_json::to_vec(&tampered).expect("序列化");
        let err = parse_qpack(&bytes).expect_err("篡改应拒");
        assert!(
            err.to_string().contains("qid mismatch"),
            "防篡改校验: {err}"
        );
    }

    #[test]
    fn test_from_question_clones_question_and_qid() {
        let q = choice_q(2);
        let item = PackItem::from_question(&q);
        assert_eq!(item.qid, q.qid.0);
        assert_eq!(item.question, q);
        assert!(item.explanations.is_empty());
    }

    #[test]
    fn test_content_hash_tracks_manifest_and_items() {
        let base = build_qpack(&[choice_q(2)], Manifest::local("p", "cet4"));
        let same = build_qpack(&[choice_q(2)], Manifest::local("p", "cet4"));
        assert_eq!(base.content_hash(), same.content_hash(), "同内容同哈希");
        let renamed = build_qpack(&[choice_q(2)], Manifest::local("q", "cet4"));
        assert_ne!(
            base.content_hash(),
            renamed.content_hash(),
            "manifest 变则哈希变"
        );
        assert_eq!(base.content_hash().0.len(), 64, "sha256 全长");
    }

    #[test]
    fn test_verify_accepts_consistent_and_rejects_drift() {
        let q = choice_q(2);
        let item = PackItem::from_question(&q);
        assert!(item.verify().is_ok());
        let mut bad = item.clone();
        bad.qid = "0000000000000000".into();
        assert!(bad.verify().is_err(), "qid 漂移 → Err");
    }

    #[test]
    fn test_with_explanations_carries_assets_through_roundtrip() {
        let q = fill_q();
        let item = PackItem::with_explanations(q, vec!["Use am with I.".into()]);
        let pack = QPack {
            schema_version: QPACK_SCHEMA_VERSION,
            manifest: Manifest::local("p", "cet4"),
            questions: vec![item],
        };
        let bytes = serde_json::to_vec(&pack).expect("序列化");
        let back = parse_qpack(&bytes).expect("解析");
        assert_eq!(
            back.questions[0].explanations,
            vec!["Use am with I.".to_string()],
            "随包讲解 round-trip 无损（导入即得讲解）"
        );
    }

    #[test]
    fn test_plan_import_dedupes_and_rejects_low_quality() {
        let mut high = choice_q(2);
        high.quality_score = 0.9;
        let mut low = fill_q();
        low.quality_score = 0.3;
        let pack = build_qpack(&[high.clone(), low.clone()], Manifest::local("p", "cet4"));

        let empty = HashSet::new();
        let plan = plan_import(&pack, &empty, IMPORT_QUALITY_FLOOR);
        assert_eq!(plan.accepted, vec![high.clone()], "高质题入库");
        assert_eq!(plan.low_quality, vec![low.qid.clone()], "低质题拒收");

        let mut seen = HashSet::new();
        seen.insert(high.qid.0.clone());
        let plan2 = plan_import(&pack, &seen, IMPORT_QUALITY_FLOOR);
        assert!(plan2.accepted.is_empty());
        assert_eq!(plan2.duplicates, vec![high.qid.clone()], "同 qid 去重");
        assert_eq!(plan2.low_quality, vec![low.qid.clone()]);
    }

    #[test]
    fn test_plan_import_defaults_are_empty() {
        let plan = ImportPlan::default();
        assert!(plan.accepted.is_empty());
        assert!(plan.duplicates.is_empty());
        assert!(plan.low_quality.is_empty());
    }

    #[test]
    fn test_import_quality_floor_is_documented_value() {
        assert!((IMPORT_QUALITY_FLOOR - 0.6).abs() < f32::EPSILON);
    }
}
