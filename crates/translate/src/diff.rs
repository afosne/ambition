//! 词/字级差异（M5-W7，docs/11 §4：my_turn 差异对齐、中英/中日 fixture）
//! 口径：拉丁按词切、含 CJK 按字切（两侧同一口径）；LCS 对齐相等 token，
//! 缺口双侧先按位配对为 Changed、单侧余量落 Missing/Extra（命中游程按序成对，确定性可钉）。
use crate::model::{DiffKind, DiffPoint};

/// 是否含 CJK（汉字区判定——按字切分口径，日文汉字同样命中）
fn has_cjk(s: &str) -> bool {
    s.chars()
        .any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c) || ('\u{3400}'..='\u{4dbf}').contains(&c))
}

/// 切 token（CJK 按字、否则按空白词——两侧同口径才可比）
fn tokenize(s: &str) -> Vec<String> {
    if has_cjk(s) {
        s.chars()
            .filter(|c| !c.is_whitespace())
            .map(|c| c.to_string())
            .collect()
    } else {
        s.split_whitespace().map(|w| w.to_string()).collect()
    }
}

/// LCS 长度表（dp[i][j] = a[i..] 与 b[j..] 的 LCS 长）
fn lcs_table(a: &[String], b: &[String]) -> Vec<Vec<usize>> {
    let (n, m) = (a.len(), b.len());
    let mut dp = vec![vec![0usize; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[i][j] = if a[i] == b[j] {
                dp[i + 1][j + 1] + 1
            } else {
                dp[i + 1][j].max(dp[i][j + 1])
            };
        }
    }
    dp
}

/// 差异对照（reference ↔ mine → Same/Changed/Missing/Extra，docs/11 §2 my_turn diff）
pub fn diff(reference: &str, mine: &str) -> Vec<DiffPoint> {
    let a = tokenize(reference);
    let b = tokenize(mine);
    if a.is_empty() && b.is_empty() {
        return Vec::new();
    }
    // LCS 回溯 → 双侧命中游标（第 k 个命中互为配对——回溯按序成对记录）
    let dp = lcs_table(&a, &b);
    let (mut i, mut j) = (0usize, 0usize);
    let mut matched_a = vec![false; a.len()];
    let mut matched_b = vec![false; b.len()];
    while i < a.len() && j < b.len() {
        if a[i] == b[j] {
            matched_a[i] = true;
            matched_b[j] = true;
            i += 1;
            j += 1;
        } else if dp[i + 1][j] >= dp[i][j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    // 游程扫描：当前起命中 → 吞相等 run；否则缺口按位配对、单侧余量 Missing/Extra
    let mut out: Vec<DiffPoint> = Vec::new();
    let (mut pa, mut pb) = (0usize, 0usize);
    while pa < a.len() || pb < b.len() {
        let na = if pa >= a.len() {
            a.len()
        } else {
            matched_a[pa..]
                .iter()
                .position(|&m| m)
                .map(|k| pa + k)
                .unwrap_or(a.len())
        };
        let nb = if pb >= b.len() {
            b.len()
        } else {
            matched_b[pb..]
                .iter()
                .position(|&m| m)
                .map(|k| pb + k)
                .unwrap_or(b.len())
        };
        // 1) 缺口块 [pa..na) × [pb..nb)：双侧按位配对、单侧余量 Missing/Extra（空块即无操作）
        let gap_a = &a[pa..na];
        let gap_b = &b[pb..nb];
        let paired = gap_a.len().min(gap_b.len());
        for k in 0..paired {
            let kind = if gap_a[k] == gap_b[k] {
                DiffKind::Same
            } else {
                DiffKind::Changed
            };
            out.push(DiffPoint {
                src: gap_a[k].clone(),
                tgt: gap_b[k].clone(),
                kind,
            });
        }
        for token in &gap_a[paired..] {
            out.push(DiffPoint {
                src: token.clone(),
                tgt: String::new(),
                kind: DiffKind::Missing,
            });
        }
        for token in &gap_b[paired..] {
            out.push(DiffPoint {
                src: String::new(),
                tgt: token.clone(),
                kind: DiffKind::Extra,
            });
        }
        pa = na;
        pb = nb;
        // 2) 命中游程（缺口之后两游标必同达命中/末尾）：第 k 命中互为配对，贪心吞相等 run
        while pa < a.len() && pb < b.len() && matched_a[pa] && matched_b[pb] {
            out.push(DiffPoint {
                src: a[pa].clone(),
                tgt: b[pb].clone(),
                kind: DiffKind::Same,
            });
            pa += 1;
            pb += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kind_seq(reference: &str, mine: &str) -> Vec<DiffKind> {
        diff(reference, mine).into_iter().map(|p| p.kind).collect()
    }

    #[test]
    fn test_diff_pairs_unequal_gap_as_changed() {
        let d = diff("I like spring", "I enjoy spring");
        assert_eq!(
            kind_seq("I like spring", "I enjoy spring"),
            vec![DiffKind::Same, DiffKind::Changed, DiffKind::Same],
            "命中词 Same、缺口按位配对 Changed"
        );
        assert_eq!((d[1].src.as_str(), d[1].tgt.as_str()), ("like", "enjoy"));
    }

    #[test]
    fn test_diff_marks_missing_on_reference_side() {
        assert_eq!(
            kind_seq("The cat sat", "The cat"),
            vec![DiffKind::Same, DiffKind::Same, DiffKind::Missing],
            "单侧缺词落 Missing"
        );
        let d = diff("The cat sat", "The cat");
        assert_eq!((d[2].src.as_str(), d[2].tgt.as_str()), ("sat", ""));
    }

    #[test]
    fn test_diff_marks_extra_on_target_side_without_breaking_pairs() {
        assert_eq!(
            kind_seq("The cat", "The big cat sat"),
            vec![
                DiffKind::Same,
                DiffKind::Extra,
                DiffKind::Same,
                DiffKind::Extra
            ],
            "插入词落 Extra、不打乱命中对齐"
        );
        let d = diff("The cat", "The big cat sat");
        assert_eq!(d[1].tgt, "big");
        assert_eq!(d[3].tgt, "sat");
    }

    #[test]
    fn test_diff_splits_cjk_per_char() {
        let d = diff("我喜欢春天", "我热爱春天");
        assert_eq!(
            kind_seq("我喜欢春天", "我热爱春天"),
            vec![
                DiffKind::Same,
                DiffKind::Changed,
                DiffKind::Changed,
                DiffKind::Same,
                DiffKind::Same
            ],
            "中日 fixture：按字对齐"
        );
        assert_eq!((d[1].src.as_str(), d[1].tgt.as_str()), ("喜", "热"));
        assert_eq!((d[2].src.as_str(), d[2].tgt.as_str()), ("欢", "爱"));
    }

    #[test]
    fn test_diff_empty_edges() {
        assert!(diff("", "").is_empty(), "双空无差异");
        assert_eq!(kind_seq("", "x"), vec![DiffKind::Extra], "源空 → 全 Extra");
        assert_eq!(
            kind_seq("x", ""),
            vec![DiffKind::Missing],
            "译空 → 全 Missing"
        );
    }

    #[test]
    fn test_has_cjk_detects_han_only() {
        assert!(has_cjk("我喜欢"), "汉字命中");
        assert!(!has_cjk("I like spring"), "纯拉丁不命中");
        assert!(has_cjk("カタカナと漢字"), "日文汉字命中");
    }

    #[test]
    fn test_tokenize_uses_single_split_policy() {
        assert_eq!(
            tokenize("I like spring"),
            vec!["I", "like", "spring"],
            "拉丁按词"
        );
        assert_eq!(tokenize("我喜欢"), vec!["我", "喜", "欢"], "CJK 按字");
        assert!(tokenize("").is_empty(), "空串无 token");
    }

    #[test]
    fn test_lcs_table_finds_common_subsequence() {
        let a = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let b = vec!["b".to_string(), "x".to_string(), "c".to_string()];
        let dp = lcs_table(&a, &b);
        assert_eq!(dp[0][0], 2, "LCS(a,b) = bc 长 2");
        assert_eq!(dp[3][3], 0, "越界终点为 0");
    }
}
