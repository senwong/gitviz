//! Replaces common `:shortcode:` sequences (including a subset of gitmoji)
//! with the corresponding emoji, as vscode-git-graph does.

/// Like [`replace_shortcodes`], but applies user-defined `:code:` mappings
/// first (from `.gitviz.conf`).
pub fn replace_with(input: &str, custom: &[(String, String)]) -> String {
    if custom.is_empty() {
        return replace_shortcodes(input);
    }
    let mut text = input.to_string();
    for (code, emoji) in custom {
        text = text.replace(&format!(":{code}:"), emoji);
    }
    replace_shortcodes(&text)
}

pub fn replace_shortcodes(input: &str) -> String {
    if !input.contains(':') {
        return input.to_string();
    }

    let mut output = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(start) = rest.find(':') {
        output.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        if let Some(end) = after.find(':') {
            let code = &after[..end];
            if let Some(emoji) = emoji_for(code) {
                output.push_str(emoji);
                rest = &after[end + 1..];
                continue;
            }
        }
        output.push(':');
        rest = after;
    }
    output.push_str(rest);
    output
}

fn emoji_for(code: &str) -> Option<&'static str> {
    Some(match code {
        // gitmoji
        "bug" => "🐛",
        "sparkles" => "✨",
        "zap" => "⚡️",
        "boom" => "💥",
        "fire" => "🔥",
        "art" => "🎨",
        "memo" => "📝",
        "rocket" => "🚀",
        "construction" => "🚧",
        "wrench" => "🔧",
        "ambulance" => "🚑",
        "lipstick" => "💄",
        "tada" => "🎉",
        "white_check_mark" => "✅",
        "heavy_check_mark" => "✔️",
        "x" => "❌",
        "warning" => "⚠️",
        "lock" => "🔒",
        "arrow_up" => "⬆️",
        "arrow_down" => "⬇️",
        "heavy_plus_sign" => "➕",
        "heavy_minus_sign" => "➖",
        "package" => "📦",
        "bookmark" => "🔖",
        "recycle" => "♻️",
        "pencil2" => "✏️",
        "loud_sound" => "🔊",
        "mute" => "🔇",
        "bulb" => "💡",
        "globe_with_meridians" => "🌐",
        // common
        "smile" => "😄",
        "laughing" => "😆",
        "wink" => "😉",
        "heart" => "❤️",
        "thumbsup" => "👍",
        "+1" => "👍",
        "thumbsdown" => "👎",
        "-1" => "👎",
        "eyes" => "👀",
        "tada2" => "🎉",
        "100" => "💯",
        "ok_hand" => "👌",
        "clap" => "👏",
        "pray" => "🙏",
        "thinking" => "🤔",
        "sob" => "😭",
        "cry" => "😢",
        "rage" => "😡",
        "sunglasses" => "😎",
        "robot" => "🤖",
        "trophy" => "🏆",
        "medal" => "🏅",
        "star" => "⭐",
        "star2" => "🌟",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_known_shortcodes() {
        assert_eq!(replace_shortcodes(":bug: fix"), "🐛 fix");
        assert_eq!(replace_shortcodes(":sparkles: feat :rocket:"), "✨ feat 🚀");
        assert_eq!(replace_shortcodes(":+1: nice"), "👍 nice");
    }

    #[test]
    fn custom_mappings_are_applied_first() {
        let custom = vec![("shipit".to_string(), "🚢".to_string())];
        assert_eq!(replace_with(":shipit: go", &custom), "🚢 go");
        // Built-in mappings still work.
        assert_eq!(replace_with(":bug: fix", &custom), "🐛 fix");
    }

    #[test]
    fn leaves_unknown_and_plain_colons() {
        assert_eq!(replace_shortcodes(":unknown:"), ":unknown:");
        assert_eq!(replace_shortcodes("10:30 met"), "10:30 met");
        assert_eq!(replace_shortcodes("no colons"), "no colons");
    }
}
