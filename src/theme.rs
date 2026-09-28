//! A tiny color palette so the app can offer a dark and a light theme without
//! pulling in Zed's `theme` crate.

use gpui::{Rgba, rgb};

#[derive(Clone)]
pub struct Theme {
    pub name: &'static str,
    pub bg: Rgba,
    pub panel: Rgba,
    pub overlay: Rgba,
    pub border: Rgba,
    pub text: Rgba,
    pub text_muted: Rgba,
    pub hover: Rgba,
    pub selected: Rgba,
    pub accent: Rgba,
    pub error: Rgba,
    pub lane_colors: [Rgba; 8],
}

impl Theme {
    pub fn dark() -> Self {
        Self {
            name: "dark",
            bg: rgb(0x141414),
            panel: rgb(0x1e1e1e),
            overlay: rgb(0x000000cc),
            border: rgb(0x333333),
            text: rgb(0xd0d0d0),
            text_muted: rgb(0x9aa0a6),
            hover: rgb(0x2a2a2a),
            selected: rgb(0x2f4f6f),
            accent: rgb(0x61afef),
            error: rgb(0xff6b6b),
            lane_colors: [
                rgb(0xe06c75),
                rgb(0x61afef),
                rgb(0x98c379),
                rgb(0xe5c07b),
                rgb(0xc678dd),
                rgb(0x56b6c2),
                rgb(0xd19a66),
                rgb(0xabb2bf),
            ],
        }
    }

    pub fn light() -> Self {
        Self {
            name: "light",
            bg: rgb(0xffffff),
            panel: rgb(0xf3f4f6),
            overlay: rgb(0xffffffcc),
            border: rgb(0xd0d5dd),
            text: rgb(0x1f2933),
            text_muted: rgb(0x6b7280),
            hover: rgb(0xeceff3),
            selected: rgb(0xd6e4ff),
            accent: rgb(0x1a73e8),
            error: rgb(0xc0392b),
            lane_colors: [
                rgb(0xc0392b),
                rgb(0x1a73e8),
                rgb(0x2e7d32),
                rgb(0xb58900),
                rgb(0x8e44ad),
                rgb(0x00838f),
                rgb(0xd35400),
                rgb(0x607d8b),
            ],
        }
    }

    pub fn toggled(&self) -> Self {
        if self.name == "dark" {
            Self::light()
        } else {
            Self::dark()
        }
    }

    pub fn lane(&self, lane: usize) -> Rgba {
        self.lane_colors[lane % self.lane_colors.len()]
    }
}
