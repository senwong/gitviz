//! Replaces common `:shortcode:` sequences (including a subset of gitmoji)
//! with the corresponding emoji, as vscode-git-graph does.

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
