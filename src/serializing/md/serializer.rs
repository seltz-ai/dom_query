use std::cell::Ref;

use html5ever::{QualName, local_name};
use tendril::StrTendril;

use crate::{Element, NodeId, TreeNodeOps};

use crate::node::{NodeData, NodeRef, ancestor_nodes, child_nodes, descendant_nodes, pre_text};
use crate::node::{SerializeOp, TreeNode};

use super::constants::{
    CODE_LANGUAGE_ATTRIBUTES, CODE_LANGUAGE_PREFIX, DEFAULT_SKIP_TAGS, MAX_LIST_NUMBER,
};

use super::text_utils::{
    add_linebreaks, escape_closing_hashes, escape_table_cell_inplace, escape_trailing_bang,
    is_exact_marker_end, is_whitespace_or_punctuation, linebreak, max_backtick_run, move_tail_out,
    push_code_text, push_emphasis, push_md_url, push_normalized_text, push_title, remove_emphasis,
    sanitize_attr_value, trim_space, trim_trailing_space,
};

use super::opts::{EmphasisScope, FormatOpts, ListContext, ListKind};

pub struct MDSerializer<'a> {
    root_node: &'a NodeRef<'a>,
    nodes: Ref<'a, Vec<TreeNode>>,
    skip_tags: &'a [&'a str],
}

impl<'a> MDSerializer<'a> {
    pub fn new(root_node: &'a NodeRef, skip_tags: Option<&'a [&'a str]>) -> Self {
        let skip_tags = skip_tags.unwrap_or(&DEFAULT_SKIP_TAGS);
        let nodes = root_node.tree.nodes.borrow();
        MDSerializer {
            root_node,
            nodes,
            skip_tags,
        }
    }

    pub fn serialize(&self, include_node: bool) -> StrTendril {
        let mut text = String::new();
        let opts = FormatOpts {
            include_node,
            ..Default::default()
        };
        self.write(&mut text, self.root_node.id, opts);
        text.into()
    }

    fn write(&self, text: &mut String, root_id: NodeId, opts: FormatOpts) {
        let ops = if opts.include_node {
            vec![SerializeOp::Open(root_id)]
        } else {
            child_nodes(Ref::clone(&self.nodes), &root_id, true)
                .map(SerializeOp::Open)
                .collect()
        };
        self.write_ops(text, ops, opts, !opts.include_node);
    }

    /// Writes the nodes of `ops`, last first, trimming the output's edges if
    /// `trim`. Returns the emphasis closing run that ends the output, if a
    /// letter written after it would need its punctuation moved out.
    fn write_ops<'b>(
        &'b self,
        text: &mut String,
        mut ops: Vec<SerializeOp<'b>>,
        opts: FormatOpts,
        trim: bool,
    ) -> Option<PendingClose> {
        let linebreak = linebreak(opts.table_cell);
        let double_br = linebreak.repeat(2);
        let mut pending: Option<PendingClose> = None;

        while let Some(op) = ops.pop() {
            match op {
                SerializeOp::Open(id) => {
                    let node = &self.nodes[id.value()];
                    match &node.data {
                        NodeData::Text { contents } => {
                            if let Some(c) = contents.chars().next() {
                                if let Some(p) = pending.take().filter(|p| p.end == text.len()) {
                                    if !is_whitespace_or_punctuation(c) {
                                        p.before_letter(text);
                                    }
                                }
                            }
                            push_normalized_text(text, contents, opts);
                        }
                        NodeData::Element(e) => {
                            if self.skip_tags.contains(&e.name.local.as_ref()) {
                                continue;
                            }
                            if md_emphasis(&e.name).is_none() {
                                if let Some(p) = pending.take().filter(|p| p.end == text.len()) {
                                    // an inline element's text continues the line after the run
                                    let next = self.first_char(id);
                                    if !is_md_block(&e.name)
                                        && next.is_some_and(|c| !is_whitespace_or_punctuation(c))
                                    {
                                        p.before_letter(text);
                                    }
                                }
                            }

                            if opts.single_line
                                && (is_md_block(&e.name) || is_line_element(&e.name))
                            {
                                push_space(text);
                            } else if e.name.local == local_name!("li")
                                && !opts.skip_md
                                && !opts.table_cell
                            {
                                self.write_orphan_item(text, id, opts);
                                continue;
                            }
                            // linebreaks are disabled for md blocks if md is skipped (for code);
                            // a table cell joins its blocks with `<br>`
                            if !opts.skip_md && !opts.single_line {
                                if is_md_block(&e.name) {
                                    if opts.table_cell {
                                        add_cell_block_break(text, linebreak);
                                    } else {
                                        let node = NodeRef::new(id, self.root_node.tree);
                                        // a blank line before a nested list would make the parent list loose
                                        let tight = opts.list_item
                                            && is_list(&e.name)
                                            && can_interrupt_paragraph(&node);
                                        let end = if tight { linebreak } else { &double_br };
                                        add_linebreaks(text, linebreak, end);
                                    }
                                }
                                // push md prefixes only when md mode is active; a cell holds no blocks
                                if let Some(prefix) =
                                    md_prefix(&e.name).filter(|_| !opts.table_cell)
                                {
                                    text.push_str(prefix);
                                }
                                if self.write_text_block(text, id, &e.name, opts) {
                                    ops.push(SerializeOp::Close(&e.name));
                                    continue;
                                }
                            }
                            if !opts.skip_md && md_emphasis(&e.name).is_some() {
                                pending = self.write_emphasis(text, node, opts, pending.take());
                                continue;
                            }
                            // delegate serialization if the element has its own custom handler
                            if !opts.skip_md && self.write_element(text, e, node, opts) {
                                continue;
                            }
                            ops.push(SerializeOp::Close(&e.name));
                            ops.extend(
                                child_nodes(Ref::clone(&self.nodes), &id, true)
                                    .map(SerializeOp::Open),
                            );
                        }
                        _ => {}
                    }
                }
                SerializeOp::Close(name) => close_element(text, name, opts, linebreak, &double_br),
            }
        }

        if trim {
            trim_space(text);
        }
        pending.filter(|p| p.end == text.len())
    }

    fn write_text(&self, text: &mut String, root_id: NodeId, opts: FormatOpts) {
        let mut ops = if opts.include_node {
            vec![root_id]
        } else {
            child_nodes(Ref::clone(&self.nodes), &root_id, true).collect()
        };

        while let Some(id) = ops.pop() {
            let node = &self.nodes[id.value()];
            if let NodeData::Text { contents } = &node.data {
                push_normalized_text(text, contents, opts);
            } else if let NodeData::Element(_e) = &node.data {
                ops.extend(child_nodes(Ref::clone(&self.nodes), &id, true));
            }
        }
    }

    fn write_element(
        &self,
        text: &mut String,
        e: &Element,
        tree_node: &TreeNode,
        opts: FormatOpts,
    ) -> bool {
        let mut matched = true;

        match e.name.local {
            // written inline, their blocks on one line
            local_name!("ol")
            | local_name!("ul")
            | local_name!("blockquote")
            | local_name!("table")
                if opts.single_line =>
            {
                return false;
            }
            local_name!("blockquote") if opts.table_cell => return false,
            local_name!("ol") | local_name!("ul") => self.write_list(text, tree_node, e, opts),
            // without href, its contents are written as those of any inline element
            local_name!("a") if e.has_attr("href") => self.write_link(text, tree_node),
            local_name!("img") => Self::write_img(text, tree_node),
            // a cell holds one line: a code block becomes code spans
            local_name!("pre") if opts.single_line => self.write_line_code(text, tree_node),
            local_name!("pre") if opts.table_cell => {
                add_cell_block_break(text, "<br>");
                self.write_cell_code(text, tree_node);
                add_cell_block_break(text, "<br>");
            }
            local_name!("pre") => self.write_pre(text, tree_node),
            local_name!("blockquote") => self.write_blockquote(text, tree_node),
            local_name!("table") => self.write_table(text, tree_node),
            local_name!("code") => self.write_code(text, tree_node, opts),
            _ => matched = false,
        }
        matched
    }

    /// An item outside a list is still drawn as one, set apart so that it neither
    /// continues nor underlines a paragraph.
    fn write_orphan_item(&self, text: &mut String, id: NodeId, opts: FormatOpts) {
        if !text.is_empty() {
            add_linebreaks(text, "\n", "\n\n");
        }
        text.push_str("- ");
        self.write(
            text,
            id,
            FormatOpts {
                include_node: false,
                ..opts.list_item()
            },
        );
        add_linebreaks(text, "\n", "\n\n");
    }

    /// Writes the content of a heading, on one line, or of a text block inside
    /// emphasis that spans blocks, which applies it. False for any other element.
    fn write_text_block(
        &self,
        text: &mut String,
        id: NodeId,
        name: &QualName,
        opts: FormatOpts,
    ) -> bool {
        if opts.table_cell {
            return false;
        }
        if is_heading(name) {
            // blocks and breaks inside a heading become spaces
            let mut line = String::new();
            self.write(
                &mut line,
                id,
                FormatOpts {
                    include_node: false,
                    ..opts.single_line().applying_block_emphasis()
                },
            );
            if line.is_empty() {
                // nothing to show: no heading at all
                text.truncate(text.trim_end_matches(['#', ' ']).len());
            }
            push_emphasis(text, &line, opts.block_emphasis.markers());
            return true;
        }
        let text_block = is_md_block(name)
            && !matches!(
                name.local,
                local_name!("ul")
                    | local_name!("ol")
                    | local_name!("table")
                    | local_name!("hr")
                    | local_name!("blockquote")
            );
        if opts.block_emphasis.is_empty() || !text_block {
            return false;
        }
        self.write_mixed(text, id, opts);
        true
    }

    /// Writes an emphasis; `prev` is the closing run that ends `text`, if any.
    fn write_emphasis(
        &self,
        text: &mut String,
        emphasis_node: &TreeNode,
        opts: FormatOpts,
        prev: Option<PendingClose>,
    ) -> Option<PendingClose> {
        let Some(emphasis) = emphasis_node
            .as_element()
            .map(|el| &el.name)
            .and_then(|name| md_emphasis(name))
        else {
            return prev;
        };

        let cur_scope = EmphasisScope::from(emphasis);
        if !opts.table_cell
            && !opts.single_line
            && !opts.emphasis_scope.contains(cur_scope)
            && self.has_block_descendant(emphasis_node.id)
        {
            // markers cannot span blocks: each text block inside applies them
            self.write_mixed(text, emphasis_node.id, opts.block_emphasis(cur_scope));
            return None;
        }
        let em_opts = opts.inline().emphasis_scope(cur_scope);
        let mut emphasis_text = String::new();
        let ops = child_nodes(Ref::clone(&self.nodes), &emphasis_node.id, true)
            .map(SerializeOp::Open)
            .collect();
        let inner = self
            .write_ops(&mut emphasis_text, ops, em_opts, false)
            .filter(|p| p.end == emphasis_text.len());

        if emphasis_text.is_empty() {
            return prev;
        }
        if opts.emphasis_scope.contains(cur_scope) {
            // already emphasized: written as it is
            if let Some(p) = prev.filter(|p| p.end == text.len()) {
                if emphasis_text.starts_with(|c: char| !is_whitespace_or_punctuation(c)) {
                    p.before_letter(text);
                }
            }
            if emphasis_text.starts_with('[') {
                escape_trailing_bang(text);
            }
            let offset = text.len();
            text.push_str(&emphasis_text);
            return inner.map(|p| PendingClose {
                end: p.end + offset,
                remove: p.remove.map(|(open, marker)| (open + offset, marker)),
                ..p
            });
        }

        let mut marker = emphasis;
        // markers of two emphases back to back form one run, which reads as both
        // only when it is three long and a letter follows: `**a***b*` is fine,
        // `*a***$5**` is not, and `*a*__$5__` is
        let run = text.len() - text.trim_end_matches('*').len();
        let reads_as_both = run + emphasis.len() == 3
            && emphasis_text.starts_with(char::is_alphanumeric)
            && text[..text.len() - run]
                .chars()
                .next_back()
                .is_some_and(char::is_alphanumeric);
        let fused = run > 0
            && !reads_as_both
            && !text[..text.len() - run].ends_with('\\')
            && !emphasis_text.starts_with(char::is_whitespace)
            && !is_exact_marker_end(text, emphasis);
        if fused {
            // `_b_` loses its markers if a letter follows it, then the run
            // before it needs no punctuation inside either
            if let Some(p) = prev.filter(|p| p.end == text.len()) {
                move_tail_out(text, p.run, p.tail);
            }
            marker = underscore_marker(emphasis);
        }
        let closed = push_emphasis(text, &emphasis_text, marker)?;
        // removed if a letter follows: `_` markers, or ones around a code span
        let removable = (marker.starts_with('_') || closed.removable) && !closed.merged;
        match inner {
            // a closing run that ends the content ends this one's run too
            Some(p) if closed.tail == 0 => Some(PendingClose {
                end: text.len(),
                run: p.run + marker.len(),
                tail: p.tail,
                remove: None,
            }),
            _ if closed.tail > 0 || removable => Some(PendingClose {
                end: text.len(),
                run: marker.len(),
                tail: closed.tail,
                remove: removable.then_some((closed.open, marker)),
            }),
            _ => None,
        }
    }

    fn write_list_item(&self, text: &mut String, node_id: NodeId, ctx: &mut ListContext) {
        let prefix = ctx.prefix();
        // shift ol number after writing prefix
        ctx.advance_ol_number();
        trim_trailing_space(text);
        text.push_str(&ctx.list_indent());
        text.push_str(&prefix);
        if ctx.opts.block_emphasis.is_empty() {
            self.write(text, node_id, ctx.opts.list_item());
        } else {
            // an item is a text block: emphasis spanning blocks applies within it
            let mut item = String::new();
            self.write(
                &mut item,
                node_id,
                ctx.opts.list_item().applying_block_emphasis(),
            );
            push_emphasis(text, &item, ctx.opts.block_emphasis.markers());
        }
        text.push_str(ctx.linebreak());
    }

    /// Writes the children of an element inside emphasis that spans blocks:
    /// blocks apply it themselves, and each run of inline content between them
    /// is written emphasized.
    fn write_mixed(&self, text: &mut String, id: NodeId, opts: FormatOpts) {
        let markers = opts.block_emphasis.markers();
        let inline_opts = opts.applying_block_emphasis().inline();
        let mut run: Vec<NodeId> = Vec::new();
        let flush = |text: &mut String, run: &mut Vec<NodeId>| {
            if run.is_empty() {
                return;
            }
            let ops = run.drain(..).rev().map(SerializeOp::Open).collect();
            let mut content = String::new();
            self.write_ops(&mut content, ops, inline_opts, false);
            if !content.trim().is_empty() {
                push_emphasis(text, &content, markers);
            }
        };
        for child in child_nodes(Ref::clone(&self.nodes), &id, false) {
            let child_ref = NodeRef::new(child, self.root_node.tree);
            let is_block = child_ref
                .qual_name_ref()
                .is_some_and(|n| is_md_block(&n) || n.local == local_name!("pre"))
                || self.has_block_descendant(child);
            if is_block {
                flush(text, &mut run);
                self.write(text, child, opts.include_node());
            } else {
                run.push(child);
            }
        }
        flush(text, &mut run);
    }

    /// The first character of the text inside a node.
    fn first_char(&self, id: NodeId) -> Option<char> {
        descendant_nodes(Ref::clone(&self.nodes), &id).find_map(|d| {
            match &self.nodes[d.value()].data {
                NodeData::Text { contents } => contents.chars().next(),
                _ => None,
            }
        })
    }

    fn has_block_descendant(&self, id: NodeId) -> bool {
        descendant_nodes(Ref::clone(&self.nodes), &id).any(|d| {
            self.nodes[d.value()].as_element().is_some_and(|e| {
                is_md_block(&e.name)
                    || matches!(e.name.local, local_name!("li") | local_name!("pre"))
            })
        })
    }

    fn write_list_item_blocks(&self, text: &mut String, node_id: NodeId, ctx: &mut ListContext) {
        let child_node = NodeRef::new(node_id, self.root_node.tree);
        let prefix = ctx.prefix();
        ctx.advance_ol_number();
        let block_indent = " ".repeat(ctx.list_indent().len() + prefix.len());
        trim_trailing_space(text);
        // a nested `write` trims the buffer's start, which would shift `item_start`
        text.drain(..text.len() - text.trim_ascii_start().len());
        text.push_str(&ctx.list_indent());
        text.push_str(&prefix);

        let item_start = text.len();
        let mut is_first_block = true;
        // a code block or quote is followed by a blank line unless the item ends
        // or a code block comes next; there it would only make the list loose
        let mut blank_line_due = false;
        // reversed, so `pop` yields children in document order
        let mut children: Vec<_> = child_node.children_it(true).collect();
        while let Some(c) = children.pop() {
            if c.is_comment() {
                continue;
            }
            // block children are written without their own node, so skip them here
            if c.qual_name_ref()
                .is_some_and(|n| self.skip_tags.contains(&n.local.as_ref()))
            {
                continue;
            }
            if node_is_block_wrapper(&c) || node_is_inline_wrapper(&c) {
                children.extend(c.children_it(true));
                continue;
            }
            let is_blank_text = c.is_text() && c.text().trim_ascii().is_empty();
            if blank_line_due && !c.has_name("pre") && !is_blank_text {
                text.push_str(ctx.linebreak());
                blank_line_due = false;
            }
            if node_is_list_item_block(&c) {
                if c.has_name("hr") {
                    if ctx.opts.table_cell {
                        // a cell has no line for a rule; keep the words on either side apart
                        if text.len() > item_start {
                            blank_line_due = true;
                        }
                        continue;
                    }
                    // own line: `- ---` would end the list, `---` under text makes a heading
                    text.truncate(text.trim_ascii_end().len());
                    let has_content = text.len() > item_start;
                    text.push_str(ctx.linebreak());
                    if has_content {
                        text.push_str(ctx.linebreak());
                    }
                    is_first_block = false;
                    text.push_str(&block_indent);
                    text.push_str("---");
                    text.push_str(ctx.linebreak());
                    continue;
                }
                if self.is_empty_block(&c) {
                    // it would leave the item's first line empty
                    continue;
                }
                let indented = c.has_name("pre") || c.has_name("blockquote");
                if is_first_block {
                    is_first_block = false;
                    // a block after inline content starts its own indented line
                    if !indented && text.len() > item_start {
                        if !text.ends_with('\n') {
                            text.push_str(ctx.linebreak());
                        }
                        text.push_str(&block_indent);
                    }
                } else {
                    if !indented && !text.ends_with('\n') {
                        text.push_str(ctx.linebreak());
                    }
                    text.push_str(&block_indent);
                }

                if indented {
                    self.write_list_item_indented(text, &c, &block_indent, item_start, ctx);
                    blank_line_due = true;
                } else {
                    let heading = self.write_item_block(text, &c, item_start, &block_indent, ctx);
                    // a heading ends at its line end; a blank line would make the list loose
                    if heading {
                        escape_closing_hashes(text);
                    } else {
                        text.push_str(ctx.linebreak());
                    }
                }
                text.push_str(ctx.linebreak());
            } else {
                // text after a block starts a line; unindented it would leave the item
                if text.len() > item_start && text.ends_with('\n') && !is_blank_text {
                    text.push_str(&block_indent);
                }
                self.write_item_inline(text, &c, item_start, &block_indent, ctx);
            }
        }
        end_item(text, item_start, ctx);
    }

    /// Writes a child of a list item that is not a block of its own: inline
    /// content, a nested list, or an emphasis around blocks.
    fn write_item_inline(
        &self,
        text: &mut String,
        c: &NodeRef,
        item_start: usize,
        block_indent: &str,
        ctx: &ListContext,
    ) {
        // inline text can start a line, so it is escaped as in a plain item
        let opts = if node_is_list(c) {
            ctx.opts
        } else {
            ctx.opts.list_item()
        };
        let marker_end = text[..item_start].trim_end().len();
        let at_item_start = text.trim_end().len() == marker_end;
        let block_start = text.trim_end_matches(' ').len();
        self.write(text, c.id, opts.include_node());
        if at_item_start && node_is_list(c) {
            join_list_to_marker(text, marker_end);
        } else if self.is_block_emphasis(c) {
            // its blocks are written from column zero
            keep_block_in_item(text, block_start, item_start, block_indent);
        }
    }

    /// An emphasis around blocks, which applies it to each of them.
    fn is_block_emphasis(&self, n: &NodeRef) -> bool {
        n.qual_name_ref()
            .is_some_and(|name| md_emphasis(&name).is_some())
            && self.has_block_descendant(n.id)
    }

    /// A block with nothing to show: no text, image, rule or nested block.
    fn is_empty_block(&self, c: &NodeRef) -> bool {
        !c.has_name("hr")
            && c.text().trim().is_empty()
            && !self.has_descendant_img(&c.id)
            && !self.has_block_descendant(c.id)
    }

    /// Writes a block of a list item that is not indented by itself, keeping it
    /// in the item. True for a heading.
    fn write_item_block(
        &self,
        text: &mut String,
        c: &NodeRef,
        item_start: usize,
        block_indent: &str,
        ctx: &ListContext,
    ) -> bool {
        let heading = c.qual_name_ref().filter(|n| is_heading(n));
        // where this block's own text starts: a block at line start may
        // first trim the spaces before it
        let block_start = text.trim_end_matches(' ').len();
        if let Some(prefix) = heading.as_ref().and_then(|n| md_prefix(n)) {
            let prefix_start = text.len();
            text.push_str(prefix);
            self.write(text, c.id, ctx.opts.list_item().single_line());
            if text[prefix_start.min(text.len())..]
                .trim_start_matches('#')
                .trim()
                .is_empty()
            {
                // nothing to show: no heading at all
                text.truncate(prefix_start.min(text.len()));
            }
        } else {
            self.write(text, c.id, ctx.opts.list_item());
        }
        keep_block_in_item(text, block_start, item_start, block_indent);
        heading.is_some()
    }

    /// Writes a code block or block quote inside a list item. Both are written
    /// from column zero, so every line after the first is indented to the
    /// item's content column, or the block would end the list.
    fn write_list_item_indented(
        &self,
        text: &mut String,
        block_node: &NodeRef,
        block_indent: &str,
        item_start: usize,
        ctx: &ListContext,
    ) {
        let mut block = String::new();
        self.write(&mut block, block_node.id, ctx.opts.include_node());
        let block = block.trim_matches('\n');

        // after inline text, the block goes on its own line; a fence or `>`
        // can interrupt a paragraph, so no blank line is needed and the list
        // stays tight
        let at_line_start = text.len() == item_start
            || text
                .strip_suffix(block_indent)
                .is_some_and(|t| t.ends_with('\n'));
        if !at_line_start {
            // a trailing hard break is redundant before the block
            text.truncate(text.trim_end().len().max(item_start));
            text.push_str(ctx.linebreak());
            text.push_str(block_indent);
        }

        for (i, line) in block.split('\n').enumerate() {
            if i > 0 {
                text.push_str(ctx.linebreak());
                if !line.is_empty() {
                    text.push_str(block_indent);
                }
            }
            text.push_str(line);
        }
    }

    fn write_list(&self, text: &mut String, list_node: &TreeNode, el: &Element, opts: FormatOpts) {
        let kind = if el.name.local == local_name!("ol") {
            let start = el
                .attr("start")
                .and_then(|v| parse_list_number(&v))
                .unwrap_or(1);
            ListKind::Ol(start)
        } else {
            ListKind::Ul
        };

        let mut ctx = ListContext::new(opts, kind);
        let mut after_item = false;
        // content outside the items, gathered up to the next item or list
        let mut outside: Vec<NodeId> = Vec::new();

        for child_id in self.list_children(list_node.id) {
            let child_node = NodeRef::new(child_id, self.root_node.tree);

            if !node_is_list(&child_node) && !child_node.has_name("li") {
                outside.push(child_id);
                continue;
            }
            if self.write_outside_items(text, &mut outside, FormatOpts::new(), opts.table_cell) {
                after_item = false;
            }
            if node_is_list(&child_node) {
                if after_item && !opts.table_cell {
                    // a list right inside a list belongs to the item before it, kept tight
                    self.write(text, child_id, ctx.opts.list_item().include_node());
                    text.truncate(text.trim_end().len());
                    text.push_str(ctx.linebreak());
                } else {
                    // with no item before it, at the level of this list
                    outside.push(child_id);
                    self.write_outside_items(text, &mut outside, opts, opts.table_cell);
                    after_item = false;
                }
                continue;
            }
            after_item = true;

            let is_list_item = child_node.query_or(false, |t| {
                t.as_element()
                    .is_some_and(|e| e.name.local == local_name!("li"))
            });

            let has_blocks = child_node.children_it(false).any(|n| {
                node_is_list_item_block(&n)
                    || node_is_block_wrapper(&n)
                    || node_is_inline_wrapper(&n)
                    || self.is_block_emphasis(&n)
            });

            if is_list_item && matches!(ctx.kind, ListKind::Ol(_)) {
                // `<li value>` renumbers this item and the ones after it
                if let Some(child_el) = child_node.element_ref() {
                    if let Some(v) = child_el.attr("value").and_then(|v| parse_list_number(&v)) {
                        ctx.kind = ListKind::Ol(v);
                    }
                }
            }

            if is_list_item && has_blocks {
                self.write_list_item_blocks(text, child_id, &mut ctx);
            } else if is_list_item {
                self.write_list_item(text, child_id, &mut ctx);
            }
        }
        self.write_outside_items(text, &mut outside, FormatOpts::new(), opts.table_cell);
        // without a blank line, text after the list would continue its last item
        if !opts.table_cell {
            add_linebreaks(text, "\n", "\n\n");
        }
    }

    /// Writes content of a list that is outside its items, in one pass and set
    /// apart, so that it does not continue the item before it; empties `nodes`.
    /// False when there is nothing to write.
    fn write_outside_items(
        &self,
        text: &mut String,
        nodes: &mut Vec<NodeId>,
        opts: FormatOpts,
        table_cell: bool,
    ) -> bool {
        if nodes.is_empty() {
            return false;
        }
        let ops = nodes.drain(..).rev().map(SerializeOp::Open).collect();
        let mut content = String::new();
        self.write_ops(&mut content, ops, opts, true);
        if content.is_empty() {
            return false;
        }
        if table_cell {
            add_cell_block_break(text, "<br>");
            text.push_str(&content);
            add_cell_block_break(text, "<br>");
        } else {
            add_linebreaks(text, "\n", "\n\n");
            text.push_str(&content);
            add_linebreaks(text, "\n", "\n\n");
        }
        true
    }

    /// A list's children, with each element that wraps items replaced by its
    /// own children, as in `<ul><div><li>`.
    fn list_children(&self, id: NodeId) -> Vec<NodeId> {
        let mut children = Vec::new();
        for c in child_nodes(Ref::clone(&self.nodes), &id, false) {
            if self.wraps_items(c) {
                children.extend(self.list_children(c));
            } else {
                children.push(c);
            }
        }
        children
    }

    /// Whether an element other than an item or a list holds items, directly
    /// or through other such elements.
    fn wraps_items(&self, id: NodeId) -> bool {
        let is_wrapper = self.nodes[id.value()].as_element().is_some_and(|e| {
            !matches!(
                e.name.local,
                local_name!("li") | local_name!("ul") | local_name!("ol")
            )
        });
        is_wrapper
            && child_nodes(Ref::clone(&self.nodes), &id, false).any(|c| {
                self.nodes[c.value()]
                    .as_element()
                    .is_some_and(|e| e.name.local == local_name!("li"))
                    || self.wraps_items(c)
            })
    }

    fn write_link(&self, text: &mut String, link_node: &TreeNode) {
        let Some(el) = link_node.as_element() else {
            return;
        };

        let default_opts = FormatOpts::new();

        let Some(href) = el.attr("href") else {
            return;
        };

        let mut link_text = String::new();
        let has_img = self.has_descendant_img(&link_node.id);

        let mut is_md_body = false;
        // try to collect plain text if there is no image
        if !has_img {
            let link_opts = FormatOpts::new().include_node().skip_escape();
            self.write_text(&mut link_text, link_node.id, link_opts);
        }
        // a link with only whitespace to show still separates the words around it
        if !link_text.is_empty() && link_text.trim_ascii().is_empty() {
            push_normalized_text(text, &link_text, default_opts);
            return;
        }
        // serialize as markdown if body contains images or non-text elements
        if has_img || link_text.is_empty() {
            // link text cannot span a blank line or hold a block marker
            let body_opts = if has_img {
                default_opts.single_line()
            } else {
                default_opts
            };
            self.write(&mut link_text, link_node.id, body_opts);
            is_md_body = true;
        }

        if link_text.is_empty() {
            return;
        }

        // an unescaped trailing `!` would turn `[x](u)` into image syntax
        escape_trailing_bang(text);
        text.push('[');
        if is_md_body {
            text.push_str(&link_text);
        } else {
            push_normalized_text(text, &link_text, default_opts);
        }

        text.push_str("](");
        push_md_url(text, &href);

        if let Some(title) = el.attr_ref(local_name!("title")) {
            push_title(text, title);
        }

        text.push(')');
    }

    fn has_descendant_img(&self, id: &NodeId) -> bool {
        descendant_nodes(Ref::clone(&self.nodes), id).any(|child_id| {
            matches!(
                &self.nodes[child_id.value()].data,
                NodeData::Element(e) if e.name.local == local_name!("img")
            )
        })
    }

    fn write_img(text: &mut String, img_node: &TreeNode) {
        let Some(el) = img_node.as_element() else {
            return;
        };

        let src = el
            .attr("src")
            .filter(|s| !s.trim().is_empty())
            .or_else(|| {
                el.attr("srcset").and_then(|s| {
                    s.split_ascii_whitespace()
                        .next()
                        .map(|u| u.trim_end_matches(','))
                        .filter(|u| !u.is_empty())
                        .map(StrTendril::from_slice)
                })
            })
            .or_else(|| el.attr("data-src").filter(|s| !s.trim().is_empty()));
        if let Some(src) = src {
            text.push_str("![");

            if let Some(alt) = el.attr_ref(local_name!("alt")) {
                push_normalized_text(text, alt, FormatOpts::new());
            }
            text.push(']');
            text.push('(');
            push_md_url(text, &src);
            if let Some(title) = el.attr_ref(local_name!("title")) {
                push_title(text, title);
            }
            text.push(')');
        }
    }

    /// Tries to find the language label in the given node using a heuristic.
    ///
    /// Pages may use custom `data-` attributes on the tag itself.
    fn find_code_language(&self, node: &TreeNode) -> Option<String> {
        // Check the current node
        if let Some(language) = find_code_lang_attribute(node) {
            return Some(language);
        }

        ancestor_nodes(Ref::clone(&self.nodes), &node.id, Some(3))
            .find_map(|id| find_code_lang_attribute(&self.nodes[id.value()]))
            .or_else(|| self.find_code_language_css_class(node))
    }

    /// Tries to find the language from the CSS class of the first `<code>` element child of the `<pre>` block.
    fn find_code_language_css_class(&self, pre_node: &TreeNode) -> Option<String> {
        let code_elem = child_nodes(Ref::clone(&self.nodes), &pre_node.id, false).find_map(|id| {
            let node = &self.nodes[id.value()];
            node.as_element()
                .filter(|el| el.name.local == local_name!("code"))
        });

        code_elem?
            .class()?
            .split_ascii_whitespace()
            .find_map(|class| class.strip_prefix(CODE_LANGUAGE_PREFIX))
            .map(sanitize_attr_value)
    }

    /// Transforms a `<pre>` code block, possibly with an associated language label that the resulting
    /// block is annotated with.
    fn write_pre(&self, text: &mut String, pre_node: &TreeNode) {
        let content = pre_text(Ref::clone(&self.nodes), pre_node.id);

        // The fence must be longer than any backtick run in the content,
        // otherwise an interior fence-length line terminates the block early
        // (CommonMark §fenced-code-blocks).
        let fence_len = max_backtick_run(&content).max(2) + 1;

        if !text.ends_with("\n\n") {
            text.push('\n');
        }
        text.extend(std::iter::repeat_n('`', fence_len));

        if let Some(lang) = self.find_code_language(pre_node) {
            text.push_str(&lang);
        }
        text.push('\n');

        text.push_str(&content);

        // The closing fence goes on its own line; a final newline of the
        // content already ends the last line, and empty content has no line.
        if !content.is_empty() && !content.ends_with('\n') {
            text.push('\n');
        }

        text.extend(std::iter::repeat_n('`', fence_len));
        text.push('\n');
    }

    /// Writes the content of the `<code>` block. Generally a `<code>` tag is used inline, but unfortunately
    /// it's also used instead of a `<pre>` block. In case the `<code>` block contains multiline
    /// text, it's handled as a `<pre>` code block.
    fn write_code(&self, text: &mut String, code_node: &TreeNode, opts: FormatOpts) {
        let is_multiline = descendant_nodes(Ref::clone(&self.nodes), &code_node.id)
            .map(|id| &self.nodes[id.value()])
            .filter_map(|t| match t.data {
                NodeData::Text { ref contents } => Some(contents),
                _ => None,
            })
            .any(|text| text.trim().contains('\n'));

        if is_multiline && opts.single_line {
            return self.write_line_code(text, code_node);
        }
        if is_multiline && opts.table_cell {
            return self.write_cell_code(text, code_node);
        }
        if is_multiline {
            return self.write_pre(text, code_node);
        }

        // edge spaces are content, so keep them; whitespace around a line
        // break at an edge is source indentation
        // TODO: return to this -- read the raw edges without allocating the
        // full text a second time
        let raw = TreeNodeOps::text_of(Ref::clone(&self.nodes), code_node.id);
        let mut code_text = String::new();
        self.write(
            &mut code_text,
            code_node.id,
            FormatOpts::new()
                .include_node()
                .inline()
                .skip_escape()
                .skip_md(),
        );
        if raw.trim_end_matches([' ', '\t']).ends_with('\n') {
            trim_trailing_space(&mut code_text);
        }
        if raw.trim_start_matches([' ', '\t']).starts_with('\n') {
            code_text.drain(..code_text.len() - code_text.trim_start_matches(' ').len());
        }
        push_code_text(text, &code_text);
    }

    /// Writes a code block on one line, as one code span, with the words of
    /// its lines apart.
    fn write_line_code(&self, text: &mut String, node: &TreeNode) {
        let content = pre_text(Ref::clone(&self.nodes), node.id);
        let words: Vec<&str> = content.split_ascii_whitespace().collect();
        push_space(text);
        push_code_text(text, &words.join(" "));
        push_space(text);
    }

    /// Writes a code block in a table cell as a code span per line, joined
    /// with `<br>`.
    fn write_cell_code(&self, text: &mut String, node: &TreeNode) {
        let content = pre_text(Ref::clone(&self.nodes), node.id);
        for (i, line) in content.lines().filter(|l| !l.trim().is_empty()).enumerate() {
            if i > 0 {
                text.push_str("<br>");
            }
            push_code_text(text, line);
        }
    }

    fn write_blockquote(&self, text: &mut String, quote_node: &TreeNode) {
        let opts = FormatOpts::new();
        let mut quote_buf = String::new();
        self.write(&mut quote_buf, quote_node.id, opts);

        if quote_buf.is_empty() {
            return;
        }

        while !text.ends_with("\n\n") {
            text.push('\n');
        }

        for line in quote_buf.lines() {
            text.push_str("> ");
            text.push_str(line);
            text.push('\n');
        }

        text.push('\n');
    }

    fn write_table(&self, text: &mut String, table_node: &TreeNode) {
        let table_ref = NodeRef::new(table_node.id, self.root_node.tree);

        let Some((header, body)) = table_layout(&table_ref) else {
            self.write(text, table_node.id, FormatOpts::default());
            return;
        };

        // a table has no caption syntax: it becomes a paragraph before the table
        if let Some(caption) = table_ref.find(&["caption"]).first() {
            let mut caption_text = String::new();
            self.write(&mut caption_text, caption.id, FormatOpts::new());
            if !caption_text.trim().is_empty() {
                add_linebreaks(text, "\n", "\n\n");
                text.push_str(caption_text.trim());
                text.push('\n');
            }
        }

        let opts = FormatOpts::new().table_cell();
        let write_row = |cells: &[Option<NodeRef>]| -> Vec<String> {
            cells
                .iter()
                .map(|cell| {
                    let mut cell_text = String::new();
                    if let Some(cell) = cell {
                        self.write(&mut cell_text, cell.id, opts);
                        trim_trailing_cell_break(&mut cell_text);
                        escape_table_cell_inplace(&mut cell_text);
                    }
                    cell_text
                })
                .collect()
        };
        let mut headings = header.as_deref().map(write_row).unwrap_or_default();
        let rows: Vec<Vec<String>> = body.iter().map(|r| write_row(r)).collect();
        let columns = rows.first().map_or(headings.len(), Vec::len);
        while headings.len() < columns {
            headings.push(" ".into());
        }

        // set apart by one blank line, which the block before may have left
        add_linebreaks(text, "\n", "\n\n");
        text.push_str("| ");
        text.push_str(&headings.join(" | "));
        text.push_str(" |\n| ");
        text.push_str(
            &headings
                .iter()
                .map(|s| "-".repeat(s.len().max(1)))
                .collect::<Vec<_>>()
                .join(" | "),
        );
        text.push_str(" |\n");
        for row in rows {
            text.push_str("| ");
            text.push_str(&row.join(" | "));
            text.push_str(" |\n");
        }
        text.push('\n');
    }
}

// Limited set of elements treated as block-level in Markdown output.
const fn is_md_block(name: &QualName) -> bool {
    matches!(
        name.local,
        local_name!("article")
            | local_name!("blockquote")
            | local_name!("section")
            | local_name!("div")
            | local_name!("p")
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
            | local_name!("hr")
    )
}

/// Keeps the block written from `start` inside its list item: a block that
/// opens the item continues the marker's line, and lines nested blocks start at
/// column zero are indented, as either would otherwise end the item. Nested
/// lists come already indented.
fn keep_block_in_item(text: &mut String, start: usize, item_start: usize, indent: &str) {
    let start = start.min(text.len());
    let (head, written) = text.split_at(start);
    let mut fixed = String::with_capacity(written.len() + indent.len());
    let mut lines = written.split('\n');
    if start + 1 >= item_start && start <= item_start && written.starts_with('\n') {
        // the item's first block: back onto the marker's line
        let first = written.trim_start_matches(['\n', ' ']);
        fixed.push(' ');
        lines = first.split('\n');
    }
    if let Some(first) = lines.next() {
        // a block may trim the indentation written before it
        if head.ends_with('\n') && !first.is_empty() && !first.starts_with(' ') {
            fixed.push_str(indent);
        }
        fixed.push_str(first);
    }
    for line in lines {
        fixed.push('\n');
        if !line.is_empty() && !line.starts_with(' ') {
            fixed.push_str(indent);
        }
        fixed.push_str(line);
    }
    // only this block is rewritten: the output before it can be long
    text.truncate(start);
    text.push_str(&fixed);
}

/// Joins a list written right after a bare item marker to it: a blank line
/// after the marker would end the item.
fn join_list_to_marker(text: &mut String, marker_end: usize) {
    let breaks = text[marker_end..].len() - text[marker_end..].trim_start_matches('\n').len();
    if breaks > 1 {
        text.replace_range(marker_end..marker_end + breaks, "\n");
    }
}

/// Ends a list item written with blocks on a line break, keeping at most one
/// blank line: spaces written by its last child would leave another.
fn end_item(text: &mut String, item_start: usize, ctx: &ListContext) {
    if !ctx.opts.table_cell {
        let end = text.trim_end().len().max(item_start);
        let breaks = text[end..].matches('\n').count().min(2);
        text.truncate(end);
        text.push_str(&"\n".repeat(breaks));
    }
    if !text.ends_with('\n') {
        text.push_str(ctx.linebreak());
    }
}

/// Elements that start a line of their own besides Markdown blocks.
const fn is_line_element(name: &QualName) -> bool {
    matches!(
        name.local,
        local_name!("br")
            | local_name!("li")
            | local_name!("tr")
            | local_name!("td")
            | local_name!("th")
    )
}

/// Separates words with a space; at the start of a buffer too, as that may be
/// the inside of an emphasis, which moves the space out.
fn push_space(text: &mut String) {
    if !text.ends_with(char::is_whitespace) || text.is_empty() {
        text.push(' ');
    }
}

/// Block children of a table cell are joined with a single cell linebreak,
/// as a cell cannot hold a blank line.
fn add_cell_block_break(text: &mut String, linebreak: &str) {
    trim_trailing_space(text);
    if !text.is_empty() && !ends_with_cell_break(text) {
        text.push_str(linebreak);
    }
}

/// Drops trailing cell linebreaks left by a final `<br>` or block child.
fn trim_trailing_cell_break(text: &mut String) {
    while ends_with_cell_break(text) {
        text.truncate(text.len() - "<br>".len());
        trim_trailing_space(text);
    }
}

/// Whether the text ends with a cell linebreak rather than escaped cell text:
/// `\<br>` is text, `\\<br>` is an escaped backslash and then a linebreak.
fn ends_with_cell_break(text: &str) -> bool {
    text.strip_suffix("<br>")
        .is_some_and(|rest| (rest.len() - rest.trim_end_matches('\\').len()) % 2 == 0)
}

fn node_is_md_block(node: &NodeRef) -> bool {
    node.qual_name_ref().is_some_and(|name| is_md_block(&name))
}

/// A child of `<li>` that is written as its own block; nested lists are
/// handled by `write_list`.
fn node_is_list_item_block(node: &NodeRef) -> bool {
    !node_is_list(node) && (node_is_md_block(node) || node.has_name("pre"))
}

/// A `<div>`-like element holding only blocks; its children are written as the
/// item's own blocks. Inline text, a table or a rule would be glued or flattened.
fn node_is_block_wrapper(node: &NodeRef) -> bool {
    node.qual_name_ref()
        .is_some_and(|name| is_block_wrapper(&name))
        && node.children_it(false).any(|n| node_is_list_item_block(&n))
        && node.children_it(false).all(|n| {
            node_is_list(&n)
                || (node_is_list_item_block(&n) && !n.has_name("table") && !n.has_name("hr"))
                || n.is_comment()
                || (n.is_text() && n.text().trim_ascii().is_empty())
        })
}

/// An inline element, such as `<span>` or `<label>`, around blocks of a list
/// item; its children are written as the item's own, as its blocks would
/// otherwise leave the item. Emphasis, code and links write their own content.
fn node_is_inline_wrapper(node: &NodeRef) -> bool {
    let Some(name) = node.qual_name_ref() else {
        return false;
    };
    let writes_own = is_md_block(&name)
        || md_emphasis(&name).is_some()
        || matches!(
            name.local,
            local_name!("pre")
                | local_name!("table")
                | local_name!("hr")
                | local_name!("code")
                | local_name!("img")
        )
        || (name.local == local_name!("a") && node.has_attr("href"));
    !writes_own
        && node.children_it(false).any(|n| {
            node_is_list(&n)
                || node_is_list_item_block(&n)
                || node_is_block_wrapper(&n)
                || node_is_inline_wrapper(&n)
        })
}

const fn is_block_wrapper(name: &QualName) -> bool {
    matches!(
        name.local,
        local_name!("div") | local_name!("section") | local_name!("article")
    )
}

const fn is_list(name: &QualName) -> bool {
    matches!(name.local, local_name!("ul") | local_name!("ol"))
}

fn node_is_list(node: &NodeRef) -> bool {
    node.qual_name_ref().is_some_and(|name| is_list(&name))
}

const fn is_heading(name: &QualName) -> bool {
    matches!(
        name.local,
        local_name!("h1")
            | local_name!("h2")
            | local_name!("h3")
            | local_name!("h4")
            | local_name!("h5")
            | local_name!("h6")
    )
}

const fn md_prefix(name: &QualName) -> Option<&'static str> {
    let prefix = match name.local {
        local_name!("h1") => "# ",
        local_name!("h2") => "## ",
        local_name!("h3") => "### ",
        local_name!("h4") => "#### ",
        local_name!("h5") => "##### ",
        local_name!("h6") => "###### ",
        local_name!("hr") => "---",
        _ => "",
    };

    if prefix.is_empty() {
        None
    } else {
        Some(prefix)
    }
}

/// Writes what follows the content of an element: the line breaks after a
/// block or a `<br>`, or a space in a single-line context.
fn close_element(
    text: &mut String,
    name: &QualName,
    opts: FormatOpts,
    linebreak: &str,
    double_br: &str,
) {
    if opts.single_line {
        if is_md_block(name) || is_line_element(name) {
            push_space(text);
        }
        return;
    }
    if text.ends_with(double_br) {
        return;
    }

    if !opts.skip_md {
        if is_heading(name) {
            escape_closing_hashes(text);
        }
        if is_md_block(name) {
            if opts.table_cell {
                add_cell_block_break(text, linebreak);
            } else {
                add_linebreaks(text, linebreak, double_br);
            }
        } else if matches!(
            name.local,
            local_name!("br") | local_name!("li") | local_name!("tr")
        ) {
            // normalize `<br>` as linebreak
            // fallback for `li` and `tr` elements rendered outside their standard context
            trim_trailing_space(text);
            text.push_str("  ");
            text.push_str(linebreak);
        } else if matches!(name.local, local_name!("td") | local_name!("th")) {
            // cells of a table not written as one keep their words apart
            push_space(text);
        }
    } else if name.local == local_name!("br") && !text.ends_with(' ') {
        // normalize `<br>` as a space when Markdown output is disabled
        text.push(' ');
    }
}

/// A closing marker run that ends the output with punctuation before it: it
/// delimits emphasis only if whitespace or punctuation follows.
#[derive(Clone, Copy)]
struct PendingClose {
    /// The output length right after the run.
    end: usize,
    /// Bytes of closing markers in the run.
    run: usize,
    /// Bytes of punctuation before the run.
    tail: usize,
    /// An emphasis opened at this offset with this marker that a letter right
    /// after its closing marker keeps from closing, whatever comes before:
    /// one written with `_`, or around a code span or link.
    remove: Option<(usize, &'static str)>,
}

impl PendingClose {
    /// Rewrites the run for a character that follows it and is neither
    /// whitespace nor punctuation, such as a letter.
    fn before_letter(self, text: &mut String) {
        match self.remove {
            Some((open, marker)) => remove_emphasis(text, open, marker),
            None => move_tail_out(text, self.run, self.tail),
        }
    }
}

/// The `_` form of an emphasis marker.
fn underscore_marker(marker: &'static str) -> &'static str {
    match marker {
        "**" => "__",
        "*" => "_",
        _ => marker,
    }
}

const fn md_emphasis(name: &QualName) -> Option<&'static str> {
    match name.local {
        local_name!("strong") | local_name!("b") => Some("**"),
        local_name!("em") | local_name!("i") => Some("*"),
        _ => None,
    }
}

/// A table's rows, laid out on its grid: `None` stands for a column taken by
/// a cell spanning several columns or rows, or missing at the end of a row.
type TableRows<'a> = Vec<Vec<Option<NodeRef<'a>>>>;

/// The header row of a table, if it has one, and its body rows, all as wide
/// as the widest; `None` when the table holds another table, or no cells.
#[allow(clippy::type_complexity)]
fn table_layout<'a>(
    table_node: &NodeRef<'a>,
) -> Option<(Option<Vec<Option<NodeRef<'a>>>>, TableRows<'a>)> {
    if table_node.is("table:has(table)") {
        return None;
    }
    let header_id = table_header_row(table_node).map(|row| row.id);
    let rows: Vec<(NodeId, Vec<NodeRef>)> = table_node
        .find(&["tr"])
        .into_iter()
        .map(|row| {
            (
                row.id,
                row.children_it(false)
                    .filter(is_table_cell)
                    .collect::<Vec<_>>(),
            )
        })
        .filter(|(_, cells)| !cells.is_empty())
        .collect();
    // a cell spans at most as many columns as the widest row has cells:
    // `colspan="100"` is a way to fill the width
    let max_columns = rows.iter().map(|(_, cells)| cells.len()).max()?;
    let max_rows = rows.len();
    let mut header = None;
    let mut grid: TableRows = Vec::with_capacity(rows.len());
    // for each column, how many more rows a cell from above still takes
    let mut taken: Vec<usize> = Vec::new();
    for (row_id, cells) in rows {
        let mut line = Vec::new();
        let mut cells = cells.into_iter();
        loop {
            let col = line.len();
            if taken.get(col).is_some_and(|&n| n > 0) {
                taken[col] -= 1;
                line.push(None);
                continue;
            }
            let Some(cell) = cells.next() else {
                break;
            };
            let below = cell_span(&cell, "rowspan", max_rows) - 1;
            for i in 0..cell_span(&cell, "colspan", max_columns) {
                let col = line.len();
                if taken.len() <= col {
                    taken.resize(col + 1, 0);
                }
                taken[col] = below;
                line.push((i == 0).then_some(cell));
            }
        }
        // columns past the row's last cell, taken from above
        for n in taken.iter_mut().skip(line.len()) {
            *n = n.saturating_sub(1);
        }
        if header_id == Some(row_id) {
            header = Some(line);
        } else {
            grid.push(line);
        }
    }
    let columns = grid.iter().chain(header.iter()).map(Vec::len).max()?;
    for line in grid.iter_mut().chain(header.iter_mut()) {
        line.resize(columns, None);
    }
    Some((header, grid))
}

/// A cell's `colspan` or `rowspan`, from 1 to `max`.
fn cell_span(cell: &NodeRef, attr: &str, max: usize) -> usize {
    cell.attr(attr)
        .and_then(|v| v.trim().parse::<usize>().ok())
        .map_or(1, |n| n.clamp(1, max))
}

fn is_table_cell(node: &NodeRef) -> bool {
    node.has_name("th") || node.has_name("td")
}

/// The first row with cells, when they are all `th`; a `th` next to a `td`
/// labels its row instead.
fn table_header_row<'a>(table_node: &NodeRef<'a>) -> Option<NodeRef<'a>> {
    let mut rows = table_node.find(&["tr"]).into_iter();
    let row = rows.find(|row| row.children_it(false).any(|c| is_table_cell(&c)))?;
    let all_th = row
        .children_it(false)
        .filter(is_table_cell)
        .all(|c| c.has_name("th"));
    all_th.then_some(row)
}

fn find_code_lang_attribute(node: &TreeNode) -> Option<String> {
    node.as_element()?
        .attrs
        .iter()
        .find(|attr| CODE_LANGUAGE_ATTRIBUTES.contains(&attr.name.local.as_ref()))
        .map(|attr| sanitize_attr_value(&attr.value))
}

/// Whether a list can start right after a line of text: its first item must
/// have text, and an ordered list must start at 1 (`CommonMark` §5.2).
fn can_interrupt_paragraph(node: &NodeRef) -> bool {
    let Some(ref first_li) = node.first_element_child().filter(|n| n.has_name("li")) else {
        return false;
    };
    let has_text = first_li
        .children_it(false)
        .take_while(|c| !node_is_list(c))
        .any(|c| !c.text().trim().is_empty());

    if !has_text {
        return false;
    }
    if !node.has_name("ol") {
        return true;
    }
    let first_number = first_li
        .attr("value")
        .and_then(|v| parse_list_number(&v))
        .or_else(|| node.attr("start").and_then(|v| parse_list_number(&v)))
        .unwrap_or(1);
    first_number == 1
}

/// Parses an `ol start` or `li value` attribute the way HTML does: leading
/// whitespace, an optional sign, then digits, ignoring anything after them
/// (`" 5abc"` is 5). A Markdown list number cannot be negative, so negative
/// values become 0, and values above the nine-digit limit are clamped.
fn parse_list_number(value: &str) -> Option<u32> {
    // Trim leading ASCII whitespace according to WHATWG HTML standard
    let value = value.trim_start_matches(|c: char| c.is_ascii_whitespace());
    // Parse an optional sign prefix
    let (negative, value) = match value.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, value.strip_prefix('+').unwrap_or(value)),
    };
    // Extract the contiguous sequence of ASCII digits
    let digits_len = value.bytes().take_while(u8::is_ascii_digit).count();
    if digits_len == 0 {
        return None;
    }
    // md doesn't support negative list markers, clamp to zero
    if negative {
        return Some(0);
    }

    // strip leading zeros from the digit slice
    let meaningful = value[..digits_len].trim_start_matches('0');
    // if string becomes empty, the input was "0" or multiple zeros ("000")
    if meaningful.is_empty() {
        return Some(0);
    }
    // fast-path: if significant digits exceed 9, clamp to the CommonMark limit
    if meaningful.len() > 9 {
        return Some(MAX_LIST_NUMBER);
    }
    // safe to unwrap since it contains 1..=9 digits and fits in u32 without overflow
    let n = meaningful.parse::<u32>().unwrap_or(u32::MAX);
    Some(n.min(MAX_LIST_NUMBER))
}
