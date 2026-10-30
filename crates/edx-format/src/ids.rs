//! Display forms and derived ids (specification, sections 2.6, 6.1 and 9.5).

use crate::model::{Operand, PadRef, Relation};

/// Escapes a name for use inside an id: `%`, `.`, `:`, `-` and `+`.
pub fn escape(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        match c {
            '%' => out.push_str("%25"),
            '.' => out.push_str("%2E"),
            ':' => out.push_str("%3A"),
            '-' => out.push_str("%2D"),
            '+' => out.push_str("%2B"),
            c => out.push(c),
        }
    }
    out
}

/// A pad reference's escaped display form, `part.pad`.
pub fn pad_id(pad: &PadRef) -> String {
    format!("{}.{}", escape(&pad.part), escape(&pad.pad))
}

/// A relation's id, `kind:subject:object`.
pub fn relation_id(r: &Relation) -> String {
    format!("{}:{}:{}", r.kind, pad_id(&r.subject), pad_id(&r.object))
}

/// The id of a constraint derived from its type and operands.
pub fn constraint_id(kind: &str, operands: &[Operand]) -> String {
    let mut id = kind.to_owned();
    for operand in operands {
        id.push(':');
        id.push_str(&match operand {
            Operand::Pad(p) => pad_id(p),
            Operand::Part { part, .. } => escape(part),
            Operand::Net { net, .. } => escape(net),
            Operand::Block { block, .. } => escape(block),
            Operand::Region { region, .. } => escape(region),
            Operand::Path { path, .. } => path.iter().map(pad_id).collect::<Vec<_>>().join("-"),
            Operand::Relations { relations, .. } => relations.iter().map(|r| escape(r)).collect::<Vec<_>>().join("+"),
            Operand::Other(v) => escape(&v.to_string()),
        });
    }
    id
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pad(part: &str, pad: &str) -> PadRef {
        PadRef { part: part.into(), pad: pad.into(), ext: Default::default() }
    }

    #[test]
    fn escapes_separators() {
        assert_eq!(escape("a%b.c:d-e+f"), "a%25b%2Ec%3Ad%2De%2Bf");
    }

    #[test]
    fn relation_ids() {
        let r = Relation {
            kind: "decouples".into(),
            subject: pad("C.1", "1"),
            object: pad("U1", "A.1"),
            id: None,
            source: None,
            ext: Default::default(),
        };
        assert_eq!(relation_id(&r), "decouples:C%2E1.1:U1.A%2E1");
    }

    #[test]
    fn constraint_ids() {
        let ops = [Operand::Path { path: vec![pad("X1", "1"), pad("U1", "2")], ext: Default::default() }];
        assert_eq!(constraint_id("same_layer", &ops), "same_layer:X1.1-U1.2");
        let ops = [Operand::Relations { relations: vec!["decouples:C1.1:U1.8".into()], ext: Default::default() }];
        assert_eq!(constraint_id("via_policy", &ops), "via_policy:decouples%3AC1%2E1%3AU1%2E8");
    }
}
