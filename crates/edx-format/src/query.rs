//! Lookups a reader needs on a checked document: records by identity, where a part stands, a net
//! class's values with their defaults, and a report's gates (sections 3.1, 8.2, 10, 11.1 and 11.2).

use crate::geometry::Transform;
use crate::model::*;

/// The class a net without one is in, which a complete document defines in full (section 4).
pub const DEFAULT_CLASS: &str = "default";

impl Design {
    pub fn part(&self, reference: &str) -> Option<&Part> {
        self.parts.iter().flatten().find(|p| p.reference == reference)
    }

    pub fn package(&self, id: &str) -> Option<&Package> {
        self.packages.iter().flatten().find(|p| p.id == id)
    }

    /// A part's package.
    pub fn package_of(&self, reference: &str) -> Option<&Package> {
        self.part(reference).and_then(|p| self.package(&p.package))
    }

    pub fn net(&self, name: &str) -> Option<&Net> {
        self.nets.iter().flatten().find(|n| n.name == name)
    }

    /// The net a pad is in, if any (section 5.4).
    pub fn net_of(&self, pad: &PadRef) -> Option<&Net> {
        self.nets.iter().flatten().find(|n| n.pads.iter().any(|p| p.part == pad.part && p.pad == pad.pad))
    }

    /// A copper layer by name.
    pub fn layer(&self, name: &str) -> Option<&Layer> {
        self.board.as_ref().and_then(|b| b.stackup.iter().find(|l| l.name == name))
    }

    pub fn region(&self, name: &str) -> Option<&Region> {
        self.regions.iter().find(|r| r.name == name)
    }
}

impl Package {
    /// The pads with a number: more than one when they share it, as one electrical pad (section 5.2).
    pub fn pads_numbered<'a>(&'a self, number: &'a str) -> impl Iterator<Item = &'a Pad> + 'a {
        self.pads.iter().filter(move |p| p.number == number)
    }
}

impl Net {
    /// The net's class: `default` when it names none (section 5.4).
    pub fn class_name(&self) -> &str {
        self.class.as_deref().unwrap_or(DEFAULT_CLASS)
    }
}

impl Document {
    /// Where a part stands, if it is placed: the layout's placement when the document has a layout
    /// that lists the part, else the design's `fixed` placement. A `free` part, or one the layout
    /// lists as not placed, has none.
    pub fn transform(&self, reference: &str) -> Option<Transform> {
        let part = self.design.as_ref().and_then(|d| d.part(reference));
        if let Some(placed) = self.layout.as_ref().and_then(|l| l.placement.iter().find(|p| p.reference == reference)) {
            let side = placed.side.or(part.map(|p| p.side))?;
            return match (placed.placed, placed.at, placed.angle) {
                (true, Some(at), angle) => Some(Transform::new(at, angle.unwrap_or(0.0), side)),
                _ => None,
            };
        }
        match &part?.placement {
            Placement::Fixed { at, angle, .. } => Some(Transform::new(*at, *angle, part?.side)),
            Placement::Free { .. } => None,
        }
    }
}

/// A net class's values, each from the class or else from `default` (section 8.2). A value neither
/// gives is `None`; a complete document gives every one.
#[derive(Clone, Debug, Default)]
pub struct ClassValues {
    pub clearance: Option<f64>,
    pub track_width: Option<TrackWidth>,
    pub via: Option<ViaSize>,
}

impl Rules {
    /// A class's values, on a layer if one is named. Each value comes from the first that gives it:
    /// the class's override for the layer, the class, `default`'s override for the layer, `default`.
    pub fn class(&self, name: &str, layer: Option<&str>) -> ClassValues {
        let classes = self.classes.as_ref();
        let get = |n: &str| classes.and_then(|c| c.get(n));
        let mut chain: Vec<&NetClass> = Vec::new();
        for n in [name, DEFAULT_CLASS] {
            if let Some(class) = get(n) {
                if let Some(l) = layer.and_then(|l| class.layers.get(l)) {
                    chain.push(l);
                }
                chain.push(class);
            }
            if name == DEFAULT_CLASS {
                break;
            }
        }
        ClassValues {
            clearance: chain.iter().find_map(|c| c.clearance),
            track_width: chain.iter().find_map(|c| c.track_width.clone()),
            via: chain.iter().find_map(|c| c.via.clone()),
        }
    }
}

impl Report {
    /// The gates its lists give (section 11.2): every hard constraint it lists is met, no part is
    /// unplaced, no net unrouted, no clearance violation. An engine that lists every constraint in
    /// force, as it must list every broken hard one, states these as its `gates`.
    pub fn gates_from_lists(&self) -> Gates {
        Gates {
            constraints_met: self.constraints.iter().filter(|c| c.hard).all(|c| c.status == Status::Met),
            all_placed: self.unplaced.is_empty(),
            all_routed: self.unrouted.is_empty(),
            clearance_clean: self.clearance.is_empty(),
            ext: Ext::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(json: &str) -> Document {
        let checked = crate::check(json.as_bytes());
        assert!(checked.is_ok(), "{:?}", checked.diagnostics);
        checked.document.unwrap()
    }

    #[test]
    fn class_values_fall_back_to_default() {
        let d = doc(r#"{ "edx": "1.0", "rules": { "classes": {
            "default": { "clearance": 0.15, "track_width": { "min": 0.15, "preferred": 0.2 }, "via": { "pad": 0.6, "drill": 0.3 },
                         "layers": { "bottom": { "clearance": 0.2 } } },
            "Supply": { "track_width": { "min": 0.3, "preferred": 0.5 },
                        "layers": { "bottom": { "track_width": { "min": 0.4, "preferred": 0.6 } } } } } } }"#);
        let rules = d.rules.as_ref().unwrap();
        let s = rules.class("Supply", None);
        assert_eq!((s.clearance, s.track_width.unwrap().preferred, s.via.unwrap().drill), (Some(0.15), 0.5, 0.3));
        let s = rules.class("Supply", Some("bottom"));
        assert_eq!((s.clearance, s.track_width.unwrap().preferred), (Some(0.2), 0.6));
        assert_eq!(rules.class("default", Some("bottom")).clearance, Some(0.2));
        assert_eq!(rules.class("Unknown", None).clearance, Some(0.15));
        let missing: Vec<String> = crate::incomplete(&d).into_iter().map(|e| e.message).collect();
        assert_eq!(missing, ["a layout engine or checker needs a design"]);
    }

    #[test]
    fn layout_placement_wins_over_the_design() {
        let d = doc(r#"{ "edx": "1.0",
            "design": {
              "packages": [ { "id": "R0402", "class": "passive", "courtyard": { "top": ["M -1 -0.5 H 1 V 0.5 H -1 Z"] },
                "pads": [ { "number": "1", "kind": "smd", "shape": "rect", "size": [0.5, 0.5], "position": [-0.5, 0], "layers": "top" },
                          { "number": "2", "kind": "smd", "shape": "rect", "size": [0.5, 0.5], "position": [0.5, 0], "layers": "top" } ] } ],
              "parts": [ { "ref": "R1", "package": "R0402", "side": "top", "placement": { "kind": "fixed", "at": [1, 2], "angle": 0 } },
                         { "ref": "R2", "package": "R0402", "side": "top", "placement": { "kind": "free" } },
                         { "ref": "R3", "package": "R0402", "side": "top", "placement": { "kind": "free" } } ],
              "nets": [ { "name": "A", "pads": [ { "part": "R1", "pad": "1" }, { "part": "R2", "pad": "2" } ] } ] } }"#);
        let design = d.design.as_ref().unwrap();
        assert_eq!(design.package_of("R2").unwrap().pads_numbered("2").count(), 1);
        let pad = PadRef { part: "R2".into(), pad: "2".into(), ext: Default::default() };
        assert_eq!(design.net_of(&pad).unwrap().class_name(), "default");
        assert_eq!(d.transform("R1").unwrap().at, [1.0, 2.0]);
        assert!(d.transform("R2").is_none());

        let mut d = d;
        let placed = |r: &str, placed: bool| PlacedPart {
            reference: r.into(),
            placed,
            at: placed.then_some([5.0, 6.0]),
            angle: placed.then_some(90.0),
            side: placed.then_some(Side::Bottom),
            locked: false,
            ext: Default::default(),
        };
        d.layout = Some(Layout { placement: vec![placed("R1", false), placed("R2", true)], ..Default::default() });
        assert!(d.transform("R1").is_none());
        assert_eq!(d.transform("R2"), Some(Transform::new([5.0, 6.0], 90.0, Side::Bottom)));
        assert!(d.transform("R3").is_none());
    }

    #[test]
    fn gates_from_a_reports_lists() {
        let status = |id: &str, status, hard| ConstraintStatus {
            id: id.into(),
            status,
            measured: None,
            limit: None,
            measure: None,
            hard,
            cost: None,
            location: None,
            ext: Default::default(),
        };
        let mut r = Report {
            constraints: vec![status("a", Status::Met, true), status("b", Status::Broken, false)],
            ..Default::default()
        };
        let g = r.gates_from_lists();
        assert!(g.constraints_met && g.all_placed && g.all_routed && g.clearance_clean);
        r.constraints.push(status("c", Status::NotYetMeasurable, true));
        r.unplaced.push("R1".into());
        let g = r.gates_from_lists();
        assert!(!g.constraints_met && !g.all_placed && g.all_routed);
    }
}
