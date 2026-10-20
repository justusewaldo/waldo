//! Rectangles, and the one mapping between screen points and image pixels.

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub fn center(&self) -> (f64, f64) {
        (self.x + self.w / 2.0, self.y + self.h / 2.0)
    }

    pub fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }

    pub fn intersects(&self, o: &Rect) -> bool {
        self.x < o.x + o.w && o.x < self.x + self.w && self.y < o.y + o.h && o.y < self.y + self.h
    }

    /// True when `o` sits inside this rect, with one point of slack.
    pub fn contains_rect(&self, o: &Rect) -> bool {
        o.x >= self.x - 1.0
            && o.y >= self.y - 1.0
            && o.x + o.w <= self.x + self.w + 1.0
            && o.y + o.h <= self.y + self.h + 1.0
    }
}

/// The model works in one coordinate space: the pixels of the screenshot it
/// was last shown. Every tool converts through this type and nowhere else.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    /// 1-based display number.
    pub display: usize,
    /// Top-left of the display in global screen points.
    pub origin_x: f64,
    pub origin_y: f64,
    /// Image pixels per screen point.
    pub scale: f64,
    pub img_w: u32,
    pub img_h: u32,
}

impl View {
    /// `native` is the display's real pixels per point. The image is never
    /// larger than the real pixels and never longer than `max_edge`.
    pub fn for_display(display: usize, bounds: Rect, native: f64, max_edge: f64) -> View {
        let long = bounds.w.max(bounds.h).max(1.0);
        let scale = (max_edge / long).min(native.max(1.0));
        View {
            display,
            origin_x: bounds.x,
            origin_y: bounds.y,
            scale,
            img_w: (bounds.w * scale).round() as u32,
            img_h: (bounds.h * scale).round() as u32,
        }
    }

    pub fn to_image(&self, gx: f64, gy: f64) -> (f64, f64) {
        ((gx - self.origin_x) * self.scale, (gy - self.origin_y) * self.scale)
    }

    pub fn to_screen(&self, ix: f64, iy: f64) -> (f64, f64) {
        (self.origin_x + ix / self.scale, self.origin_y + iy / self.scale)
    }

    pub fn len_to_image(&self, v: f64) -> f64 {
        v * self.scale
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn view_round_trips() {
        let b = Rect { x: -1440.0, y: 200.0, w: 1440.0, h: 932.0 };
        let v = View::for_display(2, b, 2.0, 1568.0);
        assert_eq!(v.img_w, 1568);
        let (ix, iy) = v.to_image(-720.0, 666.0);
        let (gx, gy) = v.to_screen(ix, iy);
        assert!((gx + 720.0).abs() < 1e-9 && (gy - 666.0).abs() < 1e-9);
    }

    #[test]
    fn view_never_upscales_past_real_pixels() {
        let b = Rect { x: 0.0, y: 0.0, w: 1280.0, h: 720.0 };
        assert_eq!(View::for_display(1, b, 1.0, 1568.0).img_w, 1280);
        assert_eq!(View::for_display(1, b, 2.0, 1568.0).img_w, 1568);
    }

    #[test]
    fn view_shrinks_large_displays() {
        let b = Rect { x: 0.0, y: 0.0, w: 3840.0, h: 2160.0 };
        let v = View::for_display(1, b, 1.0, 1568.0);
        assert_eq!((v.img_w, v.img_h), (1568, 882));
    }

    #[test]
    fn rect_tests() {
        let a = Rect { x: 0.0, y: 0.0, w: 10.0, h: 10.0 };
        assert!(a.contains(9.9, 0.0) && !a.contains(10.0, 0.0));
        assert!(a.intersects(&Rect { x: 9.0, y: 9.0, w: 5.0, h: 5.0 }));
        assert!(!a.intersects(&Rect { x: 10.0, y: 0.0, w: 5.0, h: 5.0 }));
        assert!(a.contains_rect(&Rect { x: 2.0, y: 2.0, w: 8.5, h: 3.0 }));
    }
}
