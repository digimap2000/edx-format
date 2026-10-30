//! Shapes: the EDX subset of SVG path data (specification, section 2.5).

use std::f64::consts::TAU;

use geo::{Area, BooleanOps, Coord, LineString, MultiPolygon, Polygon, Validation};

/// Chord error allowed when arcs are replaced by segments for checks, in millimetres.
pub const FLATTEN_TOLERANCE: f64 = 0.0001;

#[derive(Clone, Debug, PartialEq)]
pub enum Segment {
    Line { to: [f64; 2] },
    Arc { rx: f64, ry: f64, rotation: f64, large: bool, sweep: bool, to: [f64; 2] },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Subpath {
    pub start: [f64; 2],
    pub segments: Vec<Segment>,
}

/// Parses a shape into its subpaths. The first is the outer boundary, the rest are holes.
pub fn parse(d: &str) -> Result<Vec<Subpath>, String> {
    let mut tokens = Tokens { s: d.as_bytes(), i: 0 };
    let mut subpaths = Vec::new();
    while let Some(c) = tokens.command()? {
        if c != b'M' {
            return Err(format!("a subpath must start with M, found {}", c as char));
        }
        let start = [tokens.number()?, tokens.number()?];
        let mut current = start;
        let mut segments = Vec::new();
        loop {
            let c = tokens.command()?.ok_or("a subpath must end with Z")?;
            let segment = match c {
                b'L' => Segment::Line { to: [tokens.number()?, tokens.number()?] },
                b'H' => Segment::Line { to: [tokens.number()?, current[1]] },
                b'V' => Segment::Line { to: [current[0], tokens.number()?] },
                b'A' => Segment::Arc {
                    rx: tokens.number()?,
                    ry: tokens.number()?,
                    rotation: tokens.number()?,
                    large: tokens.flag()?,
                    sweep: tokens.flag()?,
                    to: [tokens.number()?, tokens.number()?],
                },
                b'Z' => break,
                c => return Err(format!("command {} is not in the EDX subset", c as char)),
            };
            current = match &segment {
                Segment::Line { to } | Segment::Arc { to, .. } => *to,
            };
            segments.push(segment);
        }
        if segments.is_empty() {
            return Err("a subpath has no segments".into());
        }
        subpaths.push(Subpath { start, segments });
    }
    if subpaths.is_empty() {
        return Err("a shape needs at least one subpath".into());
    }
    Ok(subpaths)
}

struct Tokens<'a> {
    s: &'a [u8],
    i: usize,
}

impl Tokens<'_> {
    fn skip_separators(&mut self) {
        while self.i < self.s.len() && (self.s[self.i].is_ascii_whitespace() || self.s[self.i] == b',') {
            self.i += 1;
        }
    }

    fn command(&mut self) -> Result<Option<u8>, String> {
        self.skip_separators();
        match self.s.get(self.i) {
            None => Ok(None),
            Some(c) if c.is_ascii_alphabetic() => {
                self.i += 1;
                Ok(Some(*c))
            }
            Some(c) => Err(format!("expected a command, found {}", *c as char)),
        }
    }

    fn number(&mut self) -> Result<f64, String> {
        self.skip_separators();
        let start = self.i;
        if self.s.get(self.i) == Some(&b'-') {
            self.i += 1;
        }
        while self.i < self.s.len() && (self.s[self.i].is_ascii_digit() || self.s[self.i] == b'.') {
            self.i += 1;
        }
        let text = std::str::from_utf8(&self.s[start..self.i]).unwrap_or("");
        text.parse().map_err(|_| format!("expected a number at offset {start}"))
    }

    fn flag(&mut self) -> Result<bool, String> {
        let f = self.number()?;
        if f == 0.0 || f == 1.0 {
            Ok(f == 1.0)
        } else {
            Err(format!("an arc flag must be 0 or 1, found {f}"))
        }
    }
}

/// Rounds a length to the nearest nanometre, ties away from zero (section 2.1).
pub fn round_nm(v: f64) -> f64 {
    (v * 1e6).round() / 1e6
}

/// Replaces a subpath's arcs by segments, giving its corner points (not repeating the first).
pub fn flatten(sub: &Subpath) -> Vec<[f64; 2]> {
    let mut points = vec![sub.start];
    let mut current = sub.start;
    for segment in &sub.segments {
        match *segment {
            Segment::Line { to } => points.push(to),
            Segment::Arc { rx, large, sweep, to, .. } => arc_points(current, to, rx.abs(), large, sweep, &mut points),
        }
        current = match segment {
            Segment::Line { to } | Segment::Arc { to, .. } => *to,
        };
    }
    if points.len() > 1 && points.first() == points.last() {
        points.pop();
    }
    points
}

/// Appends points along a circular arc from `p0` to `p1` (SVG 1.1, appendix F.6.5), ending at `p1`.
fn arc_points(p0: [f64; 2], p1: [f64; 2], r: f64, large: bool, sweep: bool, out: &mut Vec<[f64; 2]>) {
    let (hx, hy) = ((p0[0] - p1[0]) / 2.0, (p0[1] - p1[1]) / 2.0);
    if r == 0.0 || (hx == 0.0 && hy == 0.0) {
        out.push(p1);
        return;
    }
    let d2 = hx * hx + hy * hy;
    let r = r.max(d2.sqrt());
    let sign = if large == sweep { -1.0 } else { 1.0 };
    let coef = sign * ((r * r - d2) / d2).max(0.0).sqrt();
    let (ccx, ccy) = (coef * hy, -coef * hx);
    let (cx, cy) = (ccx + (p0[0] + p1[0]) / 2.0, ccy + (p0[1] + p1[1]) / 2.0);
    let theta = |ux: f64, uy: f64| uy.atan2(ux);
    let t1 = theta((hx - ccx) / r, (hy - ccy) / r);
    let mut dt = theta((-hx - ccx) / r, (-hy - ccy) / r) - t1;
    if sweep && dt < 0.0 {
        dt += TAU;
    } else if !sweep && dt > 0.0 {
        dt -= TAU;
    }
    let step = 2.0 * (1.0 - FLATTEN_TOLERANCE / r).clamp(-1.0, 1.0).acos();
    let n = if step > 0.0 { (dt.abs() / step).ceil().max(1.0) as usize } else { 1 };
    for k in 1..n {
        let t = t1 + dt * k as f64 / n as f64;
        out.push([cx + r * t.cos(), cy + r * t.sin()]);
    }
    out.push(p1);
}

/// Twice the signed area of a ring by the shoelace formula. Seen from the top, with y downwards,
/// a counter-clockwise ring has a negative sum.
pub fn signed_area(points: &[[f64; 2]]) -> f64 {
    let n = points.len();
    (0..n)
        .map(|i| {
            let (a, b) = (points[i], points[(i + 1) % n]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum()
}

/// A shape as a polygon, with arcs flattened.
pub fn to_polygon(subpaths: &[Subpath]) -> Polygon<f64> {
    let ring = |s: &Subpath| LineString::from(flatten(s).into_iter().map(|[x, y]| Coord { x, y }).collect::<Vec<_>>());
    Polygon::new(ring(&subpaths[0]), subpaths[1..].iter().map(ring).collect())
}

/// A problem with one shape, as a code and a message.
pub type Problem = (&'static str, String);

/// Checks a shape's geometry: circular arcs, simple subpaths enclosing area, holes inside (E3001 to E3003).
pub fn check(d: &str) -> Vec<Problem> {
    let subpaths = match parse(d) {
        Ok(s) => s,
        // The schema already rejects these; reached only by a document checked without it.
        Err(e) => return vec![("EDX-E1000", e)],
    };
    let mut problems = Vec::new();
    for (i, sub) in subpaths.iter().enumerate() {
        for segment in &sub.segments {
            if let Segment::Arc { rx, ry, .. } = *segment {
                if round_nm(rx) != round_nm(ry) || round_nm(rx) <= 0.0 {
                    problems.push((
                        "EDX-E3001",
                        format!("subpath {} has an arc with radii {rx} and {ry}; arcs must be circular", i + 1),
                    ));
                }
            }
        }
    }
    if !problems.is_empty() {
        return problems;
    }
    for error in to_polygon(&subpaths).validation_errors() {
        use geo::algorithm::validation::InvalidPolygon as P;
        let (code, message) = match error {
            P::SelfIntersection(_) => ("EDX-E3002", "a subpath crosses itself".to_owned()),
            P::TooFewPointsInRing(_) | P::NonFiniteCoord(..) => ("EDX-E3002", error.to_string()),
            P::InteriorRingNotContainedInExteriorRing(_) => {
                ("EDX-E3003", "a hole is not inside the outer boundary".to_owned())
            }
            P::IntersectingRingsOnALine(..) | P::IntersectingRingsOnAnArea(..) => {
                ("EDX-E3003", "holes overlap each other or the outer boundary".to_owned())
            }
        };
        if !problems.iter().any(|(c, m)| *c == code && *m == message) {
            problems.push((code, message));
        }
    }
    if !problems.iter().any(|(c, _)| *c == "EDX-E3002") {
        for (i, sub) in subpaths.iter().enumerate() {
            let ring = Polygon::new(to_polygon(std::slice::from_ref(sub)).exterior().clone(), vec![]);
            if ring.unsigned_area() < 1e-12 {
                problems.push(("EDX-E3002", format!("subpath {} encloses no area", i + 1)));
            }
        }
    }
    problems
}

/// The union of several shapes. Shapes that fail to parse are left out.
pub fn union(shapes: &[String]) -> MultiPolygon<f64> {
    let mut acc = MultiPolygon::new(vec![]);
    for d in shapes {
        if let Ok(subpaths) = parse(d) {
            acc = acc.union(&MultiPolygon::new(vec![to_polygon(&subpaths)]));
        }
    }
    acc
}

/// Whether two areas overlap by more than a square nanometre.
pub fn overlap(a: &MultiPolygon<f64>, b: &MultiPolygon<f64>) -> bool {
    a.intersection(b).unsigned_area() > 1e-12
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_subset() {
        let s = parse("M 0 0 V 10 H 10 V 0 Z M 2,2 L 4,2 L 4,4 Z").unwrap();
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].segments.len(), 3);
        assert_eq!(s[0].segments[1], Segment::Line { to: [10.0, 10.0] });
    }

    #[test]
    fn rejects_open_subpaths() {
        assert!(parse("M 0 0 H 3 V 3").is_err());
    }

    #[test]
    fn quarter_circle_area() {
        // A quarter disc of radius 10: area 25 pi.
        let s = parse("M 0 0 V 10 A 10 10 0 0 0 10 0 Z").unwrap();
        let area = to_polygon(&s).unsigned_area();
        assert!((area - 25.0 * std::f64::consts::PI).abs() < 0.01, "{area}");
    }

    #[test]
    fn rounded_rectangle_is_valid() {
        let d = "M 1 0 A 1 1 0 0 0 0 1 V 19 A 1 1 0 0 0 1 20 H 29 A 1 1 0 0 0 30 19 V 1 A 1 1 0 0 0 29 0 Z";
        assert!(check(d).is_empty(), "{:?}", check(d));
        let area = to_polygon(&parse(d).unwrap()).unsigned_area();
        let expected = 30.0 * 20.0 - 4.0 + std::f64::consts::PI;
        assert!((area - expected).abs() < 0.01, "{area}");
    }

    #[test]
    fn geometry_problems() {
        assert_eq!(check("M 1 0 A 1 2 0 0 0 0 1 V 10 H 10 V 0 Z")[0].0, "EDX-E3001");
        assert_eq!(check("M 0 0 L 10 10 V 0 L 0 10 Z")[0].0, "EDX-E3002");
        assert_eq!(check("M 0 0 V 10 H 10 V 0 Z M 20 20 H 22 V 22 H 20 Z")[0].0, "EDX-E3003");
        assert_eq!(check("M 0 0 H 10 H 5 Z")[0].0, "EDX-E3002");
    }
}
