//! Symbol extraction.
//!
//! PROTOTYPE NOTE: this is a line scanner, not a parser. It is deliberately
//! crude, because the question this prototype answers is about *query* latency
//! and index size, and extraction quality affects neither — it only affects
//! index BUILD time, which is amortized. A real implementation would use
//! tree-sitter (slower to build, far more accurate); swapping it in would not
//! change the numbers this prototype is measuring.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Function,
    Struct,
    Enum,
    Trait,
    Class,
    Interface,
    TypeAlias,
    Const,
}

impl Kind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Kind::Function => "fn",
            Kind::Struct => "struct",
            Kind::Enum => "enum",
            Kind::Trait => "trait",
            Kind::Class => "class",
            Kind::Interface => "interface",
            Kind::TypeAlias => "type",
            Kind::Const => "const",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Symbol {
    pub name: String,
    pub kind: Kind,
    /// Index into the index's `files` table, so paths are stored once.
    pub file: u32,
    /// 1-based, inclusive.
    pub line_start: u32,
    pub line_end: u32,
    /// The declaration line, trimmed — enough to answer "what is this?"
    /// without reading the body.
    pub signature: String,
}

/// Language dispatch by extension.
pub fn language_of(path: &str) -> Option<Lang> {
    let ext = path.rsplit('.').next()?;
    Some(match ext {
        "rs" => Lang::Rust,
        "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" => Lang::TypeScript,
        "py" => Lang::Python,
        "go" => Lang::Go,
        _ => return None,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Rust,
    TypeScript,
    Python,
    Go,
}

/// Pull symbols out of one file's text.
pub fn extract(text: &str, lang: Lang, file: u32) -> Vec<Symbol> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();

    for (i, raw) in lines.iter().enumerate() {
        let line = raw.trim_start();
        if line.is_empty()
            || line.starts_with("//")
            || line.starts_with('#') && lang != Lang::Python
        {
            continue;
        }

        let Some((kind, name)) = declaration(line, lang) else {
            continue;
        };
        // Skip obvious noise: single-letter names are usually generics or loop vars
        // that slipped past the crude matcher.
        if name.len() < 2
            || !name
                .chars()
                .next()
                .is_some_and(|c| c.is_alphabetic() || c == '_')
        {
            continue;
        }

        let end = match lang {
            Lang::Python => end_by_indent(&lines, i),
            _ => end_by_braces(&lines, i),
        };

        out.push(Symbol {
            name: name.to_string(),
            kind,
            file,
            line_start: (i + 1) as u32,
            line_end: end as u32,
            signature: line
                .trim_end()
                .trim_end_matches('{')
                .trim()
                .chars()
                .take(200)
                .collect(),
        });
    }
    out
}

fn declaration(line: &str, lang: Lang) -> Option<(Kind, &str)> {
    // Strip modifiers so one set of patterns covers `pub async fn`, `export
    // default class`, and friends.
    let mut s = line;
    for m in [
        "pub(crate) ",
        "pub(super) ",
        "pub ",
        "export default ",
        "export ",
        "async ",
        "default ",
        "const ",
        "static ",
        "abstract ",
        "declare ",
        "unsafe ",
        "extern ",
    ] {
        if let Some(rest) = s.strip_prefix(m) {
            // `const NAME =` in TS/Rust is itself a declaration worth keeping.
            if m == "const " || m == "static " {
                if let Some(name) = ident_after(rest, "") {
                    if rest.contains('=') || rest.contains(':') {
                        return Some((Kind::Const, name));
                    }
                }
            }
            s = rest;
        }
    }

    let pairs: &[(&str, Kind)] = match lang {
        Lang::Rust => &[
            ("fn ", Kind::Function),
            ("struct ", Kind::Struct),
            ("enum ", Kind::Enum),
            ("trait ", Kind::Trait),
            ("type ", Kind::TypeAlias),
        ],
        Lang::TypeScript => &[
            ("function ", Kind::Function),
            ("class ", Kind::Class),
            ("interface ", Kind::Interface),
            ("type ", Kind::TypeAlias),
            ("enum ", Kind::Enum),
        ],
        Lang::Python => &[("def ", Kind::Function), ("class ", Kind::Class)],
        Lang::Go => &[("func ", Kind::Function), ("type ", Kind::TypeAlias)],
    };

    for (prefix, kind) in pairs {
        if let Some(rest) = s.strip_prefix(prefix) {
            if let Some(name) = ident_after(rest, "") {
                return Some((*kind, name));
            }
        }
    }
    None
}

/// First identifier in `s`, skipping an optional prefix.
fn ident_after<'a>(s: &'a str, skip: &str) -> Option<&'a str> {
    let s = s.strip_prefix(skip).unwrap_or(s).trim_start();
    let s = s.strip_prefix('*').unwrap_or(s); // go pointer receivers
    let end = s
        .find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$'))
        .unwrap_or(s.len());
    if end == 0 {
        return None;
    }
    Some(&s[..end])
}

/// Walk braces from the declaration to find the body's last line.
///
/// Approximate by design: strings and comments containing braces will fool it.
/// A parser would not, which is the main thing tree-sitter would buy.
fn end_by_braces(lines: &[&str], start: usize) -> usize {
    let mut depth = 0i32;
    let mut opened = false;

    for (offset, line) in lines[start..].iter().enumerate() {
        for ch in line.chars() {
            match ch {
                '{' => {
                    depth += 1;
                    opened = true;
                }
                '}' => depth -= 1,
                _ => {}
            }
        }
        if opened && depth <= 0 {
            return start + offset + 1;
        }
        // A declaration with no body (trait method, type alias) ends on its line.
        if !opened && line.trim_end().ends_with(';') {
            return start + offset + 1;
        }
        if offset > 3000 {
            break; // runaway guard
        }
    }
    start + 1
}

/// Python bodies end where indentation returns to the declaration's level.
fn end_by_indent(lines: &[&str], start: usize) -> usize {
    let base = indent_of(lines[start]);
    let mut last = start;
    for (offset, line) in lines[start + 1..].iter().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        if indent_of(line) <= base {
            return start + offset + 1;
        }
        last = start + offset + 1;
    }
    last + 1
}

fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start().len()
}
