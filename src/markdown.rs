//! Parses a small inline Markdown subset (bold, italics, inline code) into
//! styled spans. Rendering is left to the caller so this stays dependency-free.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpanStyle {
    Normal,
    Bold,
    Italic,
    BoldItalic,
    Code,
}

#[derive(Clone, Debug)]
pub struct Span {
    pub text: String,
    pub style: SpanStyle,
}

/// Parses one line into spans.
pub fn parse_line(line: &str) -> Vec<Span> {
    let mut spans = Vec::new();
    let mut plain = String::new();
    let mut chars: Vec<char> = line.chars().collect();
    // Markdown uses `*`/`_` for emphasis and `` ` `` for code.
    let mut index = 0;

    while index < chars.len() {
        let ch = chars[index];

        if ch == '`'
            && let Some(end) = find(&chars, index + 1, '`')
        {
            push(&mut spans, &mut plain, SpanStyle::Normal);
            let text: String = chars[index + 1..end].iter().collect();
            spans.push(Span {
                text,
                style: SpanStyle::Code,
            });
            index = end + 1;
            continue;
        }

        if (ch == '*' || ch == '_') && !(ch == '_' && index > 0 && chars[index - 1].is_alphanumeric())
        {
            let marker = ch;
            let triple = index + 2 < chars.len() && chars[index + 1] == marker && chars[index + 2] == marker;
            let double = index + 1 < chars.len() && chars[index + 1] == marker;

            let (count, style) = if triple {
                (3, SpanStyle::BoldItalic)
            } else if double {
                (2, SpanStyle::Bold)
            } else {
                (1, SpanStyle::Italic)
            };

            if let Some(end) = find_n(&chars, index + count, marker, count) {
                push(&mut spans, &mut plain, SpanStyle::Normal);
                let text: String = chars[index + count..end].iter().collect();
                spans.push(Span { text, style });
                index = end + count;
                continue;
            }
        }

        plain.push(ch);
        index += 1;
    }

    push(&mut spans, &mut plain, SpanStyle::Normal);
    if spans.is_empty() {
        spans.push(Span {
            text: String::new(),
            style: SpanStyle::Normal,
        });
    }
    let _ = &mut chars;
    spans
}

/// Parses a full message into lines of spans.
pub fn parse(text: &str) -> Vec<Vec<Span>> {
    text.lines().map(parse_line).collect()
}

fn push(spans: &mut Vec<Span>, plain: &mut String, style: SpanStyle) {
    if !plain.is_empty() {
        spans.push(Span {
            text: std::mem::take(plain),
            style,
        });
    }
}

fn find(chars: &[char], from: usize, target: char) -> Option<usize> {
    (from..chars.len()).find(|&i| chars[i] == target)
}

fn find_n(chars: &[char], from: usize, target: char, count: usize) -> Option<usize> {
    let mut index = from;
    while index + count <= chars.len() {
        if (0..count).all(|offset| chars[index + offset] == target) {
            return Some(index);
        }
        index += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn styles(line: &str) -> Vec<(String, SpanStyle)> {
        parse_line(line)
            .into_iter()
            .map(|span| (span.text, span.style))
            .collect()
    }

    #[test]
    fn plain_text() {
        assert_eq!(styles("hello"), vec![("hello".to_string(), SpanStyle::Normal)]);
    }

    #[test]
    fn bold_italic_code() {
        assert_eq!(styles("**hi**"), vec![("hi".to_string(), SpanStyle::Bold)]);
        assert_eq!(styles("*hi*"), vec![("hi".to_string(), SpanStyle::Italic)]);
        assert_eq!(
            styles("***hi***"),
            vec![("hi".to_string(), SpanStyle::BoldItalic)]
        );
        assert_eq!(styles("`x`"), vec![("x".to_string(), SpanStyle::Code)]);
    }

    #[test]
    fn mixed_line() {
        assert_eq!(
            styles("a **b** c"),
            vec![
                ("a ".to_string(), SpanStyle::Normal),
                ("b".to_string(), SpanStyle::Bold),
                (" c".to_string(), SpanStyle::Normal),
            ]
        );
    }

    #[test]
    fn underscore_inside_word_is_literal() {
        assert_eq!(
            styles("foo_bar"),
            vec![("foo_bar".to_string(), SpanStyle::Normal)]
        );
    }
}
