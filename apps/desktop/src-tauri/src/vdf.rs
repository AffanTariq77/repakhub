//! Minimal parser for Valve's text KeyValues format (.vdf / .acf).
//! Keys are lowercased so lookups survive Steam's inconsistent casing.

use std::collections::HashMap;
use std::iter::Peekable;
use std::str::Chars;

#[derive(Debug, PartialEq)]
pub enum Vdf {
    Str(String),
    Obj(HashMap<String, Vdf>),
}

impl Vdf {
    pub fn get(&self, key: &str) -> Option<&Vdf> {
        match self {
            Vdf::Obj(m) => m.get(key),
            Vdf::Str(_) => None,
        }
    }

    pub fn str(&self, key: &str) -> Option<&str> {
        match self.get(key) {
            Some(Vdf::Str(s)) => Some(s),
            _ => None,
        }
    }
}

enum Tok {
    Str(String),
    Open,
    Close,
}

/// Real Steam files nest 4-5 levels. The cap stops crafted files from overflowing the stack.
const MAX_DEPTH: u32 = 32;

pub fn parse(src: &str) -> Option<Vdf> {
    parse_obj(&mut src.chars().peekable(), 0).map(Vdf::Obj)
}

fn parse_obj(it: &mut Peekable<Chars>, depth: u32) -> Option<HashMap<String, Vdf>> {
    if depth > MAX_DEPTH {
        return None;
    }
    let nested = depth > 0;
    let mut map = HashMap::new();
    loop {
        match next_token(it) {
            None => return (!nested).then_some(map),
            Some(Tok::Close) => return nested.then_some(map),
            Some(Tok::Open) => return None,
            Some(Tok::Str(key)) => {
                let value = match next_token(it)? {
                    Tok::Str(v) => Vdf::Str(v),
                    Tok::Open => Vdf::Obj(parse_obj(it, depth + 1)?),
                    Tok::Close => return None,
                };
                map.insert(key.to_lowercase(), value);
            }
        }
    }
}

fn next_token(it: &mut Peekable<Chars>) -> Option<Tok> {
    loop {
        match *it.peek()? {
            c if c.is_whitespace() => {
                it.next();
            }
            '/' => {
                // `//` comment to end of line
                it.by_ref().take_while(|&c| c != '\n').for_each(drop);
            }
            '{' => {
                it.next();
                return Some(Tok::Open);
            }
            '}' => {
                it.next();
                return Some(Tok::Close);
            }
            '"' => {
                it.next();
                let mut s = String::new();
                while let Some(c) = it.next() {
                    match c {
                        '"' => return Some(Tok::Str(s)),
                        '\\' => match it.next()? {
                            'n' => s.push('\n'),
                            't' => s.push('\t'),
                            other => s.push(other),
                        },
                        c => s.push(c),
                    }
                }
                return None; // unterminated string
            }
            _ => {
                let mut s = String::new();
                while let Some(&c) = it.peek() {
                    if c.is_whitespace() || c == '{' || c == '}' || c == '"' {
                        break;
                    }
                    s.push(c);
                    it.next();
                }
                return Some(Tok::Str(s));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nested_escaped_and_commented() {
        let v = parse(
            r#"
            // comment
            "LibraryFolders"
            {
                "0" { "path" "C:\\Program Files (x86)\\Steam" "label" "" }
                Unquoted value
            }"#,
        )
        .unwrap();
        let lf = v.get("libraryfolders").unwrap();
        assert_eq!(lf.get("0").unwrap().str("path"), Some(r"C:\Program Files (x86)\Steam"));
        assert_eq!(lf.get("0").unwrap().str("label"), Some(""));
        assert_eq!(lf.str("unquoted"), Some("value"));
    }

    #[test]
    fn rejects_malformed() {
        assert!(parse(r#""a" { "b" "c" "#).is_none());
        assert!(parse(r#""a" "b" }"#).is_none());
        assert!(parse(r#""a" "unterminated"#).is_none());
        let deep = r#""k" {"#.repeat(10_000);
        assert!(parse(&deep).is_none(), "deep nesting rejected without overflowing the stack");
    }
}
