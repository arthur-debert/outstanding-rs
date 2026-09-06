//! Theme-relative colorspace for perceptually uniform palette generation.
//!
//! Terminal 256-color palettes have 240 extended colors (indices 16-255) that
//! are normally hardcoded, ignoring the user's base16 theme. This module
//! generates those colors instead, per [jake-stewart's proposal][gist], by
//! trilinear interpolation in CIE LAB space using the 8 base ANSI colors as the
//! corners of a cube.
//!
//! [gist]: https://gist.github.com/jake-stewart/0a8ea46159a7da2c808e5be2177e1783
//!
//! A [`CubeCoord`] addresses a color as a fractional position in that cube
//! rather than as an absolute RGB value, so the same coordinate renders
//! differently under different themes. Interpolation happens in LAB rather than
//! RGB because LAB is perceptually uniform: equal numeric distance means equal
//! perceived difference, so blends stay smooth and brightness stays consistent
//! across hues.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

#[derive(Debug, Clone, Copy)]
struct Lab {
    l: f64,
    a: f64,
    b: f64,
}

const XN: f64 = 0.95047;
const YN: f64 = 1.00000;
const ZN: f64 = 1.08883;

fn srgb_to_linear(c: u8) -> f64 {
    let c = c as f64 / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(c: f64) -> u8 {
    let c = c.clamp(0.0, 1.0);
    let s = if c <= 0.0031308 {
        12.92 * c
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    };
    (s * 255.0).round() as u8
}

fn lab_f(t: f64) -> f64 {
    if t > 0.008856 {
        t.cbrt()
    } else {
        7.787 * t + 16.0 / 116.0
    }
}

fn lab_f_inv(t: f64) -> f64 {
    if t > 0.206896 {
        t * t * t
    } else {
        (t - 16.0 / 116.0) / 7.787
    }
}

fn rgb_to_lab(rgb: Rgb) -> Lab {
    let r = srgb_to_linear(rgb.0);
    let g = srgb_to_linear(rgb.1);
    let b = srgb_to_linear(rgb.2);

    let x = 0.4124564 * r + 0.3575761 * g + 0.1804375 * b;
    let y = 0.2126729 * r + 0.7151522 * g + 0.0721750 * b;
    let z = 0.0193339 * r + 0.1191920 * g + 0.9503041 * b;

    let fx = lab_f(x / XN);
    let fy = lab_f(y / YN);
    let fz = lab_f(z / ZN);

    Lab {
        l: 116.0 * fy - 16.0,
        a: 500.0 * (fx - fy),
        b: 200.0 * (fy - fz),
    }
}

fn lab_to_rgb(lab: Lab) -> Rgb {
    let fy = (lab.l + 16.0) / 116.0;
    let fx = lab.a / 500.0 + fy;
    let fz = fy - lab.b / 200.0;

    let x = XN * lab_f_inv(fx);
    let y = YN * lab_f_inv(fy);
    let z = ZN * lab_f_inv(fz);

    let r = 3.2404542 * x - 1.5371385 * y - 0.4985314 * z;
    let g = -0.9692660 * x + 1.8760108 * y + 0.0415560 * z;
    let b = 0.0556434 * x - 0.2040259 * y + 1.0572252 * z;

    Rgb(linear_to_srgb(r), linear_to_srgb(g), linear_to_srgb(b))
}

fn lerp_lab(t: f64, a: &Lab, b: &Lab) -> Lab {
    Lab {
        l: a.l + t * (b.l - a.l),
        a: a.a + t * (b.a - a.a),
        b: a.b + t * (b.b - a.b),
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CubeCoord {
    pub r: f64,
    pub g: f64,
    pub b: f64,
}

// `PartialEq`-derived `Eq` requires values to never be NaN; `new`/`from_percentages`
// reject anything outside 0.0..=1.0, which guarantees that.
impl Eq for CubeCoord {}

impl CubeCoord {
    pub fn new(r: f64, g: f64, b: f64) -> Result<Self, String> {
        if !(0.0..=1.0).contains(&r) || !(0.0..=1.0).contains(&g) || !(0.0..=1.0).contains(&b) {
            return Err(format!(
                "CubeCoord components must be 0.0..=1.0, got ({}, {}, {})",
                r, g, b
            ));
        }
        Ok(Self { r, g, b })
    }

    pub fn from_percentages(r: f64, g: f64, b: f64) -> Result<Self, String> {
        Self::new(r / 100.0, g / 100.0, b / 100.0)
    }

    pub fn quantize(&self, levels: u8) -> (u8, u8, u8) {
        let max = (levels - 1) as f64;
        let r = (self.r * max).round() as u8;
        let g = (self.g * max).round() as u8;
        let b = (self.b * max).round() as u8;
        (r.min(levels - 1), g.min(levels - 1), b.min(levels - 1))
    }

    pub fn to_palette_index(&self, levels: u8) -> u8 {
        let (r, g, b) = self.quantize(levels);
        let levels_sq = levels as u16 * levels as u16;
        (16 + levels_sq * r as u16 + levels as u16 * g as u16 + b as u16) as u8
    }
}

#[derive(Debug, Clone)]
pub struct ThemePalette {
    anchors: [Rgb; 8],
    bg: Rgb,
    fg: Rgb,
}

impl ThemePalette {
    pub fn default_xterm() -> Self {
        Self::new([
            Rgb(0, 0, 0),
            Rgb(205, 0, 0),
            Rgb(0, 205, 0),
            Rgb(205, 205, 0),
            Rgb(0, 0, 238),
            Rgb(205, 0, 205),
            Rgb(0, 205, 205),
            Rgb(229, 229, 229),
        ])
    }

    pub fn new(anchors: [Rgb; 8]) -> Self {
        let bg = anchors[0];
        let fg = anchors[7];
        Self { anchors, bg, fg }
    }

    pub fn with_bg(mut self, bg: Rgb) -> Self {
        self.bg = bg;
        self
    }

    pub fn with_fg(mut self, fg: Rgb) -> Self {
        self.fg = fg;
        self
    }

    pub fn resolve(&self, coord: &CubeCoord) -> Rgb {
        let bg_lab = rgb_to_lab(self.bg);
        let fg_lab = rgb_to_lab(self.fg);
        let labs: Vec<Lab> = self.anchors.iter().map(|c| rgb_to_lab(*c)).collect();

        let c0 = lerp_lab(coord.r, &bg_lab, &labs[1]);
        let c1 = lerp_lab(coord.r, &labs[2], &labs[3]);
        let c2 = lerp_lab(coord.r, &labs[4], &labs[5]);
        let c3 = lerp_lab(coord.r, &labs[6], &fg_lab);

        let c4 = lerp_lab(coord.g, &c0, &c1);
        let c5 = lerp_lab(coord.g, &c2, &c3);

        let c6 = lerp_lab(coord.b, &c4, &c5);

        lab_to_rgb(c6)
    }

    pub fn generate_palette(&self, subdivisions: u8) -> Vec<Rgb> {
        let bg_lab = rgb_to_lab(self.bg);
        let fg_lab = rgb_to_lab(self.fg);
        let labs: Vec<Lab> = self.anchors.iter().map(|c| rgb_to_lab(*c)).collect();
        let max = (subdivisions - 1) as f64;

        let mut palette = Vec::new();

        for r in 0..subdivisions {
            let rt = r as f64 / max;
            let c0 = lerp_lab(rt, &bg_lab, &labs[1]);
            let c1 = lerp_lab(rt, &labs[2], &labs[3]);
            let c2 = lerp_lab(rt, &labs[4], &labs[5]);
            let c3 = lerp_lab(rt, &labs[6], &fg_lab);

            for g in 0..subdivisions {
                let gt = g as f64 / max;
                let c4 = lerp_lab(gt, &c0, &c1);
                let c5 = lerp_lab(gt, &c2, &c3);

                for b in 0..subdivisions {
                    let bt = b as f64 / max;
                    let c6 = lerp_lab(bt, &c4, &c5);
                    palette.push(lab_to_rgb(c6));
                }
            }
        }

        for i in 0..24 {
            let t = (i + 1) as f64 / 25.0;
            let lab = lerp_lab(t, &bg_lab, &fg_lab);
            palette.push(lab_to_rgb(lab));
        }

        palette
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_rgb_roundtrip(rgb: Rgb, tolerance: u8) {
        let lab = rgb_to_lab(rgb);
        let back = lab_to_rgb(lab);
        let dr = (rgb.0 as i16 - back.0 as i16).unsigned_abs() as u8;
        let dg = (rgb.1 as i16 - back.1 as i16).unsigned_abs() as u8;
        let db = (rgb.2 as i16 - back.2 as i16).unsigned_abs() as u8;
        assert!(
            dr <= tolerance && dg <= tolerance && db <= tolerance,
            "Round-trip failed: {:?} → {:?} → {:?} (delta: {}, {}, {})",
            rgb,
            lab,
            back,
            dr,
            dg,
            db
        );
    }

    #[test]
    fn roundtrip_black() {
        assert_rgb_roundtrip(Rgb(0, 0, 0), 1);
    }

    #[test]
    fn roundtrip_white() {
        assert_rgb_roundtrip(Rgb(255, 255, 255), 1);
    }

    #[test]
    fn roundtrip_pure_red() {
        assert_rgb_roundtrip(Rgb(255, 0, 0), 1);
    }

    #[test]
    fn roundtrip_pure_green() {
        assert_rgb_roundtrip(Rgb(0, 255, 0), 1);
    }

    #[test]
    fn roundtrip_pure_blue() {
        assert_rgb_roundtrip(Rgb(0, 0, 255), 1);
    }

    #[test]
    fn roundtrip_mid_gray() {
        assert_rgb_roundtrip(Rgb(128, 128, 128), 1);
    }

    #[test]
    fn roundtrip_arbitrary_color() {
        assert_rgb_roundtrip(Rgb(200, 100, 50), 1);
    }

    #[test]
    fn lab_black_is_zero_lightness() {
        let lab = rgb_to_lab(Rgb(0, 0, 0));
        assert!(lab.l.abs() < 1.0, "Black L* should be ~0, got {}", lab.l);
    }

    #[test]
    fn lab_white_is_full_lightness() {
        let lab = rgb_to_lab(Rgb(255, 255, 255));
        assert!(
            (lab.l - 100.0).abs() < 1.0,
            "White L* should be ~100, got {}",
            lab.l
        );
    }

    #[test]
    fn lab_red_has_positive_a() {
        let lab = rgb_to_lab(Rgb(255, 0, 0));
        assert!(
            lab.a > 50.0,
            "Red should have large positive a*, got {}",
            lab.a
        );
    }

    #[test]
    fn lerp_at_zero_returns_first() {
        let a = rgb_to_lab(Rgb(255, 0, 0));
        let b = rgb_to_lab(Rgb(0, 0, 255));
        let result = lerp_lab(0.0, &a, &b);
        assert!((result.l - a.l).abs() < 0.001);
        assert!((result.a - a.a).abs() < 0.001);
        assert!((result.b - a.b).abs() < 0.001);
    }

    #[test]
    fn lerp_at_one_returns_second() {
        let a = rgb_to_lab(Rgb(255, 0, 0));
        let b = rgb_to_lab(Rgb(0, 0, 255));
        let result = lerp_lab(1.0, &a, &b);
        assert!((result.l - b.l).abs() < 0.001);
        assert!((result.a - b.a).abs() < 0.001);
        assert!((result.b - b.b).abs() < 0.001);
    }

    #[test]
    fn lerp_midpoint_is_between() {
        let a = rgb_to_lab(Rgb(0, 0, 0));
        let b = rgb_to_lab(Rgb(255, 255, 255));
        let mid = lerp_lab(0.5, &a, &b);
        assert!(mid.l > a.l && mid.l < b.l);
    }

    #[test]
    fn cubecoord_valid_range() {
        assert!(CubeCoord::new(0.0, 0.0, 0.0).is_ok());
        assert!(CubeCoord::new(1.0, 1.0, 1.0).is_ok());
        assert!(CubeCoord::new(0.5, 0.5, 0.5).is_ok());
    }

    #[test]
    fn cubecoord_rejects_negative() {
        assert!(CubeCoord::new(-0.1, 0.0, 0.0).is_err());
        assert!(CubeCoord::new(0.0, -0.1, 0.0).is_err());
        assert!(CubeCoord::new(0.0, 0.0, -0.1).is_err());
    }

    #[test]
    fn cubecoord_rejects_over_one() {
        assert!(CubeCoord::new(1.1, 0.0, 0.0).is_err());
        assert!(CubeCoord::new(0.0, 1.1, 0.0).is_err());
        assert!(CubeCoord::new(0.0, 0.0, 1.1).is_err());
    }

    #[test]
    fn cubecoord_from_percentages() {
        let coord = CubeCoord::from_percentages(60.0, 20.0, 0.0).unwrap();
        assert!((coord.r - 0.6).abs() < 0.001);
        assert!((coord.g - 0.2).abs() < 0.001);
        assert!((coord.b - 0.0).abs() < 0.001);
    }

    #[test]
    fn cubecoord_from_percentages_bounds() {
        assert!(CubeCoord::from_percentages(0.0, 0.0, 0.0).is_ok());
        assert!(CubeCoord::from_percentages(100.0, 100.0, 100.0).is_ok());
        assert!(CubeCoord::from_percentages(101.0, 0.0, 0.0).is_err());
        assert!(CubeCoord::from_percentages(-1.0, 0.0, 0.0).is_err());
    }

    #[test]
    fn quantize_corners_levels_6() {
        assert_eq!(
            CubeCoord::new(0.0, 0.0, 0.0).unwrap().quantize(6),
            (0, 0, 0)
        );
        assert_eq!(
            CubeCoord::new(1.0, 1.0, 1.0).unwrap().quantize(6),
            (5, 5, 5)
        );
        assert_eq!(
            CubeCoord::new(1.0, 0.0, 0.0).unwrap().quantize(6),
            (5, 0, 0)
        );
        assert_eq!(
            CubeCoord::new(0.0, 1.0, 0.0).unwrap().quantize(6),
            (0, 5, 0)
        );
        assert_eq!(
            CubeCoord::new(0.0, 0.0, 1.0).unwrap().quantize(6),
            (0, 0, 5)
        );
    }

    #[test]
    fn quantize_midpoint_levels_6() {
        let (r, g, b) = CubeCoord::new(0.5, 0.5, 0.5).unwrap().quantize(6);
        assert_eq!((r, g, b), (3, 3, 3));
    }

    #[test]
    fn quantize_one_fifth_levels_6() {
        let (r, _, _) = CubeCoord::new(0.2, 0.0, 0.0).unwrap().quantize(6);
        assert_eq!(r, 1);
    }

    #[test]
    fn palette_index_origin() {
        assert_eq!(
            CubeCoord::new(0.0, 0.0, 0.0).unwrap().to_palette_index(6),
            16
        );
    }

    #[test]
    fn palette_index_max() {
        assert_eq!(
            CubeCoord::new(1.0, 1.0, 1.0).unwrap().to_palette_index(6),
            231
        );
    }

    #[test]
    fn palette_index_pure_red() {
        assert_eq!(
            CubeCoord::new(1.0, 0.0, 0.0).unwrap().to_palette_index(6),
            196
        );
    }

    #[test]
    fn palette_index_pure_blue() {
        assert_eq!(
            CubeCoord::new(0.0, 0.0, 1.0).unwrap().to_palette_index(6),
            21
        );
    }

    #[test]
    fn palette_index_pure_green() {
        assert_eq!(
            CubeCoord::new(0.0, 1.0, 0.0).unwrap().to_palette_index(6),
            46
        );
    }

    fn test_palette() -> ThemePalette {
        ThemePalette::new([
            Rgb(0, 0, 0),
            Rgb(205, 0, 0),
            Rgb(0, 205, 0),
            Rgb(205, 205, 0),
            Rgb(0, 0, 238),
            Rgb(205, 0, 205),
            Rgb(0, 205, 205),
            Rgb(229, 229, 229),
        ])
    }

    #[test]
    fn resolve_corner_bg() {
        let palette = test_palette();
        let coord = CubeCoord::new(0.0, 0.0, 0.0).unwrap();
        let rgb = palette.resolve(&coord);
        assert_eq!(rgb, Rgb(0, 0, 0));
    }

    #[test]
    fn resolve_corner_red() {
        let palette = test_palette();
        let coord = CubeCoord::new(1.0, 0.0, 0.0).unwrap();
        let rgb = palette.resolve(&coord);
        assert_eq!(rgb, Rgb(205, 0, 0));
    }

    #[test]
    fn resolve_corner_green() {
        let palette = test_palette();
        let coord = CubeCoord::new(0.0, 1.0, 0.0).unwrap();
        let rgb = palette.resolve(&coord);
        assert_eq!(rgb, Rgb(0, 205, 0));
    }

    #[test]
    fn resolve_corner_yellow() {
        let palette = test_palette();
        let coord = CubeCoord::new(1.0, 1.0, 0.0).unwrap();
        let rgb = palette.resolve(&coord);
        assert_eq!(rgb, Rgb(205, 205, 0));
    }

    #[test]
    fn resolve_corner_blue() {
        let palette = test_palette();
        let coord = CubeCoord::new(0.0, 0.0, 1.0).unwrap();
        let rgb = palette.resolve(&coord);
        assert_eq!(rgb, Rgb(0, 0, 238));
    }

    #[test]
    fn resolve_corner_magenta() {
        let palette = test_palette();
        let coord = CubeCoord::new(1.0, 0.0, 1.0).unwrap();
        let rgb = palette.resolve(&coord);
        assert_eq!(rgb, Rgb(205, 0, 205));
    }

    #[test]
    fn resolve_corner_cyan() {
        let palette = test_palette();
        let coord = CubeCoord::new(0.0, 1.0, 1.0).unwrap();
        let rgb = palette.resolve(&coord);
        assert_eq!(rgb, Rgb(0, 205, 205));
    }

    #[test]
    fn resolve_corner_fg() {
        let palette = test_palette();
        let coord = CubeCoord::new(1.0, 1.0, 1.0).unwrap();
        let rgb = palette.resolve(&coord);
        assert_eq!(rgb, Rgb(229, 229, 229));
    }

    #[test]
    fn resolve_center_is_blend() {
        let palette = test_palette();
        let coord = CubeCoord::new(0.5, 0.5, 0.5).unwrap();
        let rgb = palette.resolve(&coord);
        assert_ne!(rgb, Rgb(0, 0, 0));
        assert_ne!(rgb, Rgb(255, 255, 255));
        assert!(rgb.0 > 50 && rgb.0 < 200);
        assert!(rgb.1 > 50 && rgb.1 < 200);
        assert!(rgb.2 > 50 && rgb.2 < 200);
    }

    #[test]
    fn resolve_with_custom_bg_fg() {
        let palette = test_palette()
            .with_bg(Rgb(30, 30, 46))
            .with_fg(Rgb(205, 214, 244));

        let origin = palette.resolve(&CubeCoord::new(0.0, 0.0, 0.0).unwrap());
        assert_eq!(origin, Rgb(30, 30, 46));

        let corner = palette.resolve(&CubeCoord::new(1.0, 1.0, 1.0).unwrap());
        assert_eq!(corner, Rgb(205, 214, 244));
    }

    #[test]
    fn generate_palette_correct_count() {
        let palette = test_palette();
        let extended = palette.generate_palette(6);
        assert_eq!(extended.len(), 240);
    }

    #[test]
    fn generate_palette_first_entry_is_bg() {
        let palette = test_palette();
        let extended = palette.generate_palette(6);
        assert_eq!(extended[0], Rgb(0, 0, 0));
    }

    #[test]
    fn generate_palette_last_cube_entry_is_fg() {
        let palette = test_palette();
        let extended = palette.generate_palette(6);
        assert_eq!(extended[215], Rgb(229, 229, 229));
    }

    #[test]
    fn generate_palette_red_corner() {
        let palette = test_palette();
        let extended = palette.generate_palette(6);
        assert_eq!(extended[180], Rgb(205, 0, 0));
    }

    #[test]
    fn generate_palette_grayscale_monotonic_lightness() {
        let palette = test_palette();
        let extended = palette.generate_palette(6);
        let grayscale = &extended[216..240];

        for i in 1..grayscale.len() {
            let prev_l = rgb_to_lab(grayscale[i - 1]).l;
            let curr_l = rgb_to_lab(grayscale[i]).l;
            assert!(
                curr_l >= prev_l - 0.01,
                "Grayscale lightness not monotonic at index {}: {} < {}",
                i,
                curr_l,
                prev_l
            );
        }
    }

    #[test]
    fn generate_palette_different_subdivisions() {
        let palette = test_palette();
        let small = palette.generate_palette(4);
        assert_eq!(small.len(), 88);
        let large = palette.generate_palette(8);
        assert_eq!(large.len(), 536);
    }

    #[test]
    fn generate_palette_with_gruvbox() {
        let palette = ThemePalette::new([
            Rgb(40, 40, 40),
            Rgb(204, 36, 29),
            Rgb(152, 151, 26),
            Rgb(215, 153, 33),
            Rgb(69, 133, 136),
            Rgb(177, 98, 134),
            Rgb(104, 157, 106),
            Rgb(168, 153, 132),
        ])
        .with_bg(Rgb(40, 40, 40))
        .with_fg(Rgb(235, 219, 178));

        let extended = palette.generate_palette(6);
        assert_eq!(extended.len(), 240);

        assert_eq!(extended[0], Rgb(40, 40, 40));
        assert_eq!(extended[215], Rgb(235, 219, 178));
    }
}
