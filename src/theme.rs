//! Built-in color themes, loosely ported from popular editor color schemes so
//! the app can offer a few recognizable dark and light palettes.

use gpui::{Rgba, rgb, rgba};

#[derive(Clone)]
pub struct Theme {
    pub name: &'static str,
    pub is_dark: bool,
    pub bg: Rgba,
    pub panel: Rgba,
    /// Dimming layer behind modal overlays; must keep its alpha, so it uses
    /// [`rgba`] rather than [`rgb`] (which forces alpha to 1).
    pub overlay: Rgba,
    pub border: Rgba,
    pub text: Rgba,
    pub text_muted: Rgba,
    pub hover: Rgba,
    pub selected: Rgba,
    pub accent: Rgba,
    pub error: Rgba,
    /// Colour used for tag labels (a calm green).
    pub tag: Rgba,
    pub lane_colors: [Rgba; 8],
}

/// The colors that vary between themes; overlays and lane colors are derived
/// from [`Spec::is_dark`].
#[derive(Clone, Copy)]
struct Spec {
    name: &'static str,
    is_dark: bool,
    bg: u32,
    panel: u32,
    border: u32,
    text: u32,
    text_muted: u32,
    hover: u32,
    selected: u32,
    accent: u32,
    error: u32,
}

fn dark_lanes() -> [Rgba; 8] {
    [
        rgb(0xe06c75),
        rgb(0x61afef),
        rgb(0x98c379),
        rgb(0xe5c07b),
        rgb(0xc678dd),
        rgb(0x56b6c2),
        rgb(0xd19a66),
        rgb(0xabb2bf),
    ]
}

fn light_lanes() -> [Rgba; 8] {
    [
        rgb(0xc0392b),
        rgb(0x1a73e8),
        rgb(0x2e7d32),
        rgb(0xb58900),
        rgb(0x8e44ad),
        rgb(0x00838f),
        rgb(0xd35400),
        rgb(0x607d8b),
    ]
}

fn build(spec: Spec) -> Theme {
    Theme {
        name: spec.name,
        is_dark: spec.is_dark,
        bg: rgb(spec.bg),
        panel: rgb(spec.panel),
        overlay: if spec.is_dark {
            rgba(0x0b0d10b3)
        } else {
            rgba(0x1a1f2b4d)
        },
        border: rgb(spec.border),
        text: rgb(spec.text),
        text_muted: rgb(spec.text_muted),
        hover: rgb(spec.hover),
        selected: rgb(spec.selected),
        accent: rgb(spec.accent),
        error: rgb(spec.error),
        tag: if spec.is_dark {
            rgb(0x98c379)
        } else {
            rgb(0x2e7d32)
        },
        lane_colors: if spec.is_dark {
            dark_lanes()
        } else {
            light_lanes()
        },
    }
}

const SPECS: &[Spec] = &[
    Spec {
        name: "Dark+",
        is_dark: true,
        bg: 0x1e1e1e,
        panel: 0x252526,
        border: 0x3c3c3c,
        text: 0xd4d4d4,
        text_muted: 0x8a8a8a,
        hover: 0x2a2d2e,
        selected: 0x094771,
        accent: 0x4daafc,
        error: 0xf14c4c,
    },
    Spec {
        name: "One Dark Pro",
        is_dark: true,
        bg: 0x282c34,
        panel: 0x21252b,
        border: 0x3b4048,
        text: 0xabb2bf,
        text_muted: 0x5c6370,
        hover: 0x2c313a,
        selected: 0x3e4451,
        accent: 0x61afef,
        error: 0xe06c75,
    },
    Spec {
        name: "Dracula",
        is_dark: true,
        bg: 0x282a36,
        panel: 0x21222c,
        border: 0x44475a,
        text: 0xf8f8f2,
        text_muted: 0x6272a4,
        hover: 0x343746,
        selected: 0x44475a,
        accent: 0xbd93f9,
        error: 0xff5555,
    },
    Spec {
        name: "Nord",
        is_dark: true,
        bg: 0x2e3440,
        panel: 0x3b4252,
        border: 0x4c566a,
        text: 0xd8dee9,
        text_muted: 0x7b88a1,
        hover: 0x3b4252,
        selected: 0x434c5e,
        accent: 0x88c0d0,
        error: 0xbf616a,
    },
    Spec {
        name: "Solarized Dark",
        is_dark: true,
        bg: 0x002b36,
        panel: 0x073642,
        border: 0x0d4b56,
        text: 0x93a1a1,
        text_muted: 0x586e75,
        hover: 0x0a3a45,
        selected: 0x0d4b56,
        accent: 0x268bd2,
        error: 0xdc322f,
    },
    Spec {
        name: "GitHub Light",
        is_dark: false,
        bg: 0xffffff,
        panel: 0xf6f8fa,
        border: 0xd0d7de,
        text: 0x1f2328,
        text_muted: 0x656d76,
        hover: 0xeaeef2,
        selected: 0xddf4ff,
        accent: 0x0969da,
        error: 0xcf222e,
    },
    Spec {
        name: "One Light",
        is_dark: false,
        bg: 0xfafafa,
        panel: 0xf0f0f0,
        border: 0xd4d4d4,
        text: 0x383a42,
        text_muted: 0xa0a1a7,
        hover: 0xe8e8e8,
        selected: 0xd4e0f5,
        accent: 0x4078f2,
        error: 0xe45649,
    },
    Spec {
        name: "Solarized Light",
        is_dark: false,
        bg: 0xfdf6e3,
        panel: 0xf5efdc,
        border: 0xeee8d5,
        text: 0x586e75,
        text_muted: 0x93a1a1,
        hover: 0xeee8d5,
        selected: 0xe4ddc1,
        accent: 0x268bd2,
        error: 0xdc322f,
    },
    Spec {
        name: "Quiet Light",
        is_dark: false,
        bg: 0xffffff,
        panel: 0xf3f3f3,
        border: 0xdcdcdc,
        text: 0x333333,
        text_muted: 0x888888,
        hover: 0xe8e8e8,
        selected: 0xcde8ff,
        accent: 0x4b83cd,
        error: 0xd16969,
    },
    Spec {
        name: "Gruvbox Light",
        is_dark: false,
        bg: 0xfbf1c7,
        panel: 0xf2e5bc,
        border: 0xebdbb2,
        text: 0x3c3836,
        text_muted: 0x928374,
        hover: 0xebdbb2,
        selected: 0xd5c4a1,
        accent: 0x076678,
        error: 0x9d0006,
    },
];

impl Theme {
    pub fn all() -> Vec<Theme> {
        SPECS.iter().map(|spec| build(*spec)).collect()
    }

    pub fn names() -> Vec<&'static str> {
        SPECS.iter().map(|spec| spec.name).collect()
    }

    pub fn default_theme() -> Self {
        build(SPECS[0])
    }

    pub fn by_name(name: &str) -> Option<Self> {
        SPECS
            .iter()
            .find(|spec| spec.name == name)
            .map(|spec| build(*spec))
    }

    /// The next theme in the list, wrapping around.
    pub fn toggled(&self) -> Self {
        let index = SPECS
            .iter()
            .position(|spec| spec.name == self.name)
            .unwrap_or(0);
        build(SPECS[(index + 1) % SPECS.len()])
    }

    pub fn lane(&self, lane: usize) -> Rgba {
        self.lane_colors[lane % self.lane_colors.len()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_five_dark_and_five_light_themes() {
        let themes = Theme::all();
        assert_eq!(themes.len(), 10);
        assert_eq!(themes.iter().filter(|theme| theme.is_dark).count(), 5);
        assert_eq!(themes.iter().filter(|theme| !theme.is_dark).count(), 5);
    }

    #[test]
    fn by_name_round_trips() {
        for name in Theme::names() {
            let theme = Theme::by_name(name).expect("known theme");
            assert_eq!(theme.name, name);
        }
        assert!(Theme::by_name("nope").is_none());
    }

    #[test]
    fn toggled_cycles_through_every_theme() {
        let mut theme = Theme::default_theme();
        let start = theme.name;
        for _ in 0..10 {
            theme = theme.toggled();
        }
        assert_eq!(theme.name, start);
    }
}
