//! The colours and sizes the bar draws with.
//!
//! One rule: chrome reads a slot and never a raw hex value. A few values
//! are stated, and the rest are blended from them once, here.

use gpui::{px, rgb, App, Global, Pixels, Rgba};

/// Blend `fg` at `alpha` over `bg`, resolving to a solid color.
fn over(bg: Rgba, fg: Rgba, alpha: f32) -> Rgba {
    bg.blend(Rgba { a: alpha, ..fg })
}

#[derive(Clone, Copy, Debug)]
pub struct Theme {
    pub background: Rgba,
    pub surface: Rgba,
    pub elevated_surface: Rgba,
    pub element_hover: Rgba,
    pub border: Rgba,
    pub text: Rgba,
    pub text_muted: Rgba,
    pub accent: Rgba,
    pub error: Rgba,
    pub success: Rgba,
}

impl Theme {
    pub fn dark() -> Self {
        let background = rgb(0x000000);
        let surface = rgb(0x161616);
        let text = rgb(0xE3E3E3);
        Self {
            background,
            surface,
            elevated_surface: rgb(0x1B1B1B),
            element_hover: over(surface, text, 0.10),
            border: rgb(0x373737),
            text,
            text_muted: over(background, text, 0.60),
            accent: rgb(0x9B6DFF),
            error: rgb(0xD07277),
            success: rgb(0xA1C181),
        }
    }
}

impl Global for Theme {}

pub fn theme(cx: &App) -> &Theme {
    cx.global::<Theme>()
}

/// The height of the top bar.
pub const HEADER_BAR: Pixels = px(36.);
/// Interactive controls: buttons, chips, inputs.
pub const RADIUS_CONTROL: Pixels = px(3.);
/// Where content starts, clear of the macOS traffic lights.
pub const TOP_BAR_LEADING: Pixels = px(90.);
pub const BODY_SMALL: Pixels = px(12.);
pub const LABEL_SMALL: Pixels = px(11.);
