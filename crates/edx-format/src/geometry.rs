//! Placement geometry (specification, sections 2.3 and 10): where a package's pads, courtyards and
//! owned regions land on the board, and back.

use crate::model::{Pad, PadLayers, PadShape, Point, Side};
use crate::shape::{self, Segment, Subpath};

/// Where a package stands on the board: its origin's position, its angle and its side.
///
/// A part on the `bottom` side is its package mirrored left to right (x becomes -x), then turned by
/// its angle, counter-clockwise as seen from the top (section 2.3).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    pub at: Point,
    pub angle: f64,
    pub side: Side,
}

impl Transform {
    pub fn new(at: Point, angle: f64, side: Side) -> Self {
        Transform { at, angle, side }
    }

    fn mirrored(&self) -> bool {
        self.side == Side::Bottom
    }

    /// A point in the package's coordinates, on the board.
    pub fn apply(&self, p: Point) -> Point {
        let x = if self.mirrored() { -p[0] } else { p[0] };
        let (s, c) = sin_cos(self.angle);
        // Counter-clockwise as seen from the top, with y downwards.
        [self.at[0] + x * c + p[1] * s, self.at[1] - x * s + p[1] * c]
    }

    /// A point on the board, in the package's coordinates: the inverse of [`Transform::apply`].
    pub fn invert(&self, p: Point) -> Point {
        let (dx, dy) = (p[0] - self.at[0], p[1] - self.at[1]);
        let (s, c) = sin_cos(self.angle);
        let x = dx * c - dy * s;
        [if self.mirrored() { -x } else { x }, dx * s + dy * c]
    }

    /// A rotation in the package (a pad's), on the board, normalised to [0, 360).
    pub fn rotation(&self, r: f64) -> f64 {
        normalize_angle(if self.mirrored() { self.angle - r } else { self.angle + r })
    }

    /// A rotation on the board, in the package: the inverse of [`Transform::rotation`].
    pub fn invert_rotation(&self, r: f64) -> f64 {
        normalize_angle(if self.mirrored() { self.angle - r } else { r - self.angle })
    }

    /// The copper sides of a pad's layers on the board: `top` and `bottom` swap for a part on the bottom.
    pub fn layers(&self, layers: PadLayers) -> PadLayers {
        match (self.side, layers) {
            (Side::Bottom, PadLayers::Top) => PadLayers::Bottom,
            (Side::Bottom, PadLayers::Bottom) => PadLayers::Top,
            (_, l) => l,
        }
    }

    /// Subpaths in the package's coordinates, on the board. Mirroring reverses each arc's sweep.
    pub fn apply_subpaths(&self, subpaths: &[Subpath]) -> Vec<Subpath> {
        map_subpaths(subpaths, |p| self.apply(p), self.mirrored())
    }

    /// Subpaths on the board, in the package's coordinates.
    pub fn invert_subpaths(&self, subpaths: &[Subpath]) -> Vec<Subpath> {
        map_subpaths(subpaths, |p| self.invert(p), self.mirrored())
    }

    /// A shape in the package's coordinates, on the board.
    pub fn apply_shape(&self, d: &str) -> Result<String, String> {
        Ok(shape::write(&self.apply_subpaths(&shape::parse(d)?)))
    }

    /// A shape on the board, in the package's coordinates.
    pub fn invert_shape(&self, d: &str) -> Result<String, String> {
        Ok(shape::write(&self.invert_subpaths(&shape::parse(d)?)))
    }
}

/// Sine and cosine of an angle in degrees, exact at multiples of 90 degrees.
fn sin_cos(degrees: f64) -> (f64, f64) {
    let a = normalize_angle(degrees);
    match a {
        0.0 => (0.0, 1.0),
        90.0 => (1.0, 0.0),
        180.0 => (0.0, -1.0),
        270.0 => (-1.0, 0.0),
        _ => a.to_radians().sin_cos(),
    }
}

/// An angle normalised to [0, 360), compared to 0.000001 degrees (section 2.1).
pub fn normalize_angle(a: f64) -> f64 {
    let a = (a.rem_euclid(360.0) * 1e6).round() / 1e6;
    if a >= 360.0 {
        0.0
    } else {
        a + 0.0
    }
}

fn map_subpaths(subpaths: &[Subpath], f: impl Fn(Point) -> Point, mirrored: bool) -> Vec<Subpath> {
    subpaths
        .iter()
        .map(|sub| Subpath {
            start: f(sub.start),
            segments: sub
                .segments
                .iter()
                .map(|seg| match *seg {
                    Segment::Line { to } => Segment::Line { to: f(to) },
                    Segment::Arc { rx, ry, rotation, large, sweep, to } => {
                        Segment::Arc { rx, ry, rotation, large, sweep: sweep != mirrored, to: f(to) }
                    }
                })
                .collect(),
        })
        .collect()
}

/// A pad's copper as subpaths in its package's coordinates: its shape (section 5.2) turned by its
/// rotation and moved to its position. Arcs are kept, so a circle is two half circles.
pub fn pad_shape(pad: &Pad) -> Result<Vec<Subpath>, String> {
    let size = || pad.size.ok_or_else(|| format!("pad {} has no size", pad.number));
    let local = match pad.shape {
        PadShape::Rect => rounded_rect(size()?, 0.0),
        PadShape::Roundrect => {
            let r = pad.corner_radius.ok_or_else(|| format!("pad {} has no corner radius", pad.number))?;
            rounded_rect(size()?, r)
        }
        PadShape::Circle | PadShape::Oval => {
            let [w, h] = size()?;
            rounded_rect([w, h], w.min(h) / 2.0)
        }
        PadShape::Polygon => {
            shape::parse(pad.outline.as_deref().ok_or_else(|| format!("pad {} has no outline", pad.number))?)?
        }
    };
    Ok(Transform::new(pad.position, pad.rotation, Side::Top).apply_subpaths(&local))
}

/// A rectangle centred on the origin with corners rounded to `r` (clamped to half its smaller side):
/// a rectangle when `r` is 0, a stadium or circle when it is half the smaller side.
fn rounded_rect([w, h]: [f64; 2], r: f64) -> Vec<Subpath> {
    let (x, y) = (w / 2.0, h / 2.0);
    let r = r.clamp(0.0, x.min(y));
    if r == 0.0 {
        return vec![shape::ring(&[[-x, -y], [x, -y], [x, y], [-x, y]])];
    }
    // Clockwise as seen on screen, y downwards: sweep 1. Each corner is a quarter turn; a side of zero
    // length (a stadium's end, every side of a circle) is left out.
    let corners = [
        ([x - r, -y], [x, -y + r]),
        ([x, y - r], [x - r, y]),
        ([-x + r, y], [-x, y - r]),
        ([-x, -y + r], [-x + r, -y]),
    ];
    let start = [-x + r, -y];
    let mut segments = Vec::new();
    let mut current = start;
    for (from, to) in corners {
        if from != current {
            segments.push(Segment::Line { to: from });
        }
        segments.push(Segment::Arc { rx: r, ry: r, rotation: 0.0, large: false, sweep: true, to });
        current = to;
    }
    vec![Subpath { start, segments }]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{PadKind, PadLayers};
    use geo::Area;

    fn close(a: Point, b: Point) -> bool {
        (a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9
    }

    #[test]
    fn turns_counter_clockwise_as_seen_from_the_top() {
        // y is downwards, so a quarter turn counter-clockwise takes +x to -y.
        let t = Transform::new([10.0, 20.0], 90.0, Side::Top);
        assert!(close(t.apply([1.0, 0.0]), [10.0, 19.0]));
        assert!(close(t.invert([10.0, 19.0]), [1.0, 0.0]));
    }

    #[test]
    fn mirrors_the_bottom_side_before_turning() {
        let t = Transform::new([0.0, 0.0], 0.0, Side::Bottom);
        assert!(close(t.apply([1.0, 2.0]), [-1.0, 2.0]));
        let t = Transform::new([5.0, 5.0], 30.0, Side::Bottom);
        for p in [[1.0, 2.0], [-3.5, 0.25]] {
            assert!(close(t.invert(t.apply(p)), p));
        }
        assert_eq!(t.rotation(10.0), 20.0);
        assert_eq!(t.invert_rotation(t.rotation(10.0)), 10.0);
        assert_eq!(t.layers(PadLayers::Top), PadLayers::Bottom);
        assert_eq!(t.layers(PadLayers::Through), PadLayers::Through);
    }

    #[test]
    fn mirrored_shapes_stay_valid() {
        let d = "M 1 0 A 1 1 0 0 0 0 1 V 19 A 1 1 0 0 0 1 20 H 29 A 1 1 0 0 0 30 19 V 1 A 1 1 0 0 0 29 0 Z";
        let t = Transform::new([3.0, 4.0], 45.0, Side::Bottom);
        let moved = t.apply_shape(d).unwrap();
        assert!(shape::check(&moved).is_empty(), "{moved}");
        let area = |d: &str| shape::to_polygon(&shape::parse(d).unwrap()).unsigned_area();
        assert!((area(&moved) - area(d)).abs() < 1e-3);
        assert_eq!(
            crate::canonical::canonical_shape(&t.invert_shape(&moved).unwrap()),
            crate::canonical::canonical_shape(d)
        );
    }

    fn pad(shape: PadShape, size: [f64; 2]) -> Pad {
        Pad {
            number: "1".into(),
            kind: PadKind::Smd,
            shape,
            size: Some(size),
            corner_radius: Some(0.1),
            outline: None,
            position: [1.0, 2.0],
            rotation: 90.0,
            layers: PadLayers::Top,
            drill: None,
            zone_connection: None,
            ext: Default::default(),
        }
    }

    #[test]
    fn pad_shapes() {
        let area = |p: &Pad| shape::to_polygon(&pad_shape(p).unwrap()).unsigned_area();
        let pi = std::f64::consts::PI;
        assert!((area(&pad(PadShape::Rect, [2.0, 1.0])) - 2.0).abs() < 1e-9);
        assert!((area(&pad(PadShape::Circle, [1.0, 1.0])) - pi / 4.0).abs() < 1e-3);
        assert!((area(&pad(PadShape::Oval, [3.0, 1.0])) - (2.0 + pi / 4.0)).abs() < 1e-3);
        assert!((area(&pad(PadShape::Roundrect, [2.0, 1.0])) - (2.0 - 0.04 + pi * 0.01)).abs() < 1e-3);
        for s in [PadShape::Rect, PadShape::Circle, PadShape::Oval, PadShape::Roundrect] {
            let d = shape::write(&pad_shape(&pad(s, [3.0, 1.0])).unwrap());
            assert!(shape::check(&d).is_empty(), "{s:?}: {d}");
        }
        // Turned a quarter: 2 wide by 1 high becomes 1 wide by 2 high, about the pad's position.
        let ring = shape::flatten(&pad_shape(&pad(PadShape::Rect, [2.0, 1.0])).unwrap()[0]);
        let xs: Vec<f64> = ring.iter().map(|p| p[0]).collect();
        let ys: Vec<f64> = ring.iter().map(|p| p[1]).collect();
        let span = |v: &[f64]| v.iter().cloned().fold(f64::MIN, f64::max) - v.iter().cloned().fold(f64::MAX, f64::min);
        assert!((span(&xs) - 1.0).abs() < 1e-9 && (span(&ys) - 2.0).abs() < 1e-9);
    }
}
