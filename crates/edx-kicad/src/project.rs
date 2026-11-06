//! The design rules a KiCad 6 or later project keeps beside its board, in `<name>.kicad_pro`: the
//! net classes, which nets are in each, and the board-wide minimums.

use std::collections::BTreeMap;

use serde_json::Value;

/// A net class as KiCad states it, lengths in mm.
#[derive(Clone, Debug, PartialEq)]
pub struct Class {
    pub clearance: Option<f64>,
    pub track_width: Option<f64>,
    pub via_diameter: Option<f64>,
    pub via_drill: Option<f64>,
}

/// What a project says about the rules.
#[derive(Clone, Debug, Default)]
pub struct Project {
    /// KiCad's class names; `Default` is always there once a project is read.
    pub classes: BTreeMap<String, Class>,
    /// Net name to class name, from the project's explicit lists (KiCad 6) and assignments (7 on).
    assigned: BTreeMap<String, String>,
    /// `(class, pattern)` in the project's order (KiCad 7 on); `*` and `?` are wildcards.
    patterns: Vec<(String, String)>,
    pub min_track_width: Option<f64>,
    pub copper_to_edge: Option<f64>,
    pub hole_to_hole: Option<f64>,
}

/// KiCad's own defaults for a new project, used when no project is given.
pub const KICAD_DEFAULT: Class =
    Class { clearance: Some(0.2), track_width: Some(0.25), via_diameter: Some(0.6), via_drill: Some(0.3) };

/// A length the project gives, rounded to the nanometre as EDX keeps lengths: KiCad writes some
/// converted from mils with float noise (`0.15239999999999998`).
fn positive(v: Option<&Value>) -> Option<f64> {
    v.and_then(Value::as_f64).filter(|v| *v > 0.0).map(edx_format::shape::round_nm)
}

impl Project {
    pub fn read(text: &str) -> Result<Project, String> {
        let json: Value = serde_json::from_str(text).map_err(|e| format!("the project file is not JSON: {e}"))?;
        let mut p = Project::default();
        let rules = &json["board"]["design_settings"]["rules"];
        p.min_track_width = positive(rules.get("min_track_width"));
        p.copper_to_edge = positive(rules.get("min_copper_edge_clearance"));
        p.hole_to_hole = positive(rules.get("min_hole_to_hole"));
        let settings = &json["net_settings"];
        for c in settings["classes"].as_array().into_iter().flatten() {
            let Some(name) = c["name"].as_str() else { continue };
            p.classes.insert(
                name.to_owned(),
                Class {
                    clearance: positive(c.get("clearance")),
                    track_width: positive(c.get("track_width")),
                    via_diameter: positive(c.get("via_diameter")),
                    via_drill: positive(c.get("via_drill")),
                },
            );
            for net in c["nets"].as_array().into_iter().flatten().filter_map(Value::as_str) {
                p.assigned.insert(net.to_owned(), name.to_owned());
            }
        }
        for pat in settings["netclass_patterns"].as_array().into_iter().flatten() {
            if let (Some(class), Some(pattern)) = (pat["netclass"].as_str(), pat["pattern"].as_str()) {
                p.patterns.push((class.to_owned(), pattern.to_owned()));
            }
        }
        for (net, class) in settings["netclass_assignments"].as_object().into_iter().flatten() {
            // KiCad 8 on may list several classes for a net; the first is its own.
            let class = class.as_str().or_else(|| class.as_array().and_then(|a| a.first()).and_then(Value::as_str));
            if let Some(class) = class {
                p.assigned.insert(net.clone(), class.to_owned());
            }
        }
        p.classes.entry("Default".into()).or_insert(KICAD_DEFAULT);
        Ok(p)
    }

    /// The KiCad class of a net: its assignment, else the first pattern it matches, else `Default`.
    pub fn class_of(&self, net: &str) -> &str {
        let found = self
            .assigned
            .get(net)
            .map(String::as_str)
            .or_else(|| self.patterns.iter().find(|(_, pat)| glob(pat, net)).map(|(c, _)| c.as_str()));
        match found {
            Some(c) if self.classes.contains_key(c) => c,
            _ => "Default",
        }
    }
}

/// Whether `text` matches `pattern`, where `*` matches any run of characters and `?` any one.
fn glob(pattern: &str, text: &str) -> bool {
    let (p, t): (Vec<char>, Vec<char>) = (pattern.chars().collect(), text.chars().collect());
    let (mut pi, mut ti, mut star, mut mark) = (0, 0, None, 0);
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = ti;
            pi += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    p[pi..].iter().all(|&c| c == '*')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcards_match_as_kicad_does() {
        assert!(glob("+3V3", "+3V3") && glob("*V3", "+3V3") && glob("/USB_D?", "/USB_DP") && glob("*", ""));
        assert!(!glob("+3V3", "+3V30") && !glob("/USB_D?", "/USB_D"));
    }

    #[test]
    fn a_net_takes_its_assignment_then_a_pattern_then_default() {
        let p = Project::read(
            r#"{ "board": { "design_settings": { "rules": { "min_track_width": 0.15239999999999998, "min_copper_edge_clearance": 0 } } },
                 "net_settings": {
                   "classes": [ { "name": "Default", "clearance": 0.2, "track_width": 0.2, "via_diameter": 0.6, "via_drill": 0.3 },
                                { "name": "Power", "clearance": 0.2, "track_width": 0.5, "via_diameter": 0.8, "via_drill": 0.4 } ],
                   "netclass_patterns": [ { "netclass": "Power", "pattern": "+*" } ],
                   "netclass_assignments": { "+1V8_LDO": "Default", "VBUS": ["Power", "Default"] } } }"#,
        )
        .unwrap();
        assert_eq!(
            (p.class_of("+5V"), p.class_of("+1V8_LDO"), p.class_of("VBUS"), p.class_of("SDA")),
            ("Power", "Default", "Power", "Default")
        );
        assert_eq!((p.min_track_width, p.copper_to_edge), (Some(0.1524), None));
    }
}
