use std::fmt::Write;

pub(crate) fn escape_control_characters(text: String) -> String {
    let Some(first) = text.find(needs_escape) else {
        return text;
    };
    let mut escaped = String::with_capacity(text.len());
    escaped.push_str(&text[..first]);
    for character in text[first..].chars() {
        if needs_escape(character) {
            let _ = write!(escaped, "\\u{{{:x}}}", character as u32);
        } else {
            escaped.push(character);
        }
    }
    escaped
}

fn needs_escape(character: char) -> bool {
    character.is_control() && character != '\n' && character != '\t'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_escape_sequence_becomes_visible_text() {
        assert_eq!(
            escape_control_characters("path\u{1b}]0;pwned\u{7} here".to_string()),
            "path\\u{1b}]0;pwned\\u{7} here"
        );
    }

    #[test]
    fn layout_whitespace_survives_and_other_controls_do_not() {
        assert_eq!(
            escape_control_characters("first\nsecond\tthird\rfourth\u{0}".to_string()),
            "first\nsecond\tthird\\u{d}fourth\\u{0}"
        );
    }

    #[test]
    fn c1_controls_are_escaped_by_codepoint_and_text_is_left_alone() {
        assert_eq!(
            escape_control_characters("csi\u{9b}0m\u{7f}".to_string()),
            "csi\\u{9b}0m\\u{7f}"
        );
        let plain = "nothing to escape — même en unicode";
        assert_eq!(escape_control_characters(plain.to_string()), plain);
    }

    #[test]
    fn text_with_nothing_to_escape_comes_back_as_the_same_allocation() {
        let untouched = "first\nsecond\tthird — même en unicode".to_string();
        let address = untouched.as_ptr();
        let returned = escape_control_characters(untouched);
        assert_eq!(returned, "first\nsecond\tthird — même en unicode");
        assert_eq!(returned.as_ptr(), address);
    }

    #[test]
    fn multibyte_text_before_the_first_escape_survives() {
        assert_eq!(
            escape_control_characters("même\u{1b}]0;pwned\u{7}après".to_string()),
            "même\\u{1b}]0;pwned\\u{7}après"
        );
    }
}
