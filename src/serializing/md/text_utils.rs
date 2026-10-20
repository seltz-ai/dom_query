use memchr::memchr3;

use super::constants::{ALWAYS_ESCAPED, LINE_START_ESCAPED};
use super::opts::FormatOpts;

#[allow(clippy::cast_possible_truncation)]
pub(super) fn push_normalized_text(text: &mut String, new_text: &str, f_opts: FormatOpts) {
    if !text.ends_with(['\n', ' ']) && !new_text.is_empty() && new_text.trim_ascii().is_empty() {
        text.push(' ');
        return;
    }

    let follows_newline = text.ends_with(['\n', ' ']) || text.is_empty();
    let push_start_whitespace = (f_opts.inline || !follows_newline)
        && new_text.starts_with(|c: char| c.is_ascii_whitespace());
    let push_end_whitespace = new_text.ends_with(|c: char| c.is_ascii_whitespace());

    let mut result = String::with_capacity(new_text.len());
    let mut iter = new_text.split_ascii_whitespace();

    if let Some(first) = iter.next() {
        let escape = !f_opts.skip_escape;
        if push_start_whitespace {
            result.push(' ');
        } else if escape && matches!(first, "." | ")") && ends_with_marker_digits(text) {
            // `<span>1</span>. foo`: the digits already written at the line
            // start and this `.` would form an ordered-list marker
            result.push('\\');
        }
        // only the first word of a text node can continue a Markdown line;
        // spaces before it may be trimmed later, and a line may start with some
        let line = text.trim_end_matches(' ');
        let is_line_start = line.is_empty() || line.ends_with('\n') || f_opts.list_item;
        push_escaped_chunk(&mut result, first, escape, is_line_start);
        for word in iter {
            result.push(' ');
            push_escaped_chunk(&mut result, word, escape, false);
        }
    }

    if result.is_empty() && follows_newline {
        return;
    }

    text.push_str(&result);

    if push_end_whitespace && !text.ends_with(|c: char| c.is_ascii_whitespace()) {
        text.push(' ');
    }
}

/// True when `text` ends with one to nine digits that start a line or a list
/// item's content, so a following `.` or `)` would make an ordered-list marker.
fn ends_with_marker_digits(text: &str) -> bool {
    // CommonMark ordered-list markers have one to nine digits
    let stem = text.trim_end_matches(|c: char| c.is_ascii_digit());
    if !(1..=9).contains(&(text.len() - stem.len())) {
        return false;
    }
    // in a list item, the item's own marker (`- `, `+ `, `12. `) precedes the
    // digits on this line, so set it aside and look at what comes before it
    let stem = stem.strip_suffix(' ').map_or(stem, |s| {
        let word = s.rsplit([' ', '\n']).next().unwrap_or(s);
        if matches!(word, "-" | "+") || is_ordered_list_marker(word) {
            &s[..s.len() - word.len()]
        } else {
            stem
        }
    });
    // the digits start the line when only indentation precedes them
    let indent = stem.trim_end_matches(' ');
    indent.is_empty() || indent.ends_with('\n')
}

/// An ordered-list marker like `3.` or `12)` at the beginning of a line would
/// otherwise turn the line into a list item.
fn is_ordered_list_marker(chunk: &str) -> bool {
    let Some(stem) = chunk.strip_suffix(['.', ')']) else {
        return false;
    };
    !stem.is_empty() && stem.as_bytes().iter().all(u8::is_ascii_digit)
}

pub(super) fn push_escaped_chunk(text: &mut String, chunk: &str, escape: bool, line_start: bool) {
    if !escape {
        text.push_str(chunk);
        return;
    }

    let list_marker = line_start && is_ordered_list_marker(chunk);
    let bytes = chunk.as_bytes();
    // a first word of only `=` could underline the previous line into a setext heading
    let setext_underline = line_start && bytes.iter().all(|&b| b == b'=');
    let mut last = 0;

    text.reserve(chunk.len());

    for (i, &b) in bytes.iter().enumerate() {
        let escaped = if ALWAYS_ESCAPED.contains(&b) {
            true
        } else if LINE_START_ESCAPED.contains(&b) {
            line_start && i == 0
        } else if b == b'!' {
            bytes.get(i + 1) == Some(&b'[')
        } else if b == b'.' || b == b')' {
            list_marker
        } else if b == b'=' {
            setext_underline && i == 0
        } else if b == b'&' {
            // `&ograve;` in the text would otherwise render as `ò`
            starts_entity(&chunk[i + 1..])
        } else {
            false
        };

        if escaped {
            text.push_str(&chunk[last..i]);
            text.push('\\');
            last = i;
        }
    }

    text.push_str(&chunk[last..]);
}

/// Whether `s`, following an `&`, completes an entity reference like `&amp;`.
fn starts_entity(s: &str) -> bool {
    let body = s.strip_prefix('#').unwrap_or(s);
    let len = body.find(';').unwrap_or(0);
    len > 0 && body[..len].bytes().all(|b| b.is_ascii_alphanumeric())
}

/// Escapes a trailing `!` so the `[` written right after it
/// cannot turn the text into image syntax (`Wow![x](u)`).
pub(super) fn escape_trailing_bang(text: &mut String) {
    if text.ends_with('!') {
        text.pop();
        text.push_str("\\!");
    }
}

pub(super) fn trim_trailing_space(s: &mut String) {
    s.truncate(s.trim_end_matches(' ').len());
}

/// A run of `#` that ends a heading after a space would be dropped as the
/// ATX closing sequence.
pub(super) fn escape_closing_hashes(text: &mut String) {
    let content = text.trim_end();
    let stem = content.trim_end_matches('#');
    if stem.len() < content.len() && stem.ends_with(' ') {
        text.insert(stem.len(), '\\');
    }
}

pub(super) fn add_linebreaks(text: &mut String, linebreak: &str, end: &str) {
    trim_trailing_space(text);
    while !text.ends_with(end) {
        text.push_str(linebreak);
    }
}

/// Keep only the first whitespace‑delimited token and a conservative set of characters.
pub(super) fn sanitize_attr_value(raw: &str) -> String {
    let token = raw.split_ascii_whitespace().next().unwrap_or("");
    token
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '+' | '.' | '#'))
        .collect()
}

/// An emphasis whose closing marker ends the output.
pub(super) struct Closed {
    /// Where its opening marker starts.
    pub open: usize,
    /// Bytes of edge punctuation right before its closing marker.
    pub tail: usize,
    /// Whether it continues the emphasis before it, with no marker of its own.
    pub merged: bool,
    /// Whether a letter after its closing marker would keep it from closing,
    /// with no punctuation to move out: its markers are removed then.
    pub removable: bool,
}

/// Writes `inner` between `marker`s so `CommonMark` reads it as emphasis:
/// whitespace and line breaks at its edges go outside the markers, and so does
/// punctuation where the markers could not delimit it (`a**(b)**` is not
/// emphasis, `a(**b**)` is). Content spanning blocks keeps its text only.
///
/// Returns the emphasis when the output ends with its closing marker, so edge
/// punctuation kept before it can be moved out if a letter follows.
pub(super) fn push_emphasis(acc: &mut String, inner: &str, marker: &str) -> Option<Closed> {
    if marker.is_empty() || inner.trim().is_empty() {
        acc.push_str(inner);
        return None;
    }
    let content = trim_breaks(inner);
    let start = content.as_ptr() as usize - inner.as_ptr() as usize;
    let lead = &inner[..start];
    let trail = &inner[start + content.len()..];
    // emphasis holds hard line breaks (`  \n`) within a paragraph; any other
    // break, or a blank line at an edge, means it spans blocks
    let hard_breaks_only = content
        .split('\n')
        .rev()
        .skip(1)
        .all(|line| line.ends_with("  ") && !line.trim().is_empty());
    if !hard_breaks_only
        || content.contains("```")
        || lead.contains("\n\n")
        || trail.contains("\n\n")
    {
        let raw = if acc.is_empty() || acc.ends_with("\n\n") {
            inner.trim_start_matches(['\n', ' '])
        } else {
            inner
        };
        acc.push_str(raw);
        return None;
    }
    if lead.contains('\n') && !acc.is_empty() && !acc.ends_with('\n') {
        // a hard break stays a hard break, before the marker
        trim_trailing_space(acc);
        acc.push_str("  \n");
    } else if lead.contains("<br>") {
        acc.push_str(lead.trim());
    } else if !lead.is_empty() && !acc.ends_with(char::is_whitespace) {
        // passed on even at the start: an emphasis around this one moves it out
        acc.push(' ');
    }
    let code_or_link = has_unescaped(content, '`') || content.contains("](");
    let (head, body, tail) = split_edge_punctuation(content);
    if body.is_empty() && !code_or_link {
        return push_punctuation_emphasis(acc, head, marker, lead.is_empty(), trail.is_empty());
    }
    let merged = try_trim_prev_emphasis_marker(acc, marker);
    if merged && content.starts_with('[') {
        // emphasis merged into the one before it adds no marker before the link
        escape_trailing_bang(acc);
    }
    // an opening marker followed by punctuation needs whitespace or
    // punctuation before it; a closing one preceded by punctuation needs the
    // same after it, which is not written yet
    let after_space_or_punct = acc
        .chars()
        .next_back()
        .is_none_or(is_whitespace_or_punctuation);
    let mut closed;
    if code_or_link {
        // a code span or link cannot move its edge punctuation out: without
        // room for the opening marker it is not emphasized, and the caller
        // removes the markers if a letter follows the closing one
        if !merged && !after_space_or_punct && content.starts_with(is_whitespace_or_punctuation) {
            acc.push_str(content);
            if !trail.is_empty() {
                acc.push(' ');
            }
            return None;
        }
        let open = acc.len();
        if !merged {
            acc.push_str(marker);
        }
        acc.push_str(content);
        acc.push_str(marker);
        closed = Some(Closed {
            open,
            tail: 0,
            merged,
            removable: !merged && content.ends_with(is_whitespace_or_punctuation),
        });
    } else {
        // `tail` stays inside, and the caller moves it out if a letter follows
        let keep_head = merged || after_space_or_punct;
        if !keep_head {
            acc.push_str(head);
        }
        let open = acc.len();
        if !merged {
            acc.push_str(marker);
        }
        if keep_head {
            acc.push_str(head);
        }
        acc.push_str(body);
        acc.push_str(tail);
        acc.push_str(marker);
        closed = Some(Closed {
            open,
            tail: tail.len(),
            merged,
            removable: false,
        });
    }
    if !trail.is_empty() {
        // a hard break stays a hard break, after the marker
        if trail.contains('\n') || trail.contains("<br>") {
            acc.push_str(trail);
        } else {
            acc.push(' ');
        }
        closed = None;
    }
    closed
}

/// Writes emphasized punctuation, which has nothing to emphasize by itself,
/// unless it continues an emphasis ending right before it, as in
/// `<b>a</b><b>:</b>`.
fn push_punctuation_emphasis(
    acc: &mut String,
    punct: &str,
    marker: &str,
    touching: bool,
    closes_output: bool,
) -> Option<Closed> {
    let merges = touching && is_exact_marker_end(acc, marker);
    let open = acc.len().saturating_sub(marker.len());
    if merges {
        acc.truncate(open);
    } else if punct.starts_with(['.', ')']) && ends_with_marker_digits(acc) {
        // without markers, it would follow line-start digits as a list marker
        acc.push('\\');
    }
    acc.push_str(punct);
    if merges {
        acc.push_str(marker);
        if closes_output {
            return Some(Closed {
                open,
                tail: punct.len(),
                merged: true,
                removable: false,
            });
        }
    }
    if !closes_output {
        acc.push(' ');
    }
    None
}

/// `s` without the whitespace and `<br>` line breaks at its edges.
fn trim_breaks(s: &str) -> &str {
    let mut s = s.trim();
    while let Some(t) = s.strip_prefix("<br>").or_else(|| s.strip_suffix("<br>")) {
        s = t.trim();
    }
    s
}

/// Whether `CommonMark` reads `c` as whitespace or punctuation next to an
/// emphasis marker. Unicode punctuation is that of the common blocks: anything
/// else, such as a zero-width joiner, counts as neither, the safe reading.
pub(super) fn is_whitespace_or_punctuation(c: char) -> bool {
    c.is_whitespace()
        || c.is_ascii_punctuation()
        || matches!(c,
            '\u{a1}'..='\u{a9}' | '\u{ab}' | '\u{ac}' | '\u{ae}'..='\u{b1}' | '\u{b4}'
            | '\u{b6}'..='\u{b8}' | '\u{bb}' | '\u{bf}' | '\u{d7}' | '\u{f7}'
            | '\u{5be}' | '\u{5c0}' | '\u{5c3}' | '\u{5f3}' | '\u{5f4}'
            | '\u{60c}' | '\u{61b}' | '\u{61f}' | '\u{66a}'..='\u{66d}' | '\u{6d4}'
            | '\u{964}' | '\u{965}'
            | '\u{2010}'..='\u{2027}' | '\u{2030}'..='\u{205e}' | '\u{20a0}'..='\u{20c0}'
            | '\u{2190}'..='\u{23ff}' | '\u{2500}'..='\u{2775}' | '\u{2794}'..='\u{27bf}'
            | '\u{3001}'..='\u{3004}' | '\u{3008}'..='\u{3020}' | '\u{3030}' | '\u{3036}'
            | '\u{3037}' | '\u{303d}'..='\u{303f}' | '\u{ff01}'..='\u{ff0f}'
            | '\u{ff1a}'..='\u{ff20}' | '\u{ff3b}'..='\u{ff40}' | '\u{ff5b}'..='\u{ff65}')
}

/// Moves the `tail` bytes of punctuation before the `run` bytes of closing
/// markers that end `text` after them, for a letter that follows them:
/// `**Note:**e` is not emphasis, `**Note**:e` is.
pub(super) fn move_tail_out(text: &mut String, run: usize, tail: usize) {
    let end = text.len();
    let moved = format!(
        "{}{}",
        &text[end - run..],
        &text[end - run - tail..end - run]
    );
    text.truncate(end - run - tail);
    text.push_str(&moved);
}

/// Removes the markers of the emphasis opened at `open` with `marker` that
/// ends `text`.
pub(super) fn remove_emphasis(text: &mut String, open: usize, marker: &str) {
    let inner = text[open + marker.len()..text.len() - marker.len()].to_string();
    text.truncate(open);
    text.push_str(&inner);
}

fn has_unescaped(s: &str, needle: char) -> bool {
    let mut escaped = false;
    for c in s.chars() {
        if c == needle && !escaped {
            return true;
        }
        escaped = c == '\\' && !escaped;
    }
    false
}

fn is_edge_char(c: char) -> bool {
    // unescaped `*` and `_` at an edge belong to a nested emphasis marker
    c != '*' && c != '_' && !c.is_alphanumeric()
}

/// Splits `s` into its leading run of punctuation and whitespace, the rest,
/// and its trailing run, keeping each backslash escape with what it escapes.
fn split_edge_punctuation(s: &str) -> (&str, &str, &str) {
    let mut head = 0;
    let mut chars = s.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some((j, e)) if e.is_ascii_punctuation() => head = j + e.len_utf8(),
                _ => break,
            }
        } else if is_edge_char(c) {
            head = i + c.len_utf8();
        } else {
            break;
        }
    }
    let rest = &s[head..];
    let mut tail = rest.len();
    for (i, c) in rest.char_indices().rev() {
        let escaped = c.is_ascii_punctuation() && rest[..i].ends_with('\\');
        if !is_edge_char(c) && !escaped {
            break;
        }
        tail = i;
    }
    (&s[..head], &rest[..tail], &rest[tail..])
}

#[allow(clippy::cast_possible_truncation)]
pub(super) fn try_trim_prev_emphasis_marker(acc: &mut String, marker: &str) -> bool {
    if marker.is_empty() || acc.is_empty() {
        return false;
    }

    let has_space = acc.ends_with(' ');
    let trimmed = acc.trim_ascii_end();
    let trailing = &acc[trimmed.len()..];

    if (trailing.is_empty() || trailing == " ") && is_exact_marker_end(trimmed, marker) {
        let trim_bytes = marker.len() + usize::from(has_space);
        acc.truncate(acc.len() - trim_bytes);

        if has_space {
            acc.push(' ');
        }
        true
    } else {
        false
    }
}

#[inline]
pub(super) fn is_exact_marker_end(tail: &str, marker: &str) -> bool {
    // `_` markers are never merged, as they are written only to stand apart
    let Some(prefix) = tail.strip_suffix(marker) else {
        return false;
    };
    // part of a longer run, as `**` of `***`, whose pairing would change
    if prefix.ends_with('*') {
        return false;
    }
    !prefix.bytes().rev().take_while(|&b| b == b'\\').count() % 2 != 0
}

/// Formats a Markdown link destination and writes it directly to the buffer.
pub(super) fn push_md_url(text: &mut String, dest: &str) {
    let mut balance: i32 = 0;
    let mut needs_wrap = false;
    let mut has_backslash = false;

    let dest = &html_escape(dest);

    for &b in dest.as_bytes() {
        match b {
            b'(' => balance += 1,
            b')' => {
                balance -= 1;
                if balance < 0 {
                    needs_wrap = true;
                    break;
                }
            }
            b' ' | b'<' | 0..=31 | 127 => {
                needs_wrap = true;
                break;
            }
            b'\\' => has_backslash = true,
            _ => {}
        }
    }

    if needs_wrap || balance != 0 {
        text.reserve(dest.len() + 4);
        text.push('<');
        for c in dest.chars() {
            match c {
                '\\' => text.push_str("\\\\"),
                '<' => text.push_str("\\<"),
                '>' => text.push_str("\\>"),
                '\n' => text.push_str("%0A"),
                '\r' => text.push_str("%0D"),
                c => text.push(c),
            }
        }
        text.push('>');
    } else if has_backslash {
        text.push_str(&dest.replace('\\', "\\\\"));
    } else {
        text.push_str(dest);
    }
}

pub(super) fn push_title(text: &mut String, title: &str) {
    text.push_str(" \"");
    if title.contains('"') {
        let mut normalized = String::with_capacity(title.len());
        push_normalized_text(&mut normalized, title, FormatOpts::new());
        text.push_str(&normalized.replace('"', "\\\""));
    } else {
        push_normalized_text(text, title, FormatOpts::new());
    }
    text.push('"');
}

pub(super) fn max_backtick_run(text: &str) -> usize {
    let mut max = 0;
    let mut current = 0;
    for b in text.as_bytes() {
        current = if *b == b'`' { current + 1 } else { 0 };
        max = max.max(current);
    }
    max
}

pub(super) fn push_code_text(text: &mut String, code_text: &str) {
    if code_text.is_empty() {
        return;
    }

    // a code span right before would merge its closing run with this
    // opening one: `x` and `y` would give `x``y`, a single span
    if text.ends_with('`') {
        text.push(' ');
    }

    let fence_len = max_backtick_run(code_text) + 1;
    let bytes = code_text.as_bytes();
    // a backtick at either edge would merge into the fence, and a parser
    // strips one space from each side of content like ` a `
    let needs_space = bytes.first() == Some(&b'`')
        || bytes.last() == Some(&b'`')
        || (bytes.first() == Some(&b' ')
            && bytes.last() == Some(&b' ')
            && bytes.iter().any(|&b| b != b' '));

    text.extend(std::iter::repeat_n('`', fence_len));
    if needs_space {
        text.push(' ');
    }
    text.push_str(code_text);
    if needs_space {
        text.push(' ');
    }
    text.extend(std::iter::repeat_n('`', fence_len));
}

pub(super) fn trim_space(s: &mut String) {
    s.truncate(s.trim_ascii_end().len());
    s.drain(..s.len() - s.trim_ascii_start().len());
}

pub(super) fn escape_table_cell_inplace(content: &mut String) {
    if content.is_empty() || !content.contains('|') {
        return;
    }

    let bytes = content.as_bytes();
    let mut result = String::with_capacity(content.len() + 4);
    let mut last = 0;

    for i in 0..bytes.len() {
        if bytes[i] == b'|' {
            // Check if '|' is preceded by an unescaped backslash
            let is_escaped = i > 0 && bytes[i - 1] == b'\\';
            if !is_escaped {
                // Append preceding slice and insert escaping backslash
                result.push_str(&content[last..i]);
                result.push('\\');
                // Set 'last' to 'i' so '|' itself will be included in the next chunk slice
                last = i;
            }
        }
    }
    // If all pipes were already escaped, avoid extra string assignment
    if result.is_empty() {
        return;
    }
    // Append the remaining tail of the string
    result.push_str(&content[last..]);
    *content = result;
}

pub(super) const fn linebreak(br: bool) -> &'static str {
    if br { "<br>" } else { "\n" }
}

/// Escapes special characters for HTML attribute values (quotes, ampersands, and &nbsp;).
/// Note: `<` and `>` are intentionally skipped here.
pub fn html_escape(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut result = String::with_capacity(text.len());
    let mut search_start = 0;

    while search_start < bytes.len() {
        let next_special = if let Some(i) = memchr3(b'"', b'&', 0xC2, &bytes[search_start..]) {
            search_start + i
        } else {
            result.push_str(&text[search_start..]);
            break;
        };

        result.push_str(&text[search_start..next_special]);

        let (replacement, advance) = match bytes[next_special] {
            b'&' => ("&amp;", 1),
            b'"' => ("&quot;", 1),
            0xC2 if bytes.get(next_special + 1) == Some(&0xA0) => ("&nbsp;", 2),
            _ => {
                result.push_str(&text[next_special..next_special + 2]);
                search_start = next_special + 2;
                continue;
            }
        };

        result.push_str(replacement);
        search_start = next_special + advance;
    }
    result
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_escape_text() {
        let t = r"Some text: x `y` *z* _w_ [v] <u> #h >q -l +m !i . .5 5. |q|";
        let mut text = String::new();
        // mid-line: only characters with Markdown meaning anywhere are escaped
        push_normalized_text(&mut text, t, FormatOpts::new());
        assert_eq!(
            text,
            r"Some text: x \`y\` \*z\* \_w\_ \[v\] \<u> #h >q -l +m !i . .5 5. \|q\|"
        );

        // at the beginning of a line, block-starting characters are escaped too
        let mut text = String::new();
        for word in ["#h", ">q", "-l", "+m", "2024.", "5)", "."] {
            push_escaped_chunk(&mut text, word, true, true);
            text.push(' ');
        }
        text.pop();
        assert_eq!(text, r"\#h \>q \-l \+m 2024\. 5\) .");

        // escape: false is used for inline code content: only backticks are
        // escaped so they cannot terminate the code span
        let mut text = String::new();
        push_normalized_text(&mut text, t, FormatOpts::new().skip_escape());
        assert_eq!(
            text,
            r"Some text: x `y` *z* _w_ [v] <u> #h >q -l +m !i . .5 5. |q|"
        );
    }

    #[test]
    fn test_html_escape() {
        struct TestCase {
            name: &'static str,
            input: &'static str,
            expected: &'static str,
        }

        let test_cases = [
            // Empty string
            TestCase {
                name: "empty string",
                input: "",
                expected: "",
            },
            // No special characters
            TestCase {
                name: "no special chars",
                input: "hello_world_123",
                expected: "hello_world_123",
            },
            // Angle brackets untouched
            TestCase {
                name: "angle brackets untouched",
                input: "a < b and c > d",
                expected: "a < b and c > d",
            },
            // Quotes escaped
            TestCase {
                name: "quotes escaped",
                input: "Width: \"100%\"",
                expected: "Width: &quot;100%&quot;",
            },
            // Ampersand escaped
            TestCase {
                name: "ampersand escaped",
                input: "Tom & Jerry",
                expected: "Tom &amp; Jerry",
            },
            // Non-breaking space converted
            TestCase {
                name: "non-breaking space",
                input: "Hello\u{00A0}World",
                expected: "Hello&nbsp;World",
            },
            // Mixed attribute content
            TestCase {
                name: "mixed attribute content",
                input: "class=\"btn & primary\" title=\"1 < 2 & 3 > 2\"",
                expected: "class=&quot;btn &amp; primary&quot; title=&quot;1 < 2 &amp; 3 > 2&quot;",
            },
            // C2 byte without nbsp
            TestCase {
                name: "c2 byte without nbsp",
                input: "Test\u{0080}Value",
                expected: "Test\u{0080}Value",
            },
            TestCase {
                name: "amp in url",
                input: "/?q=&copy;",
                expected: "/?q=&amp;copy;",
            },
            TestCase {
                name: "amp in url",
                input: "/?a=1&b=2",
                expected: "/?a=1&amp;b=2",
            },
        ];

        for tc in test_cases {
            let actual = html_escape(tc.input);
            assert_eq!(
                actual, tc.expected,
                "Failed test case: '{}' (input: {:?})",
                tc.name, tc.input
            );
        }
    }
}
