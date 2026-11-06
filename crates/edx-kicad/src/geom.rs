//! Drawing KiCad's graphic items as EDX subpaths: lines and three-point arcs chained into closed
//! loops, and loops nested into outer boundaries and holes.

use std::f64::consts::{PI, TAU};

use edx_format::shape::{self, Segment, Subpath};
use geo::{Contains, Coord, Point as GeoPoint};

use crate::sexpr::Node;

pub type P = [f64; 2];

/// Endpoints closer than this are the same point, mm.
pub const JOIN: f64 = 0.001;

/// A piece of an open outline: a line, or a circular arc through three points.
#[derive(Clone, Debug, PartialEq)]
pub enum Seg {
    Line { a: P, b: P },
    Arc { a: P, m: P, b: P },
}

impl Seg {
    pub fn start(&self) -> P {
        match self {
            Seg::Line { a, .. } | Seg::Arc { a, .. } => *a,
        }
    }

    pub fn end(&self) -> P {
        match self {
            Seg::Line { b, .. } | Seg::Arc { b, .. } => *b,
        }
    }

    fn reversed(&self) -> Seg {
        match *self {
            Seg::Line { a, b } => Seg::Line { a: b, b: a },
            Seg::Arc { a, m, b } => Seg::Arc { a: b, m, b: a },
        }
    }

    pub fn map(&self, f: &impl Fn(P) -> P) -> Seg {
        match *self {
            Seg::Line { a, b } => Seg::Line { a: f(a), b: f(b) },
            Seg::Arc { a, m, b } => Seg::Arc { a: f(a), m: f(m), b: f(b) },
        }
    }
}

/// What a KiCad graphic item draws: pieces of an outline still to be joined, or a closed loop.
#[derive(Clone, Debug)]
pub enum Drawn {
    Open(Vec<Seg>),
    Closed(Vec<Seg>),
}

impl Drawn {
    pub fn map(&self, f: &impl Fn(P) -> P) -> Drawn {
        let all = |segs: &[Seg]| segs.iter().map(|s| s.map(f)).collect();
        match self {
            Drawn::Open(segs) => Drawn::Open(all(segs)),
            Drawn::Closed(segs) => Drawn::Closed(all(segs)),
        }
    }
}

fn near(p: P, q: P) -> bool {
    (p[0] - q[0]).hypot(p[1] - q[1]) <= JOIN
}

/// The geometry of a `gr_*` or `fp_*` item, in the coordinates it is written in. Text and other
/// items that draw no outline give `None`.
pub fn drawn(item: &Node) -> Option<Drawn> {
    let kind = item.head()?;
    let kind = kind.strip_prefix("gr_").or_else(|| kind.strip_prefix("fp_"))?;
    match kind {
        "line" => Some(Drawn::Open(vec![Seg::Line { a: item.point("start")?, b: item.point("end")? }])),
        "arc" => arc_item(item).map(|a| Drawn::Open(vec![a])),
        "circle" => {
            let c = item.point("center")?;
            let e = item.point("end")?;
            let r = (e[0] - c[0]).hypot(e[1] - c[1]);
            (r > 0.0).then(|| Drawn::Closed(circle(c, r)))
        }
        "rect" => {
            let (a, b) = (item.point("start")?, item.point("end")?);
            let pts = [a, [b[0], a[1]], b, [a[0], b[1]]];
            Some(Drawn::Closed(closed_lines(&pts)))
        }
        "poly" => {
            let segs = pts(item.child("pts")?);
            (segs.len() >= 2).then_some(Drawn::Closed(segs))
        }
        "curve" => {
            let c: Vec<P> = item.child("pts")?.children("xy").filter_map(|n| Some([n.num(1)?, n.num(2)?])).collect();
            if c.len() != 4 {
                return None;
            }
            let at = |t: f64| {
                let u = 1.0 - t;
                let w = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
                [(0..4).map(|i| w[i] * c[i][0]).sum(), (0..4).map(|i| w[i] * c[i][1]).sum()]
            };
            // A Bézier as a chain of lines, its two ends kept exactly to join its neighbours.
            let points: Vec<P> = (0..=16).map(|k| at(k as f64 / 16.0)).collect();
            Some(Drawn::Open(points.windows(2).map(|w| Seg::Line { a: w[0], b: w[1] }).collect()))
        }
        _ => None,
    }
}

/// An arc item: `(start) (mid) (end)` since KiCad 6, or the older centre, start and angle.
fn arc_item(item: &Node) -> Option<Seg> {
    if let Some(m) = item.point("mid") {
        return Some(Seg::Arc { a: item.point("start")?, m, b: item.point("end")? });
    }
    // Before KiCad 6: `start` is the centre, `end` the arc's first point and `angle` its sweep,
    // clockwise on screen.
    let c = item.point("start")?;
    let a = item.point("end")?;
    let sweep = item.number("angle")?.to_radians();
    let r = (a[0] - c[0]).hypot(a[1] - c[1]);
    let t0 = (a[1] - c[1]).atan2(a[0] - c[0]);
    let at = |t: f64| [c[0] + r * t.cos(), c[1] + r * t.sin()];
    Some(Seg::Arc { a, m: at(t0 + sweep / 2.0), b: at(t0 + sweep) })
}

/// The ring of a `(pts ...)` list: its corner points and any `(arc ...)` members, closed.
pub fn pts(list: &Node) -> Vec<Seg> {
    let mut segs = Vec::new();
    let mut first: Option<P> = None;
    let mut last: Option<P> = None;
    let mut step = |segs: &mut Vec<Seg>, to: Seg| {
        if let Some(l) = last {
            if !near(l, to.start()) {
                segs.push(Seg::Line { a: l, b: to.start() });
            }
        }
        first.get_or_insert(to.start());
        last = Some(to.end());
        if !near(to.start(), to.end()) {
            segs.push(to);
        }
    };
    for n in list.items().iter().skip(1) {
        match n.head() {
            Some("xy") => {
                if let (Some(x), Some(y)) = (n.num(1), n.num(2)) {
                    step(&mut segs, Seg::Line { a: [x, y], b: [x, y] });
                }
            }
            Some("arc") => {
                if let (Some(a), Some(m), Some(b)) = (n.point("start"), n.point("mid"), n.point("end")) {
                    step(&mut segs, Seg::Arc { a, m, b });
                }
            }
            _ => {}
        }
    }
    if let (Some(f), Some(l)) = (first, last) {
        if !near(f, l) {
            segs.push(Seg::Line { a: l, b: f });
        }
    }
    segs
}

pub fn closed_lines(points: &[P]) -> Vec<Seg> {
    (0..points.len()).map(|i| Seg::Line { a: points[i], b: points[(i + 1) % points.len()] }).collect()
}

/// A circle as two half arcs.
pub fn circle(c: P, r: f64) -> Vec<Seg> {
    let (e, w) = ([c[0] + r, c[1]], [c[0] - r, c[1]]);
    vec![Seg::Arc { a: e, m: [c[0], c[1] + r], b: w }, Seg::Arc { a: w, m: [c[0], c[1] - r], b: e }]
}

/// Joins open pieces end to end into closed loops. Pieces that close no loop are counted, not kept.
pub fn chain(mut open: Vec<Seg>) -> (Vec<Vec<Seg>>, usize) {
    let mut loops = Vec::new();
    let mut dropped = 0;
    while let Some(first) = open.pop() {
        let mut run = vec![first];
        loop {
            let end = run.last().unwrap().end();
            if near(end, run[0].start()) && run.len() > 1 {
                loops.push(run);
                break;
            }
            let Some(i) = open.iter().position(|s| near(s.start(), end) || near(s.end(), end)) else {
                dropped += run.len();
                break;
            };
            let s = open.swap_remove(i);
            run.push(if near(s.start(), end) { s } else { s.reversed() });
        }
    }
    (loops, dropped)
}

/// A closed loop as a subpath, its arcs as SVG arcs. Each piece starts where the last ended.
pub fn subpath(segs: &[Seg]) -> Subpath {
    let start = segs[0].start();
    let mut segments = Vec::new();
    for s in segs {
        match *s {
            Seg::Line { b, .. } => segments.push(Segment::Line { to: b }),
            Seg::Arc { a, m, b } => segments.push(arc(a, m, b)),
        }
    }
    // The last piece ends on the start, which `Z` closes.
    if segments.len() > 1 {
        if let Some(Segment::Line { .. }) = segments.last() {
            segments.pop();
        } else if let Some(Segment::Arc { to, .. }) = segments.last_mut() {
            *to = start;
        }
    }
    Subpath { start, segments }
}

/// The SVG arc from `a` through `m` to `b`; a straight line when the three are in a line.
fn arc(a: P, m: P, b: P) -> Segment {
    let Some((c, r)) = circumcircle(a, m, b) else { return Segment::Line { to: b } };
    let angle = |p: P| (p[1] - c[1]).atan2(p[0] - c[0]);
    let cross = (m[0] - a[0]) * (b[1] - m[1]) - (m[1] - a[1]) * (b[0] - m[0]);
    // SVG's sweep flag 1 runs the way angles grow in the document's own coordinates.
    let sweep = cross > 0.0;
    let mut span = angle(b) - angle(a);
    if sweep {
        span = span.rem_euclid(TAU);
    } else {
        span = (-span).rem_euclid(TAU);
    }
    Segment::Arc { rx: r, ry: r, rotation: 0.0, large: span > PI, sweep, to: b }
}

fn circumcircle(a: P, b: P, c: P) -> Option<(P, f64)> {
    let d = 2.0 * (a[0] * (b[1] - c[1]) + b[0] * (c[1] - a[1]) + c[0] * (a[1] - b[1]));
    if d.abs() < 1e-12 {
        return None;
    }
    let sq = |p: P| p[0] * p[0] + p[1] * p[1];
    let ux = (sq(a) * (b[1] - c[1]) + sq(b) * (c[1] - a[1]) + sq(c) * (a[1] - b[1])) / d;
    let uy = (sq(a) * (c[0] - b[0]) + sq(b) * (a[0] - c[0]) + sq(c) * (b[0] - a[0])) / d;
    let r = (a[0] - ux).hypot(a[1] - uy);
    // A radius far beyond the board is a straight line drawn as an arc.
    (r < 1e5).then_some(([ux, uy], r))
}

/// Groups loops into shapes: each outer boundary with the loops directly inside it as holes. Loops
/// that cross others are kept as outer boundaries; the shape check then reports them.
pub fn nest(loops: Vec<Subpath>) -> Vec<Vec<Subpath>> {
    let polys: Vec<_> = loops.iter().map(|s| shape::to_polygon(std::slice::from_ref(s))).collect();
    let areas: Vec<f64> = loops.iter().map(|s| shape::signed_area(&shape::flatten(s)).abs()).collect();
    let inside = |i: usize, j: usize| {
        i != j && areas[j] > areas[i] && {
            let p = shape::flatten(&loops[i]);
            p.iter().all(|&[x, y]| polys[j].contains(&GeoPoint::from(Coord { x, y })))
        }
    };
    let depth: Vec<usize> = (0..loops.len()).map(|i| (0..loops.len()).filter(|&j| inside(i, j)).count()).collect();
    let mut shapes = Vec::new();
    for i in (0..loops.len()).filter(|&i| depth[i].is_multiple_of(2)) {
        let mut subs = vec![loops[i].clone()];
        for h in (0..loops.len()).filter(|&h| depth[h] == depth[i] + 1 && inside(h, i)) {
            subs.push(loops[h].clone());
        }
        shapes.push(subs);
    }
    shapes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chains_pieces_given_in_any_order_and_direction() {
        let segs = vec![
            Seg::Line { a: [0.0, 0.0], b: [10.0, 0.0] },
            Seg::Line { a: [0.0, 5.0], b: [10.0, 5.0] },
            Seg::Line { a: [0.0, 5.0], b: [0.0, 0.0] },
            Seg::Arc { a: [10.0, 0.0], m: [12.5, 2.5], b: [10.0, 5.0] },
            Seg::Line { a: [50.0, 0.0], b: [60.0, 0.0] },
        ];
        let (loops, dropped) = chain(segs);
        assert_eq!((loops.len(), dropped), (1, 1));
        let d = shape::write(&[subpath(&loops[0])]);
        assert!(shape::check(&d).is_empty(), "{d}");
        assert_eq!(d.matches(" A ").count(), 1);
    }

    #[test]
    fn an_arc_keeps_its_side_and_size() {
        // A half circle bulging to +x, from (0,-1) through (1,0) to (0,1): angles grow, so sweep 1.
        let Segment::Arc { rx, large, sweep, .. } = arc([0.0, -1.0], [1.0, 0.0], [0.0, 1.0]) else { panic!() };
        assert!((rx - 1.0).abs() < 1e-12 && sweep && !large);
        // Three quarters of a circle is the large arc.
        let Segment::Arc { large, sweep, .. } = arc([1.0, 0.0], [-1.0, 0.0], [0.0, -1.0]) else { panic!() };
        assert!(large && sweep);
        assert!(matches!(arc([0.0, 0.0], [1.0, 1.0], [2.0, 2.0]), Segment::Line { .. }));
    }

    #[test]
    fn a_loop_inside_another_is_its_hole() {
        let square = |o: f64, s: f64| subpath(&closed_lines(&[[o, o], [o + s, o], [o + s, o + s], [o, o + s]]));
        let shapes = nest(vec![square(2.0, 2.0), square(0.0, 10.0), square(20.0, 1.0)]);
        assert_eq!(shapes.len(), 2);
        assert_eq!(shapes.iter().map(Vec::len).max(), Some(2));
    }
}
