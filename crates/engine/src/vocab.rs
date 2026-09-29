//! 语境查词与生词本门面（docs/11 M6；词库数据/匹配唯一在 ambition-vocab）。

use crate::*;

pub use ambition_vocab::ContextSense;

impl Engine {
    /// 离线语境查词：返回全部义项与例句匹配的候选义项。
    pub fn lookup(&self, word: &str, context: Option<&str>) -> Result<ContextSense, EngineError> {
        ambition_vocab::lookup(word, context).map_err(EngineError::Vocab)
    }

    /// 将文本中可识别且尚未掌握的词加入 FSRS 生词复习队列。
    pub async fn vocab_new_words(
        &self,
        text: &str,
        known: &[String],
    ) -> Result<Vec<String>, EngineError> {
        let words = ambition_vocab::new_words(text, known).map_err(EngineError::Vocab)?;
        let now = ambition_core::now_unix();
        for word in &words {
            for sense in self.word_detail(word).await? {
                self.enroll_card(
                    ambition_core::CardSource::WordSense {
                        word_id: sense.word_id,
                        sense_id: sense.sense_id,
                    },
                    now,
                )
                .await?;
            }
        }
        Ok(words)
    }

    pub async fn vocab_wordbook(&self) -> Result<Vec<String>, EngineError> {
        let cards = ambition_store::list_cards(&self.pool).await?;
        let mut words = std::collections::BTreeSet::new();
        for card in cards {
            if let ambition_core::CardSource::WordSense { word_id, .. } = card.source {
                words.insert(word_id);
            }
        }
        Ok(words.into_iter().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_lookup_returns_builtin_contextual_senses() {
        let engine = Engine::open_memory().await.expect("open");
        assert!(engine.lookup("bank", Some("river bank")).is_ok());
    }

    #[tokio::test]
    async fn test_vocab_new_words_enrolls_local_lexemes() {
        let engine = Engine::open_memory().await.expect("open");
        let added = engine
            .vocab_new_words("I bank", &[])
            .await
            .expect("生词入卡");
        assert!(added.iter().any(|word| word == "bank"));
    }

    #[tokio::test]
    async fn test_vocab_wordbook_lists_enrolled_words() {
        let engine = Engine::open_memory().await.expect("open");
        engine
            .vocab_new_words("I bank", &[])
            .await
            .expect("生词入卡");
        assert!(
            engine
                .vocab_wordbook()
                .await
                .unwrap()
                .contains(&"bank".into())
        );
    }
}
