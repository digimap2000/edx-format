//! KiCad's S-expressions: lists of bare atoms, quoted strings and further lists.

/// One node of a KiCad file.
#[derive(Clone, Debug, PartialEq)]
pub enum Node {
    List(Vec<Node>),
    /// A bare word or number, such as `smd` or `1.27`.
    Atom(String),
    /// A quoted string, its escapes undone.
    Str(String),
}

impl Node {
    /// The list's members; none for an atom or a string.
    pub fn items(&self) -> &[Node] {
        match self {
            Node::List(items) => items,
            _ => &[],
        }
    }

    /// The word a list starts with, as `footprint` in `(footprint ...)`.
    pub fn head(&self) -> Option<&str> {
        match self.items().first() {
            Some(Node::Atom(a)) => Some(a),
            _ => None,
        }
    }

    /// An atom's or a string's text.
    pub fn text(&self) -> Option<&str> {
        match self {
            Node::Atom(s) | Node::Str(s) => Some(s),
            Node::List(_) => None,
        }
    }

    /// The `i`th member's text, counting the head as 0.
    pub fn arg(&self, i: usize) -> Option<&str> {
        self.items().get(i).and_then(Node::text)
    }

    /// The `i`th member as a number.
    pub fn num(&self, i: usize) -> Option<f64> {
        self.arg(i).and_then(|s| s.parse().ok())
    }

    /// The first member list that starts with `name`.
    pub fn child(&self, name: &str) -> Option<&Node> {
        self.items().iter().find(|n| n.head() == Some(name))
    }

    /// Every member list that starts with `name`.
    pub fn children<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Node> + 'a {
        self.items().iter().filter(move |n| n.head() == Some(name))
    }

    /// The first argument of the member list `name`, as `(layer "F.Cu")` gives `F.Cu`.
    pub fn value(&self, name: &str) -> Option<&str> {
        self.child(name).and_then(|c| c.arg(1))
    }

    /// The member list `name` as a number, as `(width 0.25)` gives 0.25.
    pub fn number(&self, name: &str) -> Option<f64> {
        self.child(name).and_then(|c| c.num(1))
    }

    /// The member list `name` as a point, as `(start 1 2)` gives `[1, 2]`.
    pub fn point(&self, name: &str) -> Option<[f64; 2]> {
        let c = self.child(name)?;
        Some([c.num(1)?, c.num(2)?])
    }

    /// Whether a bare atom `word` is a member, as `locked` in `(footprint "x" locked ...)`.
    pub fn has_atom(&self, word: &str) -> bool {
        self.items().iter().skip(1).any(|n| matches!(n, Node::Atom(a) if a == word))
    }

    /// A yes/no setting: the bare atom `name`, or `(name yes)`. KiCad has written both.
    pub fn flag(&self, name: &str) -> bool {
        self.has_atom(name) || self.child(name).is_some_and(|c| matches!(c.arg(1), None | Some("yes") | Some("true")))
    }
}

/// Reads a whole file: one top-level list.
pub fn parse(text: &str) -> Result<Node, String> {
    let bytes = text.as_bytes();
    let mut stack: Vec<Vec<Node>> = Vec::new();
    let mut root = None;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'(' => {
                stack.push(Vec::new());
                i += 1;
            }
            b')' => {
                let list = stack.pop().ok_or_else(|| format!("unbalanced ')' at byte {i}"))?;
                let node = Node::List(list);
                match stack.last_mut() {
                    Some(parent) => parent.push(node),
                    None if root.is_none() => root = Some(node),
                    None => return Err(format!("a second top-level list at byte {i}")),
                }
                i += 1;
            }
            b'"' => {
                let mut s = Vec::new();
                i += 1;
                loop {
                    match bytes.get(i) {
                        None => return Err("a string runs to the end of the file".into()),
                        Some(b'"') => break,
                        Some(b'\\') => {
                            i += 1;
                            match bytes.get(i) {
                                Some(b'n') => s.push(b'\n'),
                                Some(b't') => s.push(b'\t'),
                                Some(b'r') => s.push(b'\r'),
                                Some(&c) => s.push(c),
                                None => return Err("a string runs to the end of the file".into()),
                            }
                        }
                        Some(&c) => s.push(c),
                    }
                    i += 1;
                }
                i += 1;
                let s = String::from_utf8(s).map_err(|_| "a string is not UTF-8".to_string())?;
                stack.last_mut().ok_or("a string outside any list")?.push(Node::Str(s));
            }
            c if c.is_ascii_whitespace() => i += 1,
            _ => {
                let start = i;
                while i < bytes.len() && !matches!(bytes[i], b'(' | b')' | b'"') && !bytes[i].is_ascii_whitespace() {
                    i += 1;
                }
                let atom = std::str::from_utf8(&bytes[start..i]).map_err(|_| "an atom is not UTF-8".to_string())?;
                stack.last_mut().ok_or("an atom outside any list")?.push(Node::Atom(atom.to_owned()));
            }
        }
    }
    if !stack.is_empty() {
        return Err("the file ends inside a list".into());
    }
    root.ok_or_else(|| "the file holds no list".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_atoms_strings_and_lists() {
        let n = parse(r#"(pad "1" smd (at 1.5 -2 90) (net 3 "A \"q\"") locked (locked yes))"#).unwrap();
        assert_eq!(n.head(), Some("pad"));
        assert_eq!(n.arg(1), Some("1"));
        assert_eq!(n.point("at"), Some([1.5, -2.0]));
        assert_eq!(n.child("at").unwrap().num(3), Some(90.0));
        assert_eq!(n.child("net").unwrap().arg(2), Some("A \"q\""));
        assert!(n.has_atom("smd") && n.flag("locked"));
        assert!(!n.flag("hide"));
    }

    #[test]
    fn refuses_unbalanced_input() {
        assert!(parse("(a (b)").is_err());
        assert!(parse("(a))").is_err());
        assert!(parse("(a \"b)").is_err());
    }
}
