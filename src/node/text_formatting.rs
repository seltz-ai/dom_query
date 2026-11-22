use std::cell::Ref;

use html5ever::{QualName, local_name};

use crate::NodeId;

use super::SerializeOp;
use super::{NodeData, NodeRef, TreeNode, child_nodes};

pub(crate) fn format_text(root_node: &NodeRef, include_node: bool) -> String {
    let id = root_node.id;
    let nodes = root_node.tree.nodes.borrow();
    let mut ops = if include_node {
        vec![SerializeOp::Open(id)]
    } else {
        child_nodes(Ref::clone(&nodes), &id, true)
            .map(SerializeOp::Open)
            .collect()
    };

    let mut text = String::new();

    while let Some(op) = ops.pop() {
        match op {
            SerializeOp::Open(id) => {
                let Some(node) = nodes.get(id.value()) else {
                    continue;
                };

                match node.data {
                    NodeData::Text { ref contents } => {
                        push_normalized_text(&mut text, contents.as_ref());
                    }
                    NodeData::Element(ref e) => {
                        // not drawn, as in Markdown
                        if matches!(
                            e.name.local,
                            local_name!("script")
                                | local_name!("style")
                                | local_name!("head")
                                | local_name!("title")
                                | local_name!("svg")
                        ) {
                            continue;
                        }
                        if !(text.is_empty() || text.ends_with("\n\n"))
                            && elem_require_linebreak(&e.name)
                        {
                            text.push('\n');
                        } else if e.name.local == local_name!("li")
                            && !(text.is_empty() || text.ends_with('\n'))
                        {
                            // an item starts its own line, even after inline text
                            trim_trailing_space(&mut text);
                            text.push('\n');
                        }

                        ops.push(SerializeOp::Close(&e.name));

                        if matches!(e.name.local, local_name!("pre")) {
                            text.push_str(&pre_text(Ref::clone(&nodes), id));
                            continue;
                        }

                        let mut children: Vec<_> =
                            child_nodes(Ref::clone(&nodes), &id, false).collect();
                        if matches!(e.name.local, local_name!("table")) {
                            // a caption is drawn above its table wherever it sits in the markup
                            children.sort_by_key(|c| {
                                nodes[c.value()]
                                    .as_element()
                                    .is_none_or(|ce| ce.name.local != local_name!("caption"))
                            });
                        }
                        ops.extend(children.into_iter().rev().map(SerializeOp::Open));
                    }
                    _ => {}
                }
            }
            SerializeOp::Close(name) => {
                adjust_element_offset(&mut text, name);
            }
        }
    }
    if !include_node {
        text.truncate(text.trim_end().len());
    }
    text
}

/// The text of a `<pre>` as written, with a line break for each `<br>` and
/// around each block inside it, as browsers draw them.
pub(crate) fn pre_text(nodes: Ref<Vec<TreeNode>>, id: NodeId) -> String {
    let mut text = String::new();
    let mut ops: Vec<SerializeOp> = child_nodes(Ref::clone(&nodes), &id, true)
        .map(SerializeOp::Open)
        .collect();
    // a block ended: a line break goes before whatever follows it
    let mut after_block = false;
    while let Some(op) = ops.pop() {
        match op {
            SerializeOp::Open(id) => match nodes[id.value()].data {
                NodeData::Text { ref contents } => {
                    if after_block && !contents.is_empty() && !text.ends_with('\n') {
                        text.push('\n');
                    }
                    after_block = after_block && contents.is_empty();
                    text.push_str(contents);
                }
                NodeData::Element(ref e) if e.name.local == local_name!("br") => {
                    text.push('\n');
                    after_block = false;
                }
                NodeData::Element(ref e) => {
                    if is_pre_block(&e.name) && !text.is_empty() && !text.ends_with('\n') {
                        text.push('\n');
                    }
                    ops.push(SerializeOp::Close(&e.name));
                    ops.extend(child_nodes(Ref::clone(&nodes), &id, true).map(SerializeOp::Open));
                }
                _ => {}
            },
            SerializeOp::Close(name) => after_block |= is_pre_block(name),
        }
    }
    text
}

fn is_pre_block(name: &QualName) -> bool {
    elem_require_linebreak(name) || matches!(name.local, local_name!("li") | local_name!("tr"))
}

fn push_normalized_text(text: &mut String, new_text: &str) {
    let follows_newline = text.ends_with(['\n', ' ']) || text.is_empty();
    let push_start_whitespace = !follows_newline && new_text.starts_with(char::is_whitespace);
    let push_end_whitespace = new_text.ends_with(char::is_whitespace);

    let mut result = String::with_capacity(new_text.len());
    let mut iter = new_text.split_whitespace();

    if let Some(first) = iter.next() {
        if push_start_whitespace {
            result.push(' ');
        }
        result.push_str(first);
        for word in iter {
            result.push(' ');
            result.push_str(word);
        }
    }
    if result.is_empty() && follows_newline {
        return;
    }

    text.push_str(&result);

    if push_end_whitespace && !text.ends_with(char::is_whitespace) {
        text.push(' ');
    }
}

fn trim_trailing_space(s: &mut String) {
    s.truncate(s.trim_end_matches(' ').len());
}

fn adjust_element_offset(text: &mut String, name: &QualName) {
    if text.is_empty() || text.ends_with("\n\n") {
        return;
    }

    if elem_require_linebreak(name) {
        // one blank line, whatever line break the last item or row left
        trim_trailing_space(text);
        while !text.ends_with("\n\n") {
            text.push('\n');
        }
    } else if matches!(
        name.local,
        local_name!("br") | local_name!("hr") | local_name!("li") | local_name!("tr")
    ) {
        trim_trailing_space(text);
        text.push('\n');
    } else if matches!(name.local, local_name!("td") | local_name!("th"))
        && !text.ends_with(['\n', ' '])
    {
        text.push(' ');
    }
}

const fn elem_require_linebreak(name: &QualName) -> bool {
    matches!(
        name.local,
        local_name!("article")
            | local_name!("blockquote")
            | local_name!("section")
            | local_name!("div")
            | local_name!("p")
            | local_name!("pre")
            | local_name!("h1")
            | local_name!("h2")
            | local_name!("h3")
            | local_name!("h4")
            | local_name!("h5")
            | local_name!("h6")
            | local_name!("ul")
            | local_name!("ol")
            | local_name!("dl")
            | local_name!("dt")
            | local_name!("dd")
            | local_name!("caption")
            | local_name!("header")
            | local_name!("footer")
            | local_name!("main")
            | local_name!("nav")
            | local_name!("aside")
            | local_name!("figure")
            | local_name!("figcaption")
            | local_name!("address")
            | local_name!("details")
            | local_name!("summary")
            | local_name!("fieldset")
            | local_name!("legend")
            | local_name!("hgroup")
            | local_name!("form")
            | local_name!("table")
    )
}

#[cfg(test)]
mod tests {
    use crate::Document;

    #[test]
    fn test_trim_end_multi_byte() {
        //&nbsp;
        let doc = Document::fragment("<pre>a&nbsp;\n</pre>");
        assert_eq!(&doc.formatted_text(), "a");
        let doc = Document::fragment("<pre>a\u{3000}</pre>");
        assert_eq!(&doc.formatted_text(), "a");
    }

    // ---- seltz cases ----
    #[test]
    fn ours_test_formatted_title_and_scripts_skipped() {
        let doc =
            Document::from("<div><title>Hidden</title><script>var a;</script><p>Body</p></div>");
        assert_eq!(doc.root().formatted_text().as_ref(), "Body");
    }

    #[test]
    fn ours_test_formatted_pre_breaks_and_blocks_kept() {
        let doc =
            Document::from("<pre><p>one.</p><p>two</p>tail<br>a</pre><svg><text>0</text></svg>");
        assert_eq!(doc.root().formatted_text().as_ref(), "one.\ntwo\ntail\na");
    }

    #[test]
    fn ours_test_formatted_one_blank_line_after_list_and_table() {
        let doc = Document::from(
            "<ul><li>a</li><li>b</li></ul><p>after list</p><table><tr><td>x</td></tr></table><p>after table</p>",
        );
        assert_eq!(
            doc.root().formatted_text().as_ref(),
            "a\nb\n\nafter list\n\nx\n\nafter table"
        );
    }

    #[test]
    fn ours_test_formatted_definition_list_items_separated() {
        let doc = Document::from(
            "<dl><dt>Sun, 20 Sep 2026</dt><dd><span>8:00 AM</span> Holy Eucharist</dd><dd>10:00 AM Mass</dd></dl>",
        );
        assert_eq!(
            doc.root().formatted_text().as_ref(),
            "Sun, 20 Sep 2026\n\n8:00 AM Holy Eucharist\n\n10:00 AM Mass"
        );
    }

    #[test]
    fn ours_test_formatted_caption_first() {
        let doc = Document::from(
            "<table><tbody><tr><td>2021</td><td>17th</td></tr></tbody><caption>Recent finishes</caption></table><p>After</p>",
        );
        assert_eq!(
            doc.root().formatted_text().as_ref(),
            "Recent finishes\n\n2021 17th\n\nAfter"
        );
    }

    #[test]
    fn ours_test_formatted_item_starts_line() {
        let doc = Document::from("<div>|<li>Contact us</li>|<li>Cookie policy</li></div>");
        assert_eq!(
            doc.root().formatted_text().as_ref(),
            "|\nContact us\n|\nCookie policy"
        );
    }

    #[test]
    fn ours_test_formatted_html5_blocks_separated() {
        let doc = Document::from("<header>Top</header><main>Body</main><footer>Bottom</footer>");
        assert_eq!(
            doc.root().formatted_text().as_ref(),
            "Top\n\nBody\n\nBottom"
        );
    }

    #[test]
    fn ours_test_formatted_multibyte_whitespace_trimmed() {
        let doc = Document::from("<pre>a　</pre>");
        assert_eq!(doc.root().formatted_text().as_ref(), "a");
    }
}
