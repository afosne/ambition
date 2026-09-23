//! 键位收口：key_to_msg（j/k/Enter/q/;/i/x/y/n/1-4，docs/03 §5.3）

use super::*;

/// 键位 → 消息（填空输入态吞可打印字符；Esc 恒可退出，docs/03 §5.3）
pub fn key_to_msg(fill_mode: bool, code: KeyCode) -> Option<Msg> {
    match code {
        KeyCode::Esc => Some(Msg::Quit),
        KeyCode::Enter => Some(Msg::Submit),
        KeyCode::Backspace if fill_mode => Some(Msg::Backspace),
        KeyCode::Char(c) if fill_mode => Some(Msg::Char(c)),
        KeyCode::Char('q') => Some(Msg::Quit),
        KeyCode::Char('j') | KeyCode::Down => Some(Msg::MoveDown),
        KeyCode::Char('k') | KeyCode::Up => Some(Msg::MoveUp),
        // 待裁定裁决与自评把握（仅非输入态；docs/03 §5.3）
        KeyCode::Char('y') => Some(Msg::Arbitrate(true)),
        KeyCode::Char('n') => Some(Msg::Arbitrate(false)),
        KeyCode::Char(c) if ('1'..='4').contains(&c) => {
            Some(Msg::Confidence(c.to_digit(10).unwrap_or(1) as u8))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_to_msg_fill_mode_swallows_printables() {
        use Msg::*;
        assert_eq!(key_to_msg(false, KeyCode::Char('q')), Some(Quit));
        assert_eq!(key_to_msg(false, KeyCode::Char('j')), Some(MoveDown));
        assert_eq!(key_to_msg(false, KeyCode::Char('k')), Some(MoveUp));
        assert_eq!(key_to_msg(false, KeyCode::Enter), Some(Submit));
        assert_eq!(
            key_to_msg(false, KeyCode::Char('a')),
            None,
            "选择态字符无效"
        );
        assert_eq!(key_to_msg(false, KeyCode::Backspace), None);
        // 填空输入态：可打印字符进输入行，q 也输入（Esc 才退出）
        assert_eq!(key_to_msg(true, KeyCode::Char('a')), Some(Char('a')));
        assert_eq!(key_to_msg(true, KeyCode::Char('q')), Some(Char('q')));
        assert_eq!(key_to_msg(true, KeyCode::Backspace), Some(Backspace));
        assert_eq!(key_to_msg(true, KeyCode::Enter), Some(Submit));
        assert_eq!(key_to_msg(true, KeyCode::Esc), Some(Quit), "Esc 恒退出");
        assert_eq!(key_to_msg(true, KeyCode::Down), Some(MoveDown));
        assert_eq!(key_to_msg(true, KeyCode::Tab), None);
    }

    #[test]
    fn test_key_to_msg_arbitration_and_confidence_keys() {
        use Msg::*;
        assert_eq!(
            key_to_msg(false, KeyCode::Char('y')),
            Some(Arbitrate(true)),
            "y = 判对"
        );
        assert_eq!(
            key_to_msg(false, KeyCode::Char('n')),
            Some(Arbitrate(false)),
            "n = 判错"
        );
        for level in 1..=4 {
            assert_eq!(
                key_to_msg(false, KeyCode::Char(char::from(b'0' + level))),
                Some(Confidence(level)),
                "{level} 档自评"
            );
        }
        assert_eq!(key_to_msg(false, KeyCode::Char('5')), None, "5 档不存在");
        // 填空输入态：y/n/数字仍是输入字符（裁决/自评只在选择态）
        assert_eq!(key_to_msg(true, KeyCode::Char('y')), Some(Char('y')));
        assert_eq!(key_to_msg(true, KeyCode::Char('1')), Some(Char('1')));
    }
}
