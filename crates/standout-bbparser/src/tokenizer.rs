use crate::ansi::ansi_units;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Token<'a> {
    Text {
        content: &'a str,
        start: usize,
        end: usize,
    },
    OpenTag {
        name: &'a str,
        start: usize,
        end: usize,
    },
    CloseTag {
        name: &'a str,
        start: usize,
        end: usize,
    },
    InvalidTag {
        content: &'a str,
        start: usize,
        end: usize,
    },
}

// O(N) instead of O(N^2): pre-computes which OpenTag tokens have a matching
// CloseTag.
pub(super) fn compute_valid_tags(tokens: &[Token<'_>]) -> std::collections::HashSet<usize> {
    use std::collections::{HashMap, HashSet};
    let mut valid_indices = HashSet::new();
    let mut open_indices_by_tag: HashMap<&str, Vec<usize>> = HashMap::new();

    for (index, token) in tokens.iter().enumerate() {
        match token {
            Token::OpenTag { name, .. } => {
                open_indices_by_tag.entry(name).or_default().push(index);
            }
            Token::CloseTag { name, .. } => {
                if let Some(indices) = open_indices_by_tag.get_mut(name) {
                    if let Some(open_index) = indices.pop() {
                        valid_indices.insert(open_index);
                    }
                }
            }
            _ => {}
        }
    }

    valid_indices
}

// ANSI controls are skipped as terminal syntax. Byte-level scanning is safe
// here: `\`, `[`, `]` are ASCII and cannot be UTF-8 continuation bytes.
fn find_unescaped_bracket(s: &str) -> Option<usize> {
    for unit in ansi_units(s) {
        if unit.is_escape {
            continue;
        }
        let bytes = unit.text.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'\\' && i + 1 < bytes.len() {
                let next = bytes[i + 1];
                if matches!(next, b'[' | b']' | b'\\') {
                    i += 2;
                    continue;
                }
            }
            if bytes[i] == b'[' {
                return Some(unit.offset + i);
            }
            i += 1;
        }
    }
    None
}

pub(super) fn unescape(s: &str) -> std::borrow::Cow<'_, str> {
    let bytes = s.as_bytes();
    let has_escape = bytes
        .windows(2)
        .any(|w| w[0] == b'\\' && matches!(w[1], b'[' | b']' | b'\\'));
    if !has_escape {
        return std::borrow::Cow::Borrowed(s);
    }
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(&next) = chars.peek() {
                if matches!(next, '[' | ']' | '\\') {
                    out.push(next);
                    chars.next();
                    continue;
                }
            }
        }
        out.push(c);
    }
    std::borrow::Cow::Owned(out)
}

pub(super) struct Tokenizer<'a> {
    input: &'a str,
    pos: usize,
}

impl<'a> Tokenizer<'a> {
    pub(super) fn new(input: &'a str) -> Self {
        Self { input, pos: 0 }
    }
}

pub fn is_valid_tag_name(s: &str) -> bool {
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return false;
    };

    if !first.is_ascii_lowercase() && first != '_' {
        return false;
    }

    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

impl<'a> Iterator for Tokenizer<'a> {
    type Item = Token<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.pos >= self.input.len() {
            return None;
        }

        let remaining = &self.input[self.pos..];
        let start_pos = self.pos;

        if let Some(bracket_pos) = find_unescaped_bracket(remaining) {
            if bracket_pos > 0 {
                let text = &remaining[..bracket_pos];
                self.pos += bracket_pos;
                return Some(Token::Text {
                    content: text,
                    start: start_pos,
                    end: self.pos,
                });
            }

            if let Some(close_bracket) = remaining.find(']') {
                let tag_content = &remaining[1..close_bracket];
                let full_tag = &remaining[..=close_bracket];
                let end_pos = start_pos + close_bracket + 1;

                if let Some(tag_name) = tag_content.strip_prefix('/') {
                    if is_valid_tag_name(tag_name) {
                        self.pos = end_pos;
                        Some(Token::CloseTag {
                            name: tag_name,
                            start: start_pos,
                            end: end_pos,
                        })
                    } else {
                        self.pos = end_pos;
                        Some(Token::InvalidTag {
                            content: full_tag,
                            start: start_pos,
                            end: end_pos,
                        })
                    }
                } else if is_valid_tag_name(tag_content) {
                    self.pos = end_pos;
                    Some(Token::OpenTag {
                        name: tag_content,
                        start: start_pos,
                        end: end_pos,
                    })
                } else {
                    self.pos = end_pos;
                    Some(Token::InvalidTag {
                        content: full_tag,
                        start: start_pos,
                        end: end_pos,
                    })
                }
            } else {
                let end_pos = self.input.len();
                self.pos = end_pos;
                Some(Token::Text {
                    content: remaining,
                    start: start_pos,
                    end: end_pos,
                })
            }
        } else {
            let end_pos = self.input.len();
            self.pos = end_pos;
            Some(Token::Text {
                content: remaining,
                start: start_pos,
                end: end_pos,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    mod tag_names {
        use super::*;

        #[test]
        fn valid_simple_names() {
            assert!(is_valid_tag_name("bold"));
            assert!(is_valid_tag_name("red"));
            assert!(is_valid_tag_name("a"));
        }

        #[test]
        fn valid_with_underscore() {
            assert!(is_valid_tag_name("my_style"));
            assert!(is_valid_tag_name("_private"));
            assert!(is_valid_tag_name("a_b_c"));
        }

        #[test]
        fn valid_with_hyphen() {
            assert!(is_valid_tag_name("my-style"));
            assert!(is_valid_tag_name("font-bold"));
            assert!(is_valid_tag_name("a-b-c"));
        }

        #[test]
        fn valid_with_numbers() {
            assert!(is_valid_tag_name("h1"));
            assert!(is_valid_tag_name("col2"));
            assert!(is_valid_tag_name("style123"));
        }

        #[test]
        fn invalid_starts_with_digit() {
            assert!(!is_valid_tag_name("1style"));
            assert!(!is_valid_tag_name("123"));
        }

        #[test]
        fn invalid_starts_with_hyphen() {
            assert!(!is_valid_tag_name("-style"));
            assert!(!is_valid_tag_name("-1"));
        }

        #[test]
        fn invalid_uppercase() {
            assert!(!is_valid_tag_name("Bold"));
            assert!(!is_valid_tag_name("BOLD"));
            assert!(!is_valid_tag_name("myStyle"));
        }

        #[test]
        fn invalid_special_chars() {
            assert!(!is_valid_tag_name("my.style"));
            assert!(!is_valid_tag_name("my@style"));
            assert!(!is_valid_tag_name("my style"));
        }

        #[test]
        fn invalid_empty() {
            assert!(!is_valid_tag_name(""));
        }
    }
    mod tokenizer {
        use super::*;

        #[test]
        fn tokenize_plain_text() {
            let tokens: Vec<_> = Tokenizer::new("hello world").collect();
            assert_eq!(
                tokens,
                vec![Token::Text {
                    content: "hello world",
                    start: 0,
                    end: 11
                }]
            );
        }

        #[test]
        fn tokenize_single_tag() {
            let tokens: Vec<_> = Tokenizer::new("[bold]hello[/bold]").collect();
            assert_eq!(
                tokens,
                vec![
                    Token::OpenTag {
                        name: "bold",
                        start: 0,
                        end: 6
                    },
                    Token::Text {
                        content: "hello",
                        start: 6,
                        end: 11
                    },
                    Token::CloseTag {
                        name: "bold",
                        start: 11,
                        end: 18
                    },
                ]
            );
        }

        #[test]
        fn tokenize_nested_tags() {
            let tokens: Vec<_> = Tokenizer::new("[a][b]x[/b][/a]").collect();
            assert_eq!(
                tokens,
                vec![
                    Token::OpenTag {
                        name: "a",
                        start: 0,
                        end: 3
                    },
                    Token::OpenTag {
                        name: "b",
                        start: 3,
                        end: 6
                    },
                    Token::Text {
                        content: "x",
                        start: 6,
                        end: 7
                    },
                    Token::CloseTag {
                        name: "b",
                        start: 7,
                        end: 11
                    },
                    Token::CloseTag {
                        name: "a",
                        start: 11,
                        end: 15
                    },
                ]
            );
        }

        #[test]
        fn tokenize_invalid_tag() {
            let tokens: Vec<_> = Tokenizer::new("[123]text[/123]").collect();
            assert_eq!(
                tokens,
                vec![
                    Token::InvalidTag {
                        content: "[123]",
                        start: 0,
                        end: 5
                    },
                    Token::Text {
                        content: "text",
                        start: 5,
                        end: 9
                    },
                    Token::InvalidTag {
                        content: "[/123]",
                        start: 9,
                        end: 15
                    },
                ]
            );
        }

        #[test]
        fn tokenize_mixed() {
            let tokens: Vec<_> = Tokenizer::new("a[b]c[/b]d").collect();
            assert_eq!(
                tokens,
                vec![
                    Token::Text {
                        content: "a",
                        start: 0,
                        end: 1
                    },
                    Token::OpenTag {
                        name: "b",
                        start: 1,
                        end: 4
                    },
                    Token::Text {
                        content: "c",
                        start: 4,
                        end: 5
                    },
                    Token::CloseTag {
                        name: "b",
                        start: 5,
                        end: 9
                    },
                    Token::Text {
                        content: "d",
                        start: 9,
                        end: 10
                    },
                ]
            );
        }
    }
}
