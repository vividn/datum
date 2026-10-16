//! A help page at the width it is shown in (da-nt53d).
//!
//! The pages are written by hand at about 73 columns. [`fit`] reflows them
//! to a narrower one — `$COLUMNS` or the terminal natively, the REPL's own
//! width on datum.day — and leaves a page that already fits byte for byte.
//!
//! Three shapes of line, read off the text:
//! - a ROW: an indent, a left part, a gap of two spaces or more, a
//!   description (`  -q, --quick   Go back 5 min`). Its description wraps
//!   under its own column, so a table still lines up; a table whose column
//!   leaves too little room stacks instead, each description under its
//!   left part.
//! - PROSE: lines a hand wrapped are one paragraph, joined and wrapped
//!   again, ragged nowhere.
//! - anything else (a usage line, an example) wraps alone, its tail
//!   indented under it.

/// A line ending this far out was wrapped by hand, so the line under it
/// at the same indent continues it; a shorter one was ended on purpose
/// (`-t 'sep28 22:30'` over `-t 'yesterday 10am'`).
const HAND: usize = 60;

/// The fewest columns a description is wrapped into beside its left part.
const ROOM: usize = 16;

/// `text` reflowed so that no line is wider than `width` columns, or than
/// [`ROOM`] where `width` is less: narrower, nothing reads.
pub fn fit(text: &str, width: usize) -> String {
    let width = width.max(ROOM);
    let lines: Vec<&str> = text.split('\n').collect();
    let mut paras: Vec<(&[&str], Option<usize>)> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let col = gap(lines[i]);
        let hang = col.unwrap_or_else(|| indent(lines[i]));
        let mut j = i + 1;
        while j < lines.len() && continues(lines[j - 1], lines[j], hang) {
            j += 1;
        }
        paras.push((&lines[i..j], col));
        i = j;
    }
    let over = |p: &[&str]| p.iter().any(|l| cols(l) > width);
    let mut out: Vec<String> = Vec::new();
    let mut k = 0;
    while k < paras.len() {
        // A table is the rows standing together: one that has no room
        // stacks, and with it all of them, so its columns still agree.
        let end = k + paras[k..]
            .iter()
            .take_while(|(_, c)| c.is_some())
            .count()
            .max(1);
        let table = &paras[k..end];
        let stack = table
            .iter()
            .any(|(p, c)| c.is_some_and(|c| over(p) && width < c + ROOM));
        for &(p, col) in table {
            if over(p) || stack {
                wrap(&mut out, p, col, stack, width);
            } else {
                out.extend(p.iter().map(|l| l.to_string()));
            }
        }
        k = end;
    }
    out.join("\n")
}

fn cols(s: &str) -> usize {
    s.chars().count()
}

fn indent(line: &str) -> usize {
    line.len() - line.trim_start_matches(' ').len()
}

/// The column a row's description starts at: after the first gap of two
/// spaces or more that stands between words.
fn gap(line: &str) -> Option<usize> {
    let body = line.trim_start_matches(' ');
    let start = body.find("  ")?;
    let rest = &body[start..];
    let desc = rest.trim_start_matches(' ');
    (!desc.is_empty()).then(|| cols(line) - cols(desc))
}

/// Does `next` carry on the paragraph `prev` ends, at its column `hang`?
/// A line that is a command, `datum …`, is one of its own.
fn continues(prev: &str, next: &str, hang: usize) -> bool {
    let first = next.split_whitespace().next().unwrap_or("");
    !first.is_empty()
        && first != "datum"
        && indent(next) == hang
        && gap(next).is_none()
        && super::render::heading(next).is_none()
        && !prev.ends_with(':')
        && cols(prev) + 1 + cols(first) > HAND
}

/// One paragraph, laid into `out` at `width`.
fn wrap(out: &mut Vec<String>, para: &[&str], col: Option<usize>, stack: bool, width: usize) {
    let head = para[0];
    let ind = indent(head).min(width / 2);
    let mut text: Vec<&str> = vec![];
    let lead = match col {
        Some(c) => {
            let at = head.char_indices().nth(c).map_or(head.len(), |(b, _)| b);
            text.push(&head[at..]);
            &head[..at]
        }
        None => {
            text.push(head.trim_start());
            ""
        }
    };
    text.extend(para[1..].iter().map(|l| l.trim_start()));
    let words = words(&text);
    let pad = |n: usize| " ".repeat(n);
    match col {
        Some(_) if stack => {
            lay(out, pad(ind), ind + 4, &words_of(lead.trim()), width);
            lay(out, pad(ind + 4), ind + 4, &words, width);
        }
        Some(c) => lay(out, lead.to_string(), c, &words, width),
        // A line of its own (a usage line) hangs its tail; a paragraph
        // keeps its indent.
        None if para.len() == 1 && ind > 0 => lay(out, pad(ind), ind + 4, &words, width),
        None => lay(out, pad(ind), ind, &words, width),
    }
}

/// `(spaces before, word)` for each word of `lines`, joined by one space;
/// a gap inside a line keeps its width, which is a column a table set.
fn words<'a>(lines: &[&'a str]) -> Vec<(usize, &'a str)> {
    let mut out = Vec::new();
    for line in lines {
        let mut spaces = usize::from(!out.is_empty());
        for (n, piece) in line.split(' ').enumerate() {
            if piece.is_empty() {
                spaces += 1;
                continue;
            }
            out.push((if n == 0 { spaces } else { spaces + 1 }, piece));
            spaces = 0;
        }
    }
    if let Some(w) = out.first_mut() {
        w.0 = 0;
    }
    out
}

fn words_of(s: &str) -> Vec<(usize, &str)> {
    words(&[s])
}

/// Lay `words` after `first`, every following line indented `hang`.
/// After a gap the whole cell up to the next gap must fit, so a column
/// moves to the next line together.
fn lay(out: &mut Vec<String>, first: String, hang: usize, words: &[(usize, &str)], width: usize) {
    let hang = hang.min(width.saturating_sub(ROOM));
    let mut line = first;
    let mut fresh = true;
    for (k, &(spaces, word)) in words.iter().enumerate() {
        let cell = words[k + 1..]
            .iter()
            .take_while(|(s, _)| spaces >= 2 && *s < 2);
        let need = cols(word) + cell.map(|(s, w)| s + cols(w)).sum::<usize>();
        if !fresh && cols(&line) + spaces + need > width {
            out.push(std::mem::replace(&mut line, " ".repeat(hang)));
            fresh = true;
        }
        if !fresh {
            line.push_str(&" ".repeat(spaces));
        }
        // Wider than a whole line: broken after a `|`, `/` or `,` where
        // one fits, else where the room ends.
        let mut rest = word;
        while cols(&line) + cols(rest) > width {
            let room = width - cols(&line);
            let cut = rest.char_indices().nth(room).map_or(rest.len(), |(b, _)| b);
            let at = rest[..cut].rfind(['|', '/', ',']).map_or(cut, |b| b + 1);
            line.push_str(&rest[..at]);
            out.push(std::mem::replace(&mut line, " ".repeat(hang)));
            rest = &rest[at..];
        }
        line.push_str(rest);
        fresh = false;
    }
    out.push(line.trim_end().to_string());
}
