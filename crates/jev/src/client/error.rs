//! Jev 错误域：JevError/classify_jev_status + LlmError 转换（429/529 可重试，docs/05 §4）

use super::*;

/// Jev 错误
#[derive(Debug, Clone, thiserror::Error)]
pub enum JevError {
    #[error("jev http {status} (retryable={retryable})")]
    Http { status: u16, retryable: bool },
    #[error("jev invalid response: {0}")]
    Invalid(String),
    #[error("jev offline: no api key configured")]
    Offline,
    /// 记账失败不阻断判题（账本是旁路）
    #[error("jev ledger failed: {0}")]
    Ledger(String),
}

impl From<LlmError> for JevError {
    fn from(err: LlmError) -> Self {
        match err {
            LlmError::Http { status, retryable } => Self::Http { status, retryable },
            LlmError::Offline => Self::Offline,
            other => Self::Invalid(other.to_string()),
        }
    }
}

/// 状态码 → 错误分类（429/529/5xx 可重试；401/422 不可重试）
pub fn classify_jev_status(status: u16) -> JevError {
    match status {
        429 | 529 | 500..=599 => JevError::Http {
            status,
            retryable: true,
        },
        _ => JevError::Http {
            status,
            retryable: false,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_jev_status_marks_rate_limit_retryable() {
        assert!(matches!(
            classify_jev_status(429),
            JevError::Http {
                status: 429,
                retryable: true
            }
        ));
        assert!(matches!(
            classify_jev_status(529),
            JevError::Http {
                status: 529,
                retryable: true
            }
        ));
        assert!(matches!(
            classify_jev_status(422),
            JevError::Http {
                status: 422,
                retryable: false
            }
        ));
        assert!(matches!(
            classify_jev_status(401),
            JevError::Http {
                status: 401,
                retryable: false
            }
        ));
    }

    #[test]
    fn test_from_llm_error_converts_domains() {
        let from_http = JevError::from(LlmError::Http {
            status: 500,
            retryable: true,
        });
        assert!(matches!(
            from_http,
            JevError::Http {
                status: 500,
                retryable: true
            }
        ));
        assert!(matches!(
            JevError::from(LlmError::Offline),
            JevError::Offline
        ));
        let other = JevError::from(LlmError::InvalidOutput { attempts: 3 });
        assert!(matches!(other, JevError::Invalid(_)));
    }
}
