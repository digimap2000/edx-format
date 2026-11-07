//! Converting the KiCad boards in `tests/data` (written for these tests) and checking the result.

use edx_format::model::*;
use edx_kicad::{convert, Fixed, Options};

fn data(name: &str) -> String {
    std::fs::read_to_string(format!("{}/tests/data/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

/// Converts, then checks the written document as any reader would: no errors or warnings, complete.
fn converted(board: &str, project: Option<&str>, options: &Options) -> (Document, Vec<String>) {
    let c = convert(&data(board), project.map(data).as_deref(), options).unwrap();
    let text = edx_format::to_string(&c.document);
    // Every number is rounded as EDX keeps them: no float noise such as 0.15239999999999998.
    for number in text.split(|c: char| !(c.is_ascii_digit() || c == '.')).filter(|t| t.contains('.')) {
        assert!(number.split('.').nth(1).unwrap().len() <= 6, "{number} is not rounded");
    }
    let checked = edx_format::check(text.as_bytes());
    assert!(checked.diagnostics.is_empty(), "{:#?}", checked.diagnostics);
    let document = checked.document.unwrap();
    assert!(edx_format::incomplete(&document).is_empty());
    (document, c.notes)
}

fn near(a: Point, b: Point) -> bool {
    (a[0] - b[0]).hypot(a[1] - b[1]) < 1e-6
}

/// Where a pad lands on the board, by the document's own placement geometry.
fn on_board(doc: &Document, part: &str, pad: &str) -> Point {
    let design = doc.design.as_ref().unwrap();
    let p = design.package_of(part).unwrap().pads_numbered(pad).next().unwrap();
    doc.transform(part).unwrap().apply(p.position)
}

fn part<'a>(doc: &'a Document, reference: &str) -> &'a Part {
    doc.design.as_ref().unwrap().part(reference).unwrap()
}

#[test]
fn a_two_layer_board_keeps_its_geometry() {
    let (doc, notes) = converted("two-layer.kicad_pcb", Some("two-layer.kicad_pro"), &Options::default());
    let design = doc.design.as_ref().unwrap();

    // KiCad turns R1 a quarter turn; C1 is on the bottom, half a turn from KiCad's angle in EDX.
    let layout = doc.layout.as_ref().unwrap();
    let placed = |r: &str| layout.placement.iter().find(|p| p.reference == r).unwrap();
    assert_eq!((placed("R1").angle, placed("R1").side), (Some(90.0), Some(Side::Top)));
    assert_eq!((placed("C1").angle, placed("C1").side), (Some(180.0), Some(Side::Bottom)));

    // Pads land where KiCad has them, which is where the author's tracks end.
    assert!(near(on_board(&doc, "R1", "1"), [10.0, 10.8]));
    assert!(near(on_board(&doc, "R1", "2"), [10.0, 9.2]));
    assert!(near(on_board(&doc, "C1", "1"), [19.2, 10.0]));
    assert!(near(on_board(&doc, "J1", "3"), [2.5, 7.38]));
    let tracks = &layout.copper.tracks;
    assert!(tracks.iter().any(|t| t.net == "SIG" && near(t.start, on_board(&doc, "R1", "1"))));
    assert!(tracks.iter().any(|t| t.net == "SIG" && near(t.end, on_board(&doc, "C1", "1")) && t.locked));
    assert_eq!((layout.copper.arcs.len(), layout.copper.vias.len()), (1, 1));

    // The packages are as seen from the top: C1's pads are where KiCad's library has them.
    let cap = design.package_of("C1").unwrap();
    assert_eq!(cap.pads[0].position, [-0.8, 0.0]);
    assert_eq!(cap.pads[1].shape, PadShape::Polygon);
    assert_eq!(cap.courtyard.as_ref().unwrap().top, ["M -1.5 -0.75 L 1.5 -0.75 L 1.5 0.75 L -1.5 0.75 Z"]);
    let header = design.package_of("J1").unwrap();
    assert!(matches!(header.pads[2].drill, Some(Drill::Slot { slot: [1.0, 1.5], .. })));
    assert_eq!(header.class, "connector");
    assert_eq!(design.package_of("H1").unwrap().pads[0].number, "~1");

    // The locked header and the mounting hole are fixed; the rest are free.
    let kind = |r: &str| matches!(part(&doc, r).placement, Placement::Fixed { .. });
    assert!(kind("J1") && kind("H1") && !kind("R1") && !kind("R2") && !kind("C1"));

    // The outline keeps its rounded corners and its round cut-out.
    let outline = design.board.as_ref().unwrap().outline.as_ref().unwrap();
    assert_eq!(outline.len(), 1);
    assert_eq!(outline[0].matches(" A ").count(), 4);
    assert_eq!(outline[0].matches('M').count(), 2);

    // Nets: the unconnected pin's own net and the logo are gone; +5V is in the project's Power class.
    let nets: Vec<&str> = design.nets.as_ref().unwrap().iter().map(|n| n.name.as_str()).collect();
    assert_eq!(nets, ["SIG", "+5V", "GND"]);
    assert_eq!(design.net("+5V").unwrap().class.as_deref(), Some("Power"));
    assert!(design.part("G1").is_none());
    let rules = doc.rules.as_ref().unwrap();
    let power = &rules.classes.as_ref().unwrap()["Power"];
    assert_eq!(power.track_width.as_ref().map(|t| (t.min, t.preferred)), Some((0.15, 0.5)));
    assert_eq!(rules.board.as_ref().unwrap().copper_to_edge, Some(0.3));

    // The stack-up carries its physics.
    let top = &design.board.as_ref().unwrap().stackup[0];
    assert_eq!(top.dielectric.as_ref().map(|d| (d.thickness, d.er)), Some((1.51, 4.5)));
    assert!(design.board.as_ref().unwrap().is_physical());

    // The ground zone is a pour; the keep-out is a region.
    assert_eq!(design.pours.len(), 1);
    assert_eq!(design.pours[0].net.as_deref(), Some("GND"));
    assert_eq!(design.regions[0].excludes, ["tracks", "vias", "pours"]);

    assert!(notes.iter().any(|n| n.contains("no pads")), "{notes:?}");
    assert!(notes.iter().any(|n| n.contains("no net")), "{notes:?}");
}

#[test]
fn a_kicad_10_board_with_an_inner_plane() {
    let (doc, notes) = converted("four-layer.kicad_pcb", None, &Options::default());
    let design = doc.design.as_ref().unwrap();
    let stack = &design.board.as_ref().unwrap().stackup;
    let names: Vec<&str> = stack.iter().map(|l| l.name.as_str()).collect();
    assert_eq!(names, ["top", "In1", "In2", "bottom"]);
    assert_eq!(
        (stack[1].role, stack[1].plane_net.as_deref(), stack[1].routable),
        (LayerRole::Plane, Some("GND"), false)
    );
    assert_eq!(stack[2].role, LayerRole::Signal);

    // The plane's zone is the plane; the hatched pour stays; the teardrop goes.
    assert_eq!(design.pours.len(), 1);
    assert_eq!(design.pours[0].layers, ["top", "In2"]);
    assert!(matches!(design.pours[0].fill, Some(Fill::Hatch { angle, .. }) if angle == 45.0));
    assert!(notes.iter().any(|n| n.contains("teardrops")));

    // KiCad 10 names nets in place, and locks with `(locked yes)`.
    assert_eq!(design.net("+3V3").unwrap().pads.len(), 2);
    assert!(matches!(part(&doc, "C1").placement, Placement::Fixed { angle, .. } if angle == 90.0));
    assert!(near(on_board(&doc, "C1", "1"), [12.0, 14.52]));

    // A chamfered pad and a custom pad are polygons in the package, unturned.
    let ic = design.package_of("U1").unwrap();
    assert_eq!(ic.class, "ic");
    assert_eq!(
        ic.pads[0].outline.as_deref(),
        Some("M -0.975 -0.18 L -0.855 -0.3 L 0.975 -0.3 L 0.975 0.3 L -0.975 0.3 Z")
    );
    assert_eq!(ic.pads[2].outline.as_deref(), Some("M -0.3 0.3 L -0.3 -0.3 L 1.2 -0.3 L 1.2 0.3 Z"));
    assert_eq!(ic.pads[1].rotation, 0.0);

    // The outline's arc comes from a polygon's `(arc ...)`.
    assert_eq!(
        design.board.as_ref().unwrap().outline.as_deref().unwrap(),
        ["M 0 0 L 40 0 L 40 30 A 5 5 0 0 1 35 35 L 0 35 Z"]
    );
}

#[test]
fn the_same_footprint_on_either_side_is_one_package() {
    let board = data("two-layer.kicad_pcb")
        .replace("(footprint \"Capacitor_SMD:C_0603_1608Metric\"", "(footprint \"Resistor_SMD:R_0603_1608Metric\"");
    let board = board.replace(
        "(pad \"2\" smd trapezoid (at 0.8 0) (size 0.9 0.95) (rect_delta 0 0.2)",
        "(pad \"2\" smd rect (at 0.8 0) (size 0.9 0.95)",
    );
    let board = board.replace("(net 2 \"GND\") (zone_connect 2)", "(net 2 \"GND\")");
    let board = board.replace("(pad \"1\" smd roundrect (at -0.8 0 90) (size 0.9 0.95) (layers \"F.Cu\" \"F.Paste\" \"F.Mask\") (roundrect_rratio 0.25) (net 1 \"SIG\"))", "(pad \"1\" smd rect (at -0.8 0 90) (size 0.9 0.95) (layers \"F.Cu\" \"F.Paste\" \"F.Mask\") (net 1 \"SIG\"))");
    let board = board.replace("(pad \"2\" smd roundrect (at 0.8 0 90) (size 0.9 0.95) (layers \"F.Cu\" \"F.Paste\" \"F.Mask\") (roundrect_rratio 0.25) (net 3 \"+5V\"))", "(pad \"2\" smd rect (at 0.8 0 90) (size 0.9 0.95) (layers \"F.Cu\" \"F.Paste\" \"F.Mask\") (net 3 \"+5V\"))");
    let doc = convert(&board, None, &Options::default()).unwrap().document;
    let design = doc.design.unwrap();
    assert_eq!(design.part("R1").unwrap().package, design.part("C1").unwrap().package);
    // R2 keeps its round corners, so it is a package of its own.
    assert_ne!(design.part("R1").unwrap().package, design.part("R2").unwrap().package);
}

#[test]
fn options_choose_what_is_fixed_and_whether_the_layout_is_written() {
    let fixed = |fixed| {
        let (doc, _) = converted("two-layer.kicad_pcb", None, &Options { fixed, ..Options::default() });
        let parts = doc.design.unwrap().parts.unwrap();
        parts
            .iter()
            .filter(|p| matches!(p.placement, Placement::Fixed { .. }))
            .map(|p| p.reference.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(fixed(Fixed::All).len(), 5);
    assert!(fixed(Fixed::None).is_empty());
    assert_eq!(fixed(Fixed::Locked), ["J1"]);

    let (doc, _) = converted("two-layer.kicad_pcb", None, &Options { layout: false, ..Options::default() });
    assert!(doc.layout.is_none());
}

#[test]
fn a_part_whose_courtyard_reaches_the_edge_is_fixed() {
    // Move R2 to 0.5 mm from the top edge: its courtyard comes within the default 1 mm.
    let board = data("two-layer.kicad_pcb").replacen("(at 14 6)", "(at 14 1.25)", 1);
    let doc = convert(&board, None, &Options::default()).unwrap().document;
    assert!(matches!(doc.design.unwrap().part("R2").unwrap().placement, Placement::Fixed { .. }));
}

/// The two-layer board with R2's courtyard taken away.
fn without_r2_courtyard() -> String {
    let board = data("two-layer.kicad_pcb");
    let r2 = "(fp_text value \"1k\" (at 0 1.4) (layer \"F.Fab\"))\n";
    let courtyard = "    (fp_rect (start -1.5 -0.75) (end 1.5 0.75) (layer \"F.CrtYd\") (width 0.05) (fill none))\n";
    let stripped = board.replacen(&format!("{r2}{courtyard}"), r2, 1);
    assert_ne!(stripped, board);
    stripped
}

#[test]
fn a_footprint_with_no_courtyard_is_converted_without_one() {
    let c = convert(&without_r2_courtyard(), None, &Options::default()).unwrap();
    let text = edx_format::to_string(&c.document);
    let checked = edx_format::check(text.as_bytes());
    assert!(checked.diagnostics.is_empty(), "{:#?}", checked.diagnostics);
    let doc = checked.document.unwrap();
    let design = doc.design.as_ref().unwrap();
    assert!(design.package_of("R2").unwrap().courtyard.is_none());
    assert!(design.package_of("R1").unwrap().courtyard.is_some());
    assert!(c.notes.iter().any(|n| n.contains("converted without one")), "{:?}", c.notes);
    // Far from the edge, it stays free.
    assert!(matches!(design.part("R2").unwrap().placement, Placement::Free { .. }));
}

#[test]
fn a_part_with_no_courtyard_is_at_the_edge_by_its_pads() {
    // R2's pads reach 0.475 mm either side of its centre: at 1.25 mm from the top edge they come within 1 mm.
    let board = without_r2_courtyard().replacen("(at 14 6)", "(at 14 1.25)", 1);
    let doc = convert(&board, None, &Options::default()).unwrap().document;
    assert!(matches!(doc.design.unwrap().part("R2").unwrap().placement, Placement::Fixed { .. }));
    // At 1.6 mm its pads are 1.125 mm from the edge: no longer at it (the invented courtyard was).
    let board = without_r2_courtyard().replacen("(at 14 6)", "(at 14 1.6)", 1);
    let doc = convert(&board, None, &Options::default()).unwrap().document;
    assert!(matches!(doc.design.unwrap().part("R2").unwrap().placement, Placement::Free { .. }));
}

#[test]
fn files_it_cannot_read_are_refused_with_a_reason() {
    let old = convert("(kicad_pcb (version 20171130) (host pcbnew 5.1.9))", None, &Options::default());
    assert!(old.err().unwrap().contains("KiCad 5"));
    assert!(convert("(kicad_sch (version 20211123))", None, &Options::default()).is_err());
}

#[test]
fn a_board_with_no_outline_converts_without_one() {
    let no_outline = data("two-layer.kicad_pcb").replace("\"Edge.Cuts\") (width", "\"Dwgs.User\") (width");
    assert_ne!(no_outline, data("two-layer.kicad_pcb"));
    let converted = convert(&no_outline, None, &Options::default()).unwrap();
    assert!(converted.notes.iter().any(|n| n.contains("no closed outline")), "{:?}", converted.notes);
    let design = converted.document.design.as_ref().unwrap();
    assert!(design.board.as_ref().unwrap().outline.is_none());
    // With no outline there is no edge, so the edge test fixes no part that isn't locked.
    let fixed = |r: &str| matches!(design.part(r).unwrap().placement, Placement::Fixed { .. });
    assert!(!fixed("R1") && !fixed("R2"));
    // The document is valid EDX and writes no outline.
    let text = edx_format::to_string(&converted.document);
    let value: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert!(value["design"]["board"].get("outline").is_none(), "{}", value["design"]["board"]);
    assert!(edx_format::check(text.as_bytes()).is_ok(), "{text}");
}
