mod constants;
mod ext;
mod opts;
mod serializer;
mod text_utils;

use tendril::StrTendril;

use crate::node::NodeRef;

pub fn serialize_md(
    root_node: &NodeRef,
    include_node: bool,
    skip_tags: Option<&[&str]>,
) -> StrTendril {
    serializer::MDSerializer::new(root_node, skip_tags).serialize(include_node)
}

#[cfg(test)]
mod tests {

    use crate::Document;

    use super::*;

    #[track_caller]
    fn html_2md_compare(html_contents: &str, expected: &str) {
        let doc = Document::from(html_contents);
        let root_node = &doc.root();
        let md_text = serialize_md(root_node, false, None);
        assert_eq!(md_text.as_ref(), expected);
    }

    #[test]
    fn test_heading_closing_hashes() {
        // a trailing run of `#` would be dropped as the closing sequence
        html_2md_compare("<h2>Title #</h2>", "## Title \\#");
        html_2md_compare("<h1>#</h1>", "# \\#");
        html_2md_compare("<h2>A ### </h2>", "## A \\###");
        html_2md_compare("<h1>C# and F#</h1>", "# C# and F#");
        html_2md_compare("<h2>Title #<br></h2>", "## Title \\#");
    }

    #[test]
    fn test_headings() {
        // when passing include_node: true, leading and trailing whitespaces will be kept.
        let contents = r"<h1>Heading 1</h1>
        <h2>Heading 2</h2>
        <h3>Heading 3</h3>
        <h4>Heading 4</h4>
        <h5>Heading 5</h5>
        <h6>Heading 6</h6>
        <h3><span>III.</span> Heading With Span</h3>
        <h3><span></span>Early years (2006–2009)</h3>
        <h3><span> </span> Early years (2006–2009)</h3>
        <hr>";

        let expected = "\n\n# Heading 1\n\n\
        ## Heading 2\n\n\
        ### Heading 3\n\n\
        #### Heading 4\n\n\
        ##### Heading 5\n\n\
        ###### Heading 6\n\n\
        ### III. Heading With Span\n\n\
        ### Early years (2006–2009)\n\n\
        ### Early years (2006–2009)\n\n\
        ---\n\n";

        let doc = Document::from(contents);
        let body_sel = &doc.select("body");
        let body_node = body_sel.nodes().first().unwrap();
        let md_text = serialize_md(body_node, true, None);
        assert_eq!(md_text.as_ref(), expected);
    }

    #[test]
    fn test_escaping() {
        // Punctuation that has no Markdown meaning in prose is not escaped.
        html_2md_compare("<p>Foo. Bar!</p>", "Foo. Bar!");
        html_2md_compare(
            "<p>Call (555) 123-4567, see {a: 1} and https://example.com/x.</p>",
            "Call (555) 123-4567, see {a: 1} and https://example.com/x.",
        );
        // mid-line `#`, `>` and `-` keep their literal meaning
        html_2md_compare(
            "<p>see # tags, a > b, x - y</p>",
            "see # tags, a > b, x - y",
        );

        // Characters that can start a Markdown block when a line begins are
        // escaped there.
        html_2md_compare("<p># not a heading</p>", "\\# not a heading");
        html_2md_compare("<p>> not a quote</p>", "\\> not a quote");
        html_2md_compare("<p>- not a list</p>", "\\- not a list");
        // ... and only where they would start one: `#` and `+` need a space after
        html_2md_compare("<p>#tag-name</p>", "#tag-name");
        html_2md_compare("<p>+1-2</p>", "+1-2");
        html_2md_compare("<p>>foo>bar</p>", "\\>foo>bar");
        // a preceding space on the same line is not a line start
        html_2md_compare("<p><span>foo </span><span>#bar</span></p>", "foo #bar");
        // a line of only `=` would turn the line above into a heading
        html_2md_compare("<p>Title<br>===</p>", "Title  \n\\===");
        html_2md_compare("<p>=> a == b</p>", "=> a == b");
        html_2md_compare(
            "<p>2024. not an ordered list</p>",
            "2024\\. not an ordered list",
        );
        // ...but only there
        html_2md_compare("<p>see 2024. below</p>", "see 2024. below");

        // `!` is only escaped when it could introduce an image
        html_2md_compare("<p>Wow!</p>", "Wow!");
        html_2md_compare("<p>not ![an image]</p>", "not \\![an image\\]");

        // Characters with Markdown meaning anywhere in the line are escaped.
        html_2md_compare(
            "<p>a *b* c _d_ e`f` [g]</p>",
            "a \\*b\\* c \\_d\\_ e\\`f\\` [g\\]",
        );
        // `|` only in tables, whose cells escape it so it cannot break a row
        html_2md_compare("<p>a | b</p>", "a | b");
        html_2md_compare(
            "<table><tr><td>a | b</td></tr></table>",
            "|   |\n| - |\n| a \\| b |",
        );
    }

    #[test]
    fn test_tilde_escaping() {
        html_2md_compare("<p>~~~</p><p>code?</p>", "\\~\\~\\~\n\ncode?");
        html_2md_compare("<p>~~strike~~</p>", "\\~\\~strike\\~\\~");
        html_2md_compare(
            "<p>run me@h:~/a$ make then me@h:~/b$ go</p>",
            "run me@h:\\~/a$ make then me@h:\\~/b$ go",
        );
        html_2md_compare("<p><code>~/a</code></p>", "`~/a`");
    }

    #[test]
    fn test_italic() {
        let contents = r"<h4><i>Italic Text</i></h4>";
        let expected = "#### *Italic Text*";

        html_2md_compare(contents, expected);
    }

    #[test]
    fn test_span_italic() {
        let contents = r"<span>It`s like <i>that</i></span>";
        let expected = r"It\`s like *that*";

        html_2md_compare(contents, expected);
    }

    #[test]
    fn test_bold_italic() {
        let contents = r"<span>It`s like <b><i>that</i></b></span>";
        let expected = r"It\`s like ***that***";

        html_2md_compare(contents, expected);
    }

    #[test]
    fn test_simple_code() {
        let contents = r"<span>It`s like <code>that</code></span>";
        let expected = r"It\`s like `that`";
        html_2md_compare(contents, expected);
    }

    #[test]
    fn test_code_edge_spaces() {
        // a parser strips one space from each side of `` ` a ` ``, so the
        // content is padded to keep both
        html_2md_compare("<p><code> a </code></p>", "`  a  `");
        // a space on one side only is kept as is
        html_2md_compare("<p><code> a</code></p>", "` a`");
    }

    #[test]
    fn test_false_multiline_code() {
        let contents = r"<span>
        It`s like 
        <code>
        that
        </code>
        </span>";
        let expected = r"It\`s like `that`";
        html_2md_compare(contents, expected);
    }

    #[test]
    fn test_multiline_code() {
        let contents = r"<code>$ cargo new hello
    Created binary (application) `hello` package

$ cd hello</code>";
        let expected = r"```
$ cargo new hello
    Created binary (application) `hello` package

$ cd hello
```";
        html_2md_compare(contents, expected);
    }

    #[test]
    fn test_code_span_with_backticks() {
        // Backslash escapes are not interpreted inside code spans, so content
        // containing backticks must be wrapped in a longer delimiter run
        // (CommonMark code fence rule), with spaces padding the content.
        html_2md_compare("<p><code>a `b` c</code></p>", "``a `b` c``");
        html_2md_compare("<p><code>`leading</code></p>", "`` `leading ``");
        html_2md_compare("<p><code>trailing`</code></p>", "`` trailing` ``");
        html_2md_compare("<p><code>```</code></p>", "```` ``` ````");
        // content without backticks keeps the single-backtick form
        html_2md_compare("<p><code>go.sum</code></p>", "`go.sum`");
    }
    #[test]
    fn test_ul() {
        let contents = "<h3>Pizza Margherita Ingredients</h3>\
        <ul>\
            <li>Pizza Dough</li>\
            <li>Mozzarella cheese</li>\
            <li>Tomatoes</li>\
            <li>Olive Oil</li>\
            <li><i>Basil</i></li>\
            <li><b>Salt</b></li>\
        </ul>";

        let expected = "### Pizza Margherita Ingredients\n\n\
        - Pizza Dough\n\
        - Mozzarella cheese\n\
        - Tomatoes\n\
        - Olive Oil\n\
        - *Basil*\n\
        - **Salt**";

        html_2md_compare(contents, expected);
    }

    #[test]
    fn test_ol() {
        let contents = "<h3>Pizza Margherita Ingredients</h3>\
        <ol>\
            <li>Pizza Dough</li>\
            <li>Mozzarella cheese</li>\
            <li>Tomatoes</li>\
            <li>Olive Oil</li>\
            <li><i>Basil</i></li>\
            <li><b>Salt</b></li>\
        </ol>";

        let expected = "### Pizza Margherita Ingredients\n\n\
        1. Pizza Dough\n\
        2. Mozzarella cheese\n\
        3. Tomatoes\n\
        4. Olive Oil\n\
        5. *Basil*\n\
        6. **Salt**";

        html_2md_compare(contents, expected);
    }

    #[test]
    fn test_bad_ol() {
        let contents = "<h3>Pizza Margherita Ingredients</h3>\
        <ol>\
            <li>Pizza Dough</li>\
            <li>Mozzarella cheese</li>\
            <li>Tomatoes</li>\
            <li>Olive Oil</li>\
            <div><i>Basil</i></div>\
            <li><b>Salt</b></li>\
        </ol>";

        let expected = "### Pizza Margherita Ingredients\n\n\
        1. Pizza Dough\n\
        2. Mozzarella cheese\n\
        3. Tomatoes\n\
        4. Olive Oil\n\
        \n*Basil*\n\n\
        5. **Salt**";

        html_2md_compare(contents, expected);
    }

    #[test]
    fn test_ol_inline() {
        let contents = "
        <ol>\
            <li>Item 1</li>\
            <li>Item 2</li>\
            <li>Item 3\
                <div>\
                    <ol>\
                        <li>Item 3-1</li>\
                        <li>Item 3-2</li>\
                        <li>Item 3-3\
                            <ol>\
                                <li>Item 3-3-1</li>\
                                <li>Item 3-3-2</li>\
                                <li>Item 3-3-3</li>\
                            </ol>
                        </li>\
                    </ol>\
                </div>
            </li>\
        </ol>";

        let expected = "\
1. Item 1
2. Item 2
3. Item 3
   1. Item 3-1
   2. Item 3-2
   3. Item 3-3
      1. Item 3-3-1
      2. Item 3-3-2
      3. Item 3-3-3";

        html_2md_compare(contents, expected);
    }

    #[test]
    fn test_ul_inline() {
        let contents = "
        <ul>\
            <li>Item 1</li>\
            <li>Item 2</li>\
            <li>Item 3\
                <div>\
                    <ul>\
                        <li>Item 3-1</li>\
                        <li>Item 3-2</li>\
                        <li>Item 3-3\
                            <ul>\
                                <li>Item 3-3-1</li>\
                                <li>Item 3-3-2</li>\
                                <li>Item 3-3-3</li>\
                            </ul>
                        </li>\
                    </ul>\
                </div>
            </li>\
        </ul>";

        let expected = "\
- Item 1
- Item 2
- Item 3
  - Item 3-1
  - Item 3-2
  - Item 3-3
    - Item 3-3-1
    - Item 3-3-2
    - Item 3-3-3";

        html_2md_compare(contents, expected);
    }

    #[test]
    fn test_list_with_paragraphs() {
        let contents = "<ol>
            <li>
                <p>Paragraph 1-1</p>
                <p>Paragraph 1-2</p>
            </li>
            <li><p>Paragraph 2-1</p><p>Paragraph 2-2</p></li>
            <li><p>Paragraph 3-1</p></li>
        </ol>
        <p>Another Paragraph</p>";

        let expected = "\
1. Paragraph 1-1

   Paragraph 1-2

2. Paragraph 2-1

   Paragraph 2-2

3. Paragraph 3-1

Another Paragraph";

        html_2md_compare(contents, expected);
    }

    #[test]
    fn test_nested_list_in_item_with_paragraphs() {
        html_2md_compare(
            "<ul><li><p>a</p><ul><li><p>b</p></li><li><p>c</p></li></ul></li></ul>",
            "- a\n\n  - b\n\n  - c",
        );
        // a tight nested list gets no hard break and stays tight
        html_2md_compare(
            "<ul><li><p>a</p><ul><li>b</li><li>c</li></ul></li></ul>",
            "- a\n\n  - b\n  - c",
        );
    }

    #[test]
    fn test_ordered_list_start_and_value() {
        html_2md_compare("<ol start=\"5\"><li>a</li><li>b</li></ol>", "5. a\n6. b");
        html_2md_compare(
            "<ol><li>a</li><li value=\"9\">b</li><li>c</li></ol>",
            "1. a\n9. b\n10. c",
        );
        // a non-numeric start falls back to the default numbering
        html_2md_compare("<ol start=\"x\"><li>a</li></ol>", "1. a");
        // HTML reads the leading integer and ignores the rest
        html_2md_compare("<ol start=\" 5abc\"><li>a</li></ol>", "5. a");
        html_2md_compare("<ol start=\"+3\"><li>a</li></ol>", "3. a");
        // Markdown numbers cannot be negative; 0 is the closest
        html_2md_compare("<ol start=\"-2\"><li>a</li><li>b</li></ol>", "0. a\n1. b");
        html_2md_compare("<ol><li>a</li><li value=\"-1\">b</li></ol>", "1. a\n0. b");
        // CommonMark markers hold at most nine digits, and the counter must
        // not overflow on huge values
        html_2md_compare(
            "<ol start=\"1234567890\"><li>a</li><li>b</li></ol>",
            "999999999. a\n999999999. b",
        );
        html_2md_compare(
            "<ol start=\"18446744073709551615\"><li>a</li></ol>",
            "999999999. a",
        );
        // huge values with leading zeros are clamped to the nine-digit limit
        html_2md_compare(
            "<ol start=\" 00000018446744073709551615\"><li>a</li></ol>",
            "999999999. a",
        );
        // leading zeros are stripped from the marker value
        html_2md_compare("<ol start=\"000000\"><li>a</li></ol>", "0. a");
        html_2md_compare(
            "<ol><li>a</li><li value=\"1234567890\">b</li></ol>",
            "1. a\n999999999. b",
        );
        // block syntax after a nine-digit marker is still escaped
        html_2md_compare(
            "<ol start=\"999999999\"><li>2)</li></ol>",
            "999999999. 2\\)",
        );
        html_2md_compare(
            "<ol start=\"123456789\"><li>- x</li></ol>",
            "123456789. \\- x",
        );
        html_2md_compare(
            "<ol start=\"123456789\"><li>a - x</li></ol>",
            "123456789. a - x",
        );
        html_2md_compare(
            "<ol start=\"999999998\"><li>a</li><li>2)</li></ol>",
            "999999998. a\n999999999. 2\\)",
        );
    }

    #[test]
    fn test_split_ordered_list_marker() {
        // digits at a line start and a `.` from the next node form a list marker
        html_2md_compare("<p><span>1</span>. foo</p>", "1\\. foo");
        html_2md_compare("<p>1<span>. not a list</span></p>", "1\\. not a list");
        html_2md_compare("<ul><li><span>2</span>) x</li></ul>", "- 2\\) x");
        html_2md_compare(
            "<ul><li>a<ul><li><span>3</span>. x</li></ul></li></ul>",
            "- a\n  - 3\\. x",
        );
        // the item's own marker is already written: `1. 3. x` would nest a list
        html_2md_compare("<ol><li><span>3</span>. x</li></ol>", "1. 3\\. x");
        html_2md_compare(
            "<ol start=\"12\"><li><span>3</span>) x</li></ol>",
            "12. 3\\) x",
        );
        // not a marker: mid-line digits, or a delimiter with no space after it
        html_2md_compare("<p><b>see</b> <span>1</span>. foo</p>", "**see** 1. foo");
        html_2md_compare("<p>In <span>2024</span>. Then</p>", "In 2024. Then");
        html_2md_compare("<p><span>1</span>.5 stars</p>", "1.5 stars");
        // not a marker: more than nine digits
        html_2md_compare("<p><span>10000000000</span>. foo</p>", "10000000000. foo");
    }

    #[test]
    fn test_nested_list_under_wide_marker() {
        // a child list must start at the parent item's content column; at a
        // fixed four columns it sits left of `100. ` and parses as an
        // indented code block
        let md = "100. a\n     1. x";
        html_2md_compare("<ol start=\"100\"><li>a<ol><li>x</li></ol></li></ol>", md);
        html_2md_compare(
            "<ol start=\"98\"><li>a</li><li>b</li><li>c<ul><li>n</li></ul></li></ol>",
            "98. a\n99. b\n100. c\n     - n",
        );
    }

    #[test]
    fn test_nested_list_not_starting_at_one() {
        // an ordered list that doesn't start at 1 can't interrupt a paragraph,
        // so it needs a blank line after the parent item's text
        html_2md_compare(
            "<ul><li>a<ol start=\"2\"><li>b</li></ol></li></ul>",
            "- a\n\n  2. b",
        );
        html_2md_compare(
            "<ul><li>a<ol><li value=\"3\">b</li></ol></li></ul>",
            "- a\n\n  3. b",
        );
    }

    #[test]
    fn test_nested_list_starting_with_empty_item() {
        // an empty item can't interrupt a paragraph, and a bare `-` under the
        // parent item's text would make that text a heading
        html_2md_compare(
            "<ul><li>a<ul><li></li><li>b</li></ul></li></ul>",
            "- a\n\n  -\n  - b",
        );
        html_2md_compare(
            "<ul><li>a<ul><li><ul><li>c</li></ul></li></ul></li></ul>",
            "- a\n\n  -\n    - c",
        );
    }

    #[test]
    fn test_nested_list_item_paragraphs() {
        // a nested item's second paragraph is indented to that item's content
        // column; at the outer item's column it parses as part of the outer item
        html_2md_compare(
            "<ul><li>a<ul><li><p>b</p><p>c</p></li></ul></li></ul>",
            "- a\n  - b\n\n    c",
        );
    }

    #[test]
    fn test_list_item_code_block() {
        // every line of the code block is indented to the item's content
        // column; at column zero the fence ends the list
        html_2md_compare(
            "<ul><li><p>a</p><pre><code>b\n\nc\n</code></pre><p>d</p></li></ul>",
            "- a\n\n  ```\n  b\n\n  c\n  ```\n\n  d",
        );
        html_2md_compare(
            "<ol start=\"10\"><li><pre><code>x\n</code></pre></li></ol>",
            "10. ```\n    x\n    ```",
        );
        html_2md_compare(
            "<ul><li>a<ul><li><p>b</p><pre><code>c\n</code></pre></li></ul></li></ul>",
            "- a\n  - b\n\n    ```\n    c\n    ```",
        );
        // after inline text the fence starts a new line
        html_2md_compare(
            "<ul><li>Run:<pre><code>make\n</code></pre></li></ul>",
            "- Run:\n  ```\n  make\n  ```",
        );
    }

    #[test]
    fn test_list_item_blockquote() {
        // every line of the quote is indented to the item's content column;
        // at column zero the quote ends the list
        html_2md_compare(
            "<ul><li><p>a</p><blockquote><p>b</p><p>c</p></blockquote><p>d</p></li></ul>",
            "- a\n\n  > b\n  > \n  > c\n\n  d",
        );
        html_2md_compare(
            "<ol start=\"10\"><li><blockquote><p>x</p><p>y</p></blockquote></li></ol>",
            "10. > x\n    > \n    > y",
        );
        // after inline text the quote starts a new line
        html_2md_compare(
            "<ul><li>a<blockquote><p>b</p></blockquote></li></ul>",
            "- a\n  > b",
        );
    }

    #[test]
    fn test_list_item_thematic_break() {
        html_2md_compare("<ul><li><hr></li><li>two</li></ul>", "-\n  ---\n- two");
        html_2md_compare(
            "<ul><li><p>a</p><hr><p>b</p></li><li>c</li></ul>",
            "- a\n\n  ---\n  b\n\n- c",
        );
        html_2md_compare("<ul><li>a<hr>b</li></ul>", "- a\n\n  ---\n  b");
        html_2md_compare(
            "<ol start=\"3\"><li><hr></li><li><p>a</p><hr></li></ol>",
            "3.\n   ---\n4. a\n\n   ---",
        );
        // a table cell has no line for the rule, but the break keeps the words apart
        html_2md_compare(
            "<table><tr><td><ul><li>a<hr>b</li></ul></td></tr></table>",
            "|   |\n| - |\n| + a<br>b |",
        );
    }

    #[test]
    fn test_list_tight_after_item_block() {
        // a blank line after an item's closing code block or quote would make
        // the list loose
        html_2md_compare(
            "<ul><li>a</li><li><pre><code>b\n</code></pre></li><li>c</li></ul>",
            "- a\n- ```\n  b\n  ```\n- c",
        );
        html_2md_compare(
            "<ul><li>a<blockquote><p>b</p></blockquote> <pre><code>c\n</code></pre> </li><li>d</li></ul>",
            "- a\n  > b\n  ```\n  c\n  ```\n- d",
        );
        html_2md_compare(
            "<ul><li><blockquote><p>a</p></blockquote></li><li>b</li></ul>",
            "- > a\n- b",
        );
        // two quotes in a row would merge without the blank line
        html_2md_compare(
            "<ul><li><blockquote><p>a</p></blockquote><blockquote><p>b</p></blockquote></li></ul>",
            "- > a\n\n  > b",
        );
        // text after the list would continue a quote that ends the last item
        html_2md_compare(
            "<ul><li>a</li><li><blockquote><p>b</p></blockquote></li></ul>c",
            "- a\n- > b\n\nc",
        );
    }

    #[test]
    fn test_list_item_heading() {
        // the heading keeps its marker, and text after it stays in the item
        html_2md_compare(
            "<ul><li><h1>Foo</h1></li><li><h2>Bar</h2>baz</li></ul>",
            "- # Foo\n- ## Bar\n  baz",
        );
        html_2md_compare(
            "<ol><li><h3>a #</h3><p>b</p></li><li>c</li></ol>",
            "1. ### a \\#\n   b\n\n2. c",
        );
    }

    #[test]
    fn test_list_item_nested_blocks() {
        // blocks inside a `<div>` in an item are written as the item's own blocks
        html_2md_compare(
            "<ul><li><div><p>a</p></div></li><li>two</li></ul>",
            "- a\n\n- two",
        );
        html_2md_compare(
            "<ul><li><div><p>a</p><p>b</p></div></li><li>two</li></ul>",
            "- a\n\n  b\n\n- two",
        );
        html_2md_compare(
            "<ol start=\"5\"><li><div><p>a</p></div></li><li>b</li></ol>",
            "5. a\n\n6. b",
        );
        html_2md_compare(
            "<ul><li><div><section><p>a</p><blockquote><p>b</p></blockquote></section></div></li></ul>",
            "- a\n\n  > b",
        );
        html_2md_compare(
            "<ul><li><div><p>a</p><!-- c --><p>b</p></div></li></ul>",
            "- a\n\n  b",
        );
        // whitespace between the blocks is not content
        html_2md_compare(
            "<ul><li><div>\n  <p>a</p>\n  <p>b</p>\n</div></li><li>two</li></ul>",
            "- a\n\n  b\n\n- two",
        );
    }

    #[test]
    fn test_list_item_nested_blocks_after_inline() {
        // a block after inline content starts its own indented line
        html_2md_compare("<ul><li>text<div><p>a</p></div></li></ul>", "- text\n  a");
        html_2md_compare(
            "<ul><li>text<div><h2>h</h2></div></li></ul>",
            "- text\n  ## h",
        );
        html_2md_compare(
            "<ul><li><a href=\"/u\">label</a><div><p>a</p></div></li></ul>",
            "- [label](/u)\n  a",
        );
        html_2md_compare(
            "<ul><li>text<br><div><p>a</p></div></li></ul>",
            "- text  \n  a",
        );
        // a block after inline text between blocks starts on a new line
        html_2md_compare(
            "<ul><li><div><p>a</p></div>tail<div><p>b</p></div></li></ul>",
            "- a\n\n  tail\n  b",
        );
    }

    #[test]
    fn test_list_item_nested_blocks_other_wrappers() {
        html_2md_compare(
            "<ul><li><article><p>a</p><p>b</p></article></li></ul>",
            "- a\n\n  b",
        );
        html_2md_compare(
            "<ul><li><div><pre>x\ny</pre></div></li></ul>",
            "- ```\n  x\n  y\n  ```",
        );
        // a wrapper holding a table or a rule is not unwrapped, but its blocks stay in the item
        html_2md_compare(
            "<ul><li><div><p>a</p><table><tr><td>t</td></tr></table></div></li></ul>",
            "- a\n\n  |   |\n  | - |\n  | t |",
        );
        html_2md_compare(
            "<ul><li><div><p>a</p><hr><p>b</p></div></li></ul>",
            "- a\n\n  ---\n\n  b",
        );
    }

    #[test]
    fn test_list_item_nested_blocks_skip_tags() {
        let doc = Document::from("<ul><li><div><section><p>a</p></section></div></li></ul>");
        assert_eq!(doc.md(Some(&["section"])).as_ref(), "-");
        let doc = Document::from("<ul><li><div><p>a</p></div></li></ul>");
        assert_eq!(doc.md(Some(&["p"])).as_ref(), "-");
    }

    #[test]
    fn test_list_item_text_after_block() {
        // unindented, the text would end the list and run into the next marker
        html_2md_compare(
            "<ul><li><p>a</p>b <em>c</em></li><li>d</li></ul>",
            "- a\n\n  b *c*\n- d",
        );
        html_2md_compare(
            "<ul><li><pre><code>a\n</code></pre>b</li><li>c</li></ul>",
            "- ```\n  a\n  ```\n\n  b\n- c",
        );
        // the text starts a line, so block syntax in it is escaped
        html_2md_compare(
            "<ul><li><p>a</p># b<br>+ c</li><li>+ d<p>e</p></li></ul>",
            "- a\n\n  \\# b  \n  \\+ c\n- \\+ d\n  e",
        );
    }

    #[test]
    fn test_list_item_block_syntax_text() {
        // text inside a list item that begins with block syntax must be
        // escaped, or it turns the line into a nested construct
        html_2md_compare("<ul><li>1. Preheat oven</li></ul>", "- 1\\. Preheat oven");
        html_2md_compare("<ul><li># tag</li></ul>", "- \\# tag");
        html_2md_compare("<ul><li>- x</li></ul>", "- \\- x");
        html_2md_compare("<ul><li>+ x</li></ul>", "- \\+ x");
        html_2md_compare("<ul><li>&gt; x</li></ul>", "- \\> x");
        html_2md_compare("<ol><li># tag</li></ol>", "1. \\# tag");
        // continuation blocks inside an item are indented, not at a raw line
        // start, and need the same protection
        html_2md_compare("<ul><li><p>a</p><p># b</p></li></ul>", "- a\n\n  \\# b");
        // plain inline content in the same positions stays unescaped
        html_2md_compare("<ul><li>a #b</li></ul>", "- a #b");
    }

    #[test]
    fn test_list_item_comment() {
        // a comment writes nothing, so it must not indent or space the next block
        html_2md_compare("<ul><li><p>a</p><!-- c --><p>b</p></li></ul>", "- a\n\n  b");
        html_2md_compare(
            "<ol><li><p>a</p><!-- c --><p>b</p></li></ol>",
            "1. a\n\n   b",
        );
        html_2md_compare(
            "<ul><li><pre>a</pre><!-- c --></li><li>b</li></ul>",
            "- ```\n  a\n  ```\n- b",
        );
        html_2md_compare(
            "<ul><li>x<ul><li><p>n</p><!-- c --><p>m</p></li></ul></li></ul>",
            "- x\n  - n\n\n    m",
        );
    }

    #[test]
    fn test_nbsp_is_not_collapsed() {
        // HTML collapses only ASCII whitespace; `&nbsp;` is content
        html_2md_compare("<p>a&nbsp;&nbsp;b</p>", "a\u{a0}\u{a0}b");
        html_2md_compare("<p>&nbsp;a&nbsp;</p>", "\u{a0}a\u{a0}");
        html_2md_compare("<p>a <span>&nbsp;</span> b</p>", "a \u{a0} b");
    }

    #[test]
    fn test_paragraphs() {
        let contents =
            "<p>To create paragraphs, use a blank line to separate one or more lines of text.</p>
        <p>I really like using <span>Markdown</span><span>  text</span>.</p>

        <p>I think I'll use it to format all of my documents from now on.</p>";

        let expected = "To create paragraphs, use a blank line to separate one or more lines of text.\n\n\
        I really like using Markdown text.\n\n\
        I think I'll use it to format all of my documents from now on.";

        html_2md_compare(contents, expected);
    }

    #[test]
    fn test_links() {
        let simple_contents = r#"<p>My favorite search engine is <a href="https://duckduckgo.com">Duck Duck Go</a>.</p>"#;
        let simple_expected =
            r"My favorite search engine is [Duck Duck Go](https://duckduckgo.com).";
        html_2md_compare(simple_contents, simple_expected);

        // link with title attribute
        let title_contents = r#"<p>My favorite search engine is <a href="https://duckduckgo.com" title="Duck Duck Go">Duck Duck Go</a>.</p>"#;
        let title_expected = r#"My favorite search engine is [Duck Duck Go](https://duckduckgo.com "Duck Duck Go")."#;
        html_2md_compare(title_contents, title_expected);

        let bold_contents = r#"<p>My favorite search engine is <b><a href="https://duckduckgo.com">Duck Duck Go</a></b>.</p>"#;
        let bold_expected =
            r"My favorite search engine is **[Duck Duck Go](https://duckduckgo.com)**.";
        html_2md_compare(bold_contents, bold_expected);

        // the link text keeps its formatting
        let bold_inside_contents = r#"<p>My favorite search engine is <a href="https://duckduckgo.com"><b>Duck Duck Go</b></a>.</p>"#;
        let bold_inside_expected =
            r"My favorite search engine is [**Duck Duck Go**](https://duckduckgo.com).";
        html_2md_compare(bold_inside_contents, bold_inside_expected);

        // html5ever transforms a > div to div > a, and there is no way to determine how it was.
        // This is an open question.
        let ignored_contents = r#"<p>My favorite search engine is <a href="https://duckduckgo.com"><div>Duck Duck Go</div></a>.</p>"#;
        let ignored_expected =
            "My favorite search engine is\n\n[Duck Duck Go](https://duckduckgo.com)

.";
        html_2md_compare(ignored_contents, ignored_expected);

        let no_href_contents = r"<p>My favorite search engine is <a>Duck Duck Go</a>.</p>";
        let no_href_expected = "My favorite search engine is Duck Duck Go.";
        html_2md_compare(no_href_contents, no_href_expected);

        let complex_contents =
            r#"<a href="https://duckduckgo.com" title="My &quot;Search&quot;">Duck Duck Go</a>"#;
        let comptex_expected = r#"[Duck Duck Go](https://duckduckgo.com "My \"Search\"")"#;
        html_2md_compare(complex_contents, comptex_expected);

        // avoid escaping links twice
        let content = r##"<p>See <a href="http://x">[Link]</a> and <a href="http://y">utm_source</a></p>
            <h2>What I wanted <a href="#w">#</a></h2>"##;
        let expected = concat!(
            "See [\\[Link\\]](http://x) and [utm_source](http://y)\n\n",
            "## What I wanted [#](#w)"
        );
        html_2md_compare(content, expected);
    }

    #[test]
    fn test_bang_before_link_or_image() {
        // a trailing `!` must not turn the link into image syntax
        html_2md_compare(
            r#"<p>Wow!<a href="https://e.com">x</a></p>"#,
            r"Wow\![x](https://e.com)",
        );
        // a backslash before it is text and does not escape the `!`
        html_2md_compare(
            r#"<p>Wow\!<a href="https://e.com">x</a></p>"#,
            r"Wow\\\![x](https://e.com)",
        );
        html_2md_compare(
            r#"<p>Wow!<a href="u"><img src="i.png" alt="a"></a></p>"#,
            r"Wow\![![a](i.png)](u)",
        );
        // emphasis merged into its parent or neighbour writes no marker in between
        html_2md_compare(
            r#"<p><b>Wow!<b><a href="u">x</a></b></b></p>"#,
            r"**Wow\![x](u)**",
        );
        html_2md_compare(
            r#"<p><b>Wow!</b><b><a href="u">x</a></b></p>"#,
            r"**Wow\![x](u)**",
        );
        html_2md_compare(r#"<p>Wow!<b><a href="u">x</a></b></p>"#, r"Wow!**[x](u)**");
    }

    #[test]
    fn test_images() {
        let simple_contents = r#"<p>Image: <img src="/path/to/img.jpg" alt="Alt text"></p>"#;
        let simple_expected = "Image: ![Alt text](/path/to/img.jpg)";
        html_2md_compare(simple_contents, simple_expected);

        // with title
        let simple_contents =
            r#"<p>Image: <img src="/path/to/img.jpg" alt="Alt text" title="Title"></p>"#;
        let simple_expected = r#"Image: ![Alt text](/path/to/img.jpg "Title")"#;
        html_2md_compare(simple_contents, simple_expected);

        // without alt
        let simple_contents = r#"<p>Image: <img src="/path/to/img.jpg"></p>"#;
        let simple_expected = r"Image: ![](/path/to/img.jpg)";
        html_2md_compare(simple_contents, simple_expected);

        // no img
        let simple_contents = r#"<p>Image:  <img alt="Alt text" title="Title"></p>"#;
        let simple_expected = "Image:";
        html_2md_compare(simple_contents, simple_expected);
    }

    #[test]
    fn test_img_src_fallbacks() {
        // lazy-load pages carry the image URL in srcset or data-src
        html_2md_compare(
            "<p><img srcset=\"https://i.e.com/p.png 1x, https://i.e.com/p2x.png 2x\" alt=\"pic\"></p>",
            "![pic](https://i.e.com/p.png)",
        );
        html_2md_compare(
            "<p><img srcset=\"https://i.e.com/p.png, https://i.e.com/p2x.png 2x\" alt=\"pic\"></p>",
            "![pic](https://i.e.com/p.png)",
        );
        html_2md_compare(
            "<p><img data-src=\"https://i.e.com/p.png\" alt=\"pic\"></p>",
            "![pic](https://i.e.com/p.png)",
        );
        // a real src still wins
        html_2md_compare(
            "<p><img src=\"https://i.e.com/s.png\" data-src=\"https://i.e.com/d.png\" alt=\"pic\"></p>",
            "![pic](https://i.e.com/s.png)",
        );
        // an empty placeholder src counts as missing
        html_2md_compare(
            "<p><img src=\"\" data-src=\"https://i.e.com/p.png\" alt=\"pic\"></p>",
            "![pic](https://i.e.com/p.png)",
        );
        // no URL anywhere: still dropped
        html_2md_compare("<p><img alt=\"pic\"></p>", "");
        html_2md_compare("<p><img src=\"\" alt=\"pic\"></p>", "");
    }

    #[test]
    fn test_whitespace_only_link() {
        html_2md_compare(
            "<h2>VMAF <a href=\"#vmaf\"> </a></h2><p>body</p>",
            "## VMAF\n\nbody",
        );
        // the words around it stay apart
        html_2md_compare("<p>x<a href=\"/p\"> </a>y</p>", "x y");
        // text after it at a line start is still escaped
        html_2md_compare("<p><a href=\"/p\"> </a># x</p>", "\\# x");
    }

    #[test]
    fn test_linked_image() {
        // an image wrapped in a link must become the link body, not be dropped
        html_2md_compare(
            r#"<p><a href="https://e.com"><img src="https://i.e.com/p.png" alt="pic"></a></p>"#,
            "[![pic](https://i.e.com/p.png)](https://e.com)",
        );
        // an image with a text sibling keeps both
        html_2md_compare(
            r#"<p><a href="https://e.com">see <img src="https://i.e.com/p.png" alt="pic"></a></p>"#,
            "[see ![pic](https://i.e.com/p.png)](https://e.com)",
        );
        // a genuinely empty link is still skipped
        html_2md_compare(r#"<p><a href="https://e.com"></a>x</p>"#, "x");

        let simple_contents =
            r#"<p>Image: <img src="/path/to/img.jpg" alt="Alt text" title='A "Great" Photo'></p>"#;
        let simple_expected = r#"Image: ![Alt text](/path/to/img.jpg "A \"Great\" Photo")"#;
        html_2md_compare(simple_contents, simple_expected);
    }

    #[test]
    fn test_link_wrapping_blocks() {
        // a blank line inside `[...]` ends the paragraph and breaks the link
        html_2md_compare(
            "<a href=\"/post\"><img src=\"a.jpg\" alt=\"i\"><h3>Title</h3><p>Blurb</p></a>",
            "[![i](a.jpg) Title Blurb](/post)",
        );
        html_2md_compare(
            "<a href=\"/p\"><img src=\"a.jpg\"><ul><li>a</li><li>b<ol><li>c</li></ol></li></ul></a>",
            "[![](a.jpg) a b c](/p)",
        );
        html_2md_compare(
            "<a href=\"/p\"><img src=\"a.jpg\"><blockquote><p>q</p><p>r</p></blockquote></a>",
            "[![](a.jpg) q r](/p)",
        );
        html_2md_compare(
            "<a href=\"/p\"><img src=\"a.jpg\"><table><tr><th>h</th><th>i</th></tr><tr><td>c</td><td>d</td></tr></table></a>",
            "[![](a.jpg) h i c d](/p)",
        );
        // code becomes a span; its `#` lines are text, not headings
        html_2md_compare(
            "<a href=\"/p\"><img src=\"a.jpg\"><pre><code class=\"language-rust\"># comment\n```\n# x</code></pre></a>",
            "[![](a.jpg) ````# comment ``` # x````](/p)",
        );
        // a hard break must not leave a blank line inside `[...]`, or in a table cell
        html_2md_compare(
            "<a href=\"/p\"><img src=\"a.jpg\">a<br><br>b</a>",
            "[![](a.jpg)a b](/p)",
        );
        html_2md_compare(
            "<table><tr><th>h</th></tr><tr><td><a href=\"/p\"><img src=\"a.jpg\"><h3>T</h3><p>B</p><br>x</a></td></tr></table>",
            "| h |\n| - |\n| [![](a.jpg) T B x](/p) |",
        );
    }

    #[test]
    fn test_link_destination_and_alt_escaping() {
        // balanced parens need no escaping
        html_2md_compare(
            "<p><a href=\"https://e.com/x(y)z\">x</a></p>",
            "[x](https://e.com/x(y)z)",
        );
        // unbalanced parens truncate the destination at the stray `)`
        html_2md_compare(
            "<p><a href=\"https://e.com/x)y\">x</a></p>",
            "[x](<https://e.com/x)y>)",
        );
        // a space turns the whole destination into plain text
        html_2md_compare(
            "<p><a href=\"https://e.com/a b.png\">x</a></p>",
            "[x](<https://e.com/a b.png>)",
        );
        // `>` alone is legal in a bare destination
        html_2md_compare(
            "<p><a href=\"https://e.com/a>b\">x</a></p>",
            "[x](https://e.com/a>b)",
        );
        // both brackets in the link
        html_2md_compare(
            r#"<p><a href="https://e.com/a<b>">x</a></p>"#,
            r"[x](<https://e.com/a\<b\>>)",
        );
        // brackets in alt text break the image
        html_2md_compare(
            "<p><img src=\"https://i.e.com/p.png\" alt=\"a [b] c\"></p>",
            "![a \\[b\\] c](https://i.e.com/p.png)",
        );
        // single '<'
        html_2md_compare(
            r#"<p><a href="https://e.com/a<b">x</a></p>"#,
            r"[x](<https://e.com/a\<b>)",
        );

        // '\n' (%0A)
        html_2md_compare(
            "<p><a href=\"https://e.com/a\nb\">x</a></p>",
            "[x](<https://e.com/a%0Ab>)",
        );

        // '\r' (%0D) — pass with &#13; или &#xD;
        html_2md_compare(
            r#"<p><a href="https://e.com/a&#13;b">x</a></p>"#,
            r"[x](<https://e.com/a%0Db>)",
        );

        // a backslash is doubled in either form: `\>` would escape the
        // wrapper's closing `>`, `\*` would lose the backslash
        html_2md_compare("<p><a href=\"a b\\\">x</a></p>", "[x](<a b\\\\>)");
        html_2md_compare("<p><a href=\"a\\*b\">x</a></p>", "[x](a\\\\*b)");
        // a tab also needs the wrapper
        html_2md_compare("<p><a href=\"a\tb\">x</a></p>", "[x](<a\tb>)");
        // alt text is escaped like other inline text
        html_2md_compare("<p><img src=\"i.png\" alt=\"a\\\"></p>", "![a\\\\](i.png)");
        html_2md_compare(
            "<p><img src=\"i.png\" alt=\"*y*\"></p>",
            "![\\*y\\*](i.png)",
        );
    }

    #[test]
    fn test_pre_code() {
        let simple_contents = "<pre>\
<span>fn</span> <span>main</span><span>()</span><span> </span><span>{</span>\n\
<span>    </span><span>println!</span><span>(</span><span>\"Hello, World!\"</span><span>);</span>\n\
<span>}</span>\
</pre>";
        let simple_expected = "```
fn main() {
    println!(\"Hello, World!\");
}
```";
        html_2md_compare(simple_contents, simple_expected);
    }

    #[test]
    fn test_pre_code_with_data_lang_attribute() {
        let simple_contents = "<pre data-lang=\"rust\">\
<span>fn</span> <span>main</span><span>()</span><span> </span><span>{</span>\n\
<span>    </span><span>println!</span><span>(</span><span>\"Hello, World!\"</span><span>);</span>\n\
<span>}</span>\n\
</pre>";
        let simple_expected = "```rust
fn main() {
    println!(\"Hello, World!\");
}
```";
        html_2md_compare(simple_contents, simple_expected);
    }

    #[test]
    fn test_pre_code_with_data_lang_attribute_in_parent_tag() {
        let simple_contents = "<div data-lang=\"rust\"><pre>\
<span>fn</span> <span>main</span><span>()</span><span> </span><span>{</span>\n\
<span>    </span><span>println!</span><span>(</span><span>\"Hello, World!\"</span><span>);</span>\n\
<span>}</span>\n\
</pre></div>";
        let simple_expected = "```rust
fn main() {
    println!(\"Hello, World!\");
}
```";
        html_2md_compare(simple_contents, simple_expected);
    }

    #[test]
    fn test_pre_code_with_language_css_class_in_child_code_tag() {
        let contents = "<pre><code class=\"language-rust something else\">\
<span>fn</span> <span>main</span><span>()</span><span> </span><span>{</span>\n\
<span>    </span><span>println!</span><span>(</span><span>\"Hello, World!\"</span><span>);</span>\n\
<span>}</span>\n\
</code></pre>";
        let expected = "```rust
fn main() {
    println!(\"Hello, World!\");
}
```";
        html_2md_compare(contents, expected);
    }

    #[test]
    fn test_pre_empty() {
        html_2md_compare("<pre><code></code></pre>", "```\n```");
    }

    #[test]
    fn test_blockquote() {
        let simple_contents = "<blockquote><p>Quoted text</p></blockquote>";
        let simple_expected = "> Quoted text";
        html_2md_compare(simple_contents, simple_expected);

        let complex_contents = "<blockquote>
<p>
Who has seen the wind?<br>
Neither I nor you:<br>
But when the leaves hang trembling,<br>
The wind is passing through.
</p>
<p>
Who has seen the wind?<br>
Neither you nor I:<br>
But when the trees bow down their heads,<br>
The wind is passing by.
</p>
</blockquote>
<p><i>Christina Rossetti</i></p>";
        let complex_expected = r"> Who has seen the wind?  
> Neither I nor you:  
> But when the leaves hang trembling,  
> The wind is passing through.
> 
> Who has seen the wind?  
> Neither you nor I:  
> But when the trees bow down their heads,  
> The wind is passing by.

*Christina Rossetti*";
        html_2md_compare(complex_contents, complex_expected);

        let empty_contents = "<blockquote></blockquote>";
        let empty_expected = "";
        html_2md_compare(empty_contents, empty_expected);
    }

    #[test]
    fn test_inline_blockquote() {
        let contents = "<blockquote>
<p>
Who has seen the wind?<br>
Neither I nor you:<br>
But when the leaves hang trembling,<br>
The wind is passing through.
</p>
<blockquote>
<p>
Who has seen the wind?<br>
Neither you nor I:<br>
But when the trees bow down their heads,<br>
The wind is passing by.
</p>
</blockquote>
</blockquote>";
        let expected = r"> Who has seen the wind?  
> Neither I nor you:  
> But when the leaves hang trembling,  
> The wind is passing through.
> 
> > Who has seen the wind?  
> > Neither you nor I:  
> > But when the trees bow down their heads,  
> > The wind is passing by.";
        html_2md_compare(contents, expected);
    }

    #[test]
    fn test_table() {
        let contents = "<table>
    <tr>
        <th>Column 1</th>
        <th>Column 2</th>
        <th>Column 3</th>
    </tr>
    <tr>
        <td>R 1, <i>C 1</i></td>
        <td>R 1, <i>C 2</i></td>
        <td>R 1, <i>C 3</i></td>
    </tr>
    <tr>
        <td>R 2, <i>C 1</i></td>
        <td>R 2, <i>C 2</i></td>
        <td>R 2, <i>C 3</i></td>
    </tr>
</table>";
        let expected = "| Column 1 | Column 2 | Column 3 |
| -------- | -------- | -------- |
| R 1, *C 1* | R 1, *C 2* | R 1, *C 3* |
| R 2, *C 1* | R 2, *C 2* | R 2, *C 3* |";

        html_2md_compare(contents, expected);
    }

    #[test]
    fn test_table_inside_table() {
        let contents = "<table>
    <tr>
        <td>
            <table>
                <tr>
                    <th>Column 1</th>
                    <th>Column 2</th>
                    <th>Column 3</th>
                </tr>
                <tr>
                    <td>R 1, <i>C 1</i></td>
                    <td>R 1, <i>C 2</i></td>
                    <td>R 1, <i>C 3</i></td>
                </tr>
                <tr>
                    <td>R 2, <i>C 1</i></td>
                    <td>R 2, <i>C 2</i></td>
                    <td>R 2, <i>C 3</i></td>
                </tr>
            </table>
        </td>
    </tr>
</table>";
        let expected = "| Column 1 | Column 2 | Column 3 |
| -------- | -------- | -------- |
| R 1, *C 1* | R 1, *C 2* | R 1, *C 3* |
| R 2, *C 1* | R 2, *C 2* | R 2, *C 3* |";
        html_2md_compare(contents, expected);
    }

    #[test]
    fn test_table_without_headings() {
        let contents = "<table>
    <tr>
        <td>R 1, <i>C 1</i></td>
        <td>R 1, <i>C 2</i></td>
        <td>R 1, <i>C 3</i></td>
    </tr>
    <tr>
        <td>R 2, <i>C 1</i></td>
        <td>R 2, <i>C 2</i></td>
        <td>R 2, <i>C 3</i></td>
    </tr>
</table>";
        let expected = "|   |   |   |
| - | - | - |
| R 1, *C 1* | R 1, *C 2* | R 1, *C 3* |
| R 2, *C 1* | R 2, *C 2* | R 2, *C 3* |";
        html_2md_compare(contents, expected);
    }

    #[test]
    fn test_table_short_row_padded() {
        let contents = "<table>
    <tr>
        <td>R 1, <i>C 1</i></td>
        <td>R 1, <i>C 2</i></td>
        <td>R 1, <i>C 3</i></td>
    </tr>
    <tr>
        <td>R 2, <i>C 1</i></td>
        <td>R 2, <i>C 2</i></td>
    </tr>
</table>";
        let expected = "|   |   |   |\n| - | - | - |\n| R 1, *C 1* | R 1, *C 2* | R 1, *C 3* |\n| R 2, *C 1* | R 2, *C 2* |  |";
        html_2md_compare(contents, expected);
    }

    #[test]
    fn test_table_empty() {
        let contents = "<table>
    <tr></tr>
    <tr></tr>
</table>";
        let expected = "";
        html_2md_compare(contents, expected);
    }

    #[test]
    fn test_table_with_list() {
        let contents = "<table>
    <tr>
        <td>1</td>
        <td>
            <ul><li>Lemon</li></ul>
            <ul><li>Lime</li></ul>
            <ul><li>Grapefruit</li></ul>
            <ul><li>Orange</li></ul>
        </td>
    </tr>
</table>";
        let expected =
            "|   |   |\n| - | - |\n| 1 | + Lemon<br>+ Lime<br>+ Grapefruit<br>+ Orange |";
        html_2md_compare(contents, expected);
    }

    #[test]
    fn test_table_cell_blocks() {
        // block children of a table cell are joined with the cell linebreak
        html_2md_compare(
            "<table><tr><th>H</th></tr><tr><td><p>para1</p><p>para2</p></td></tr></table>",
            "| H |\n| - |\n| para1<br>para2 |",
        );
    }

    #[test]
    fn test_table_cell_trailing_br() {
        html_2md_compare(
            "<table><tr><th>a</th></tr><tr><td>x<br></td></tr></table>",
            "| a |\n| - |\n| x |",
        );
        // text that reads `<br>` at the end of a cell is not a linebreak
        html_2md_compare(
            "<table><tr><th>H</th></tr><tr><td>use &lt;br&gt;</td></tr></table>",
            "| H |\n| - |\n| use \\<br> |",
        );
        // an escaped backslash does not escape the linebreak after it
        html_2md_compare(
            "<table><tr><th>H</th></tr><tr><td><p>a\\</p></td></tr></table>",
            "| H |\n| - |\n| a\\\\ |",
        );
        // an escaped `<br>` that ends a block does not swallow the next block's break
        html_2md_compare(
            "<table><tr><th>H</th></tr><tr><td><p>use &lt;br&gt;</p><p>x</p></td></tr></table>",
            "| H |\n| - |\n| use \\<br><br>x |",
        );
    }

    #[test]
    fn test_table_row_headers() {
        // a `th` beside a `td` labels its row, it isn't a column heading
        let contents = "<table>
    <tr><th>Lib</th><th>First</th></tr>
    <tr><th>Rustls</th><td>14</td></tr>
    <tr><th>BoringSSL</th><td>2</td></tr>
</table>";
        let expected = "| Lib | First |\n| --- | ----- |\n| Rustls | 14 |\n| BoringSSL | 2 |";
        html_2md_compare(contents, expected);

        let contents = "<table>
    <tr><th>Rustls</th><td>14</td></tr>
    <tr><th>BoringSSL</th><td>2</td></tr>
</table>";
        let expected = "|   |   |\n| - | - |\n| Rustls | 14 |\n| BoringSSL | 2 |";
        html_2md_compare(contents, expected);
    }

    #[test]
    fn test_table_headings_after_empty_row() {
        // the header row is the first row that has cells
        let contents = "<table>
    <tr></tr>
    <tr><th>A</th><th>B</th></tr>
    <tr><td>1</td><td>2</td></tr>
</table>";
        html_2md_compare(contents, "| A | B |\n| - | - |\n| 1 | 2 |");
    }

    #[test]
    fn test_table_only_headings() {
        let contents = "<table><tr><th>a</th><th>b</th></tr></table>";
        html_2md_compare(contents, "| a | b |\n| - | - |");
    }

    #[test]
    fn test_table_empty_th() {
        let contents = "<table>
        <tr>
            <th></th>
            <th>x</th>
          </tr>
        <tr>
            <td>a</td>
            <td>b</td>
            </tr>
    </table>";
        let expected = "|  | x |\n| - | - |\n| a | b |";
        html_2md_compare(contents, expected);
    }

    #[test]
    fn test_table_cell_escape() {
        // code with unescaped pipe, link with unescaped pipe
        let contents = r#"<table>
            <tr>
                <th>a</th>
            </tr>
            <tr>
                <td><code>a|b</code> <a href="http://x/a|b">l</a></td>
            </tr>
        </table>"#;
        let expected = "| a |\n| - |\n| `a\\|b` [l](http://x/a\\|b) |";
        html_2md_compare(contents, expected);

        // skipping escape table inside emphasis
        let contents = r"<table>
            <tr>
                <th><b>a|b</b></th>
            </tr>
            <tr>
                <td><i>c|d</i></td>
            </tr>
        </table>";
        let expected = "| **a\\|b** |\n| -------- |\n| *c\\|d* |";
        html_2md_compare(contents, expected);
    }

    #[test]
    fn test_skip_tags_default() {
        // By default, formatter will skip ["script", "style", "meta", "head"]
        let contents = "
        <style>p {color: blue;}</style>
        <p>I really like using <b>Markdown</b>.</p>

        <p>I think I'll use it to format all of my documents from now on.</p>";

        let expected = "I really like using **Markdown**.\n\n\
        I think I'll use it to format all of my documents from now on.";

        html_2md_compare(contents, expected);
    }

    #[test]
    fn test_skip_tags() {
        // If you need all text content of the elements, you need to pass Some(&vec![]) to `md`.
        // If you pass a structure like this into `Document::from`, the html5ever will create html > head > style.
        // If you want to preserve order use `Document::fragment`.
        let contents = "<style>p {color: blue;}</style>\
        <div><h1>Content Heading<h1></div>\
        <p>I really like using Markdown.</p>\
        <p>I think I'll use it to format all of my documents from now on.</p>";

        let expected = "p {color: blue;}\n\n\
        I really like using Markdown.\n\n\
        I think I'll use it to format all of my documents from now on.";

        let doc = Document::fragment(contents);
        let html_node = &doc.root();
        let md_text = serialize_md(html_node, false, Some(&["div"]));
        assert_eq!(md_text.as_ref(), expected);
    }

    #[test]
    fn test_text_after_list() {
        // on the next line the text would continue the last item
        html_2md_compare("<ul><li>a</li></ul>b", "- a\n\nb");
        html_2md_compare(
            "<ul><li>a</li></ul><pre><code>b\n</code></pre>",
            "- a\n\n```\nb\n```",
        );
        // a nested list that ends its item doesn't make the parent list loose
        html_2md_compare(
            "<ul><li>a<ul><li>b</li></ul></li><li>c</li></ul>d",
            "- a\n  - b\n- c\n\nd",
        );
    }
    #[test]
    fn test_linebreak_after_lists() {
        let contents = r"Influenced
        <ul>
         <li>Idris (programming language)</li>
         <li>Project Verona</li>
         <li>Spark</li>
         <li>Swift</li>
         <li>V</li>
         <li>Zig</li>
        </ul>
        <p><b>Rust</b> is a general-purpose programming language</p>";
        let expected = "Influenced\n\n\
- Idris (programming language)
- Project Verona
- Spark
- Swift
- V
- Zig

**Rust** is a general-purpose programming language";

        html_2md_compare(contents, expected);
    }

    #[test]
    fn test_pre_code_without_new_line() {
        let simple_contents = r#"<pre>
<span>fn</span> <span>main</span><span>()</span><span> </span><span>{</span>
<span>    </span><span>println!</span><span>(</span><span>"Hello, World!"</span><span>);</span>
<span>}</span></pre>"#;
        let simple_expected = "```
fn main() {
    println!(\"Hello, World!\");
}
```";
        html_2md_compare(simple_contents, simple_expected);
    }

    #[test]
    fn test_pre_with_interior_backticks() {
        // A fenced block whose content contains a fence-length backtick line
        // would terminate at that line; the fence must be longer than any
        // interior backtick run (CommonMark §fenced-code-blocks).
        html_2md_compare(
            "<pre><code>```bash\nls\n```</code></pre>",
            "````\n```bash\nls\n```\n````",
        );
        // a short interior run does not force a longer fence
        html_2md_compare("<pre><code>a`b</code></pre>", "```\na`b\n```");
    }

    #[test]
    fn test_empty_and_adjacent_code_spans() {
        // an empty code element writes nothing, instead of a stray backtick run
        html_2md_compare("<p>a<code></code>b</p>", "ab");
        // a whitespace-only one is a valid code span
        html_2md_compare("<p><code>  </code>x</p>", "` `x");
        html_2md_compare("<p>a<code></code>|<code> </code>b</p>", "a|` `b");
        // adjacent code spans are kept apart: `x``y` would be a single span
        let md = "`x` `y`";
        html_2md_compare("<p><code>x</code><code>y</code></p>", md);
    }

    #[test]
    fn test_code_span_with_elements() {
        // markup inside a code span has no meaning: nested elements give
        // their raw text, without backslash escapes or extra backticks
        html_2md_compare("<p><code><code>x</code></code></p>", "`x`");
        html_2md_compare("<p><code><a href=\"/u\">~</a></code></p>", "`~`");
        html_2md_compare("<p><code>a <b>*b*</b> c</code></p>", "`a *b* c`");
        html_2md_compare("<p><code>a<br>b</code></p>", "`a b`");
        html_2md_compare("<p><code>x_1<span>[y]</span></code></p>", "`x_1[y]`");
        // skipped tags stay skipped
        html_2md_compare("<p><code>a<script>s</script></code></p>", "`a`");
    }

    #[test]
    fn test_deeply_nested_code() {
        // each level used to rewrap the inner span in a longer fence, so
        // time was cubic and output quadratic in the depth; the exact output
        // pins both, without a wall-clock bound that could flake in CI
        let depth = 100;
        let html = format!(
            "<p>{}x{}</p>",
            "<code>".repeat(depth),
            "</code>".repeat(depth)
        );
        let doc = Document::from(html.as_str());
        let md = serialize_md(&doc.root(), false, None);
        assert_eq!(md.as_ref(), "`x`");
    }

    #[test]
    fn test_pre_with_trailing_newline() {
        // the content's final newline must not become an extra empty line
        let md = "```\na\nb\n```";
        html_2md_compare("<pre>a\nb\n</pre>", md);
    }

    #[test]
    fn test_emphasis_boundary_whitespace() {
        // CommonMark requires an opening delimiter to be followed, and a closing
        // delimiter to be preceded, by a non-whitespace character. Whitespace
        // staying inside the delimiter run renders as literal `**`/`*`.
        html_2md_compare(
            "<p><strong>The Rundown: </strong>Body text</p>",
            "**The Rundown:** Body text",
        );
        html_2md_compare(
            "<p><em> Leading</em> and <strong>trailing </strong></p>",
            "*Leading* and **trailing**",
        );
        // nested emphasis keeps working
        html_2md_compare(
            "<p><strong><em>both </em>bold</strong></p>",
            "***both* bold**",
        );
        // whitespace-only emphasis drops its delimiters instead of emitting an
        // unbalanced run
        html_2md_compare("<p>a<strong>   </strong>b</p>", "a b");
        html_2md_compare("Hello<strong> world</strong>.", "Hello **world**.");
        html_2md_compare("x<em> y</em>z", "x *y*z");
        html_2md_compare("a <em> x</em>", "a *x*");
        html_2md_compare("<p><strong> <em> x</em></strong></p>", "***x***");
        html_2md_compare("a<strong> <em> x</em></strong>", "a ***x***");
    }

    #[test]
    fn test_nested_same_type_emphasis() {
        // an element nested in one of the same type writes no delimiters:
        // `<i><i>b</i></i>` is one emphasis, not `**b**` (strong)
        html_2md_compare("<p><i><i>b</i></i></p>", "*b*");
        html_2md_compare("<p><b>a<span><b>b</b></span>c</b></p>", "**abc**");
        // a merge must not reopen the run with the inner element's opener,
        // which would close it right after `a`
        html_2md_compare("<p><em>a</em><em><i>b</i></em>c</p>", "*ab*c");

        html_2md_compare("<p><b>a</b><b><b>b</b> c</b></p>", "**ab c**");
        html_2md_compare("<p><b>a</b> <b> <b>b</b> c</b></p>", "**a b c**");
        // a different type nested inside still gets its own delimiters
        html_2md_compare("<p><b>a<i><b>b</b></i></b></p>", "**a*b***");
        html_2md_compare(r"<p><b>a*<i><b>b</b></i></b></p>", r"**a\**b***");
        html_2md_compare("<p><em>a</em><br><em>b</em></p>", "*a*  \n*b*");
        html_2md_compare(r"<p>a*b</p>", r"a\*b");

        html_2md_compare("<p><b>a</b><em>b</em></p>", "**a***b*");
        html_2md_compare("<p><b>a</b> <em>b</em></p>", "**a** *b*");
    }

    // ---- seltz cases ----
    #[test]
    fn ours_test_table_short_row_padded() {
        let contents = "<table>
    <tr>
        <td>R 1, <i>C 1</i></td>
        <td>R 1, <i>C 2</i></td>
        <td>R 1, <i>C 3</i></td>
    </tr>
    <tr>
        <td>R 2, <i>C 1</i></td>
        <td>R 2, <i>C 2</i></td>
    </tr>
</table>";
        let expected = "|   |   |   |\n| - | - | - |\n| R 1, *C 1* | R 1, *C 2* | R 1, *C 3* |\n| R 2, *C 1* | R 2, *C 2* |  |";
        html_2md_compare(contents, expected);
    }

    #[test]
    fn ours_test_emphasis_whitespace_moves_outside() {
        html_2md_compare(
            "<p>effort. <em>Test it out </em>– Never</p>",
            "effort. *Test it out* – Never",
        );
    }

    #[test]
    fn ours_test_emphasis_edge_punctuation_moves_out_only_where_needed() {
        html_2md_compare(
            "<p>a <b>retention,</b> b c<b>(x</b>y d<b>Note:</b>e</p>",
            "a **retention,** b c(**x**y d**Note**:e",
        );
    }

    #[test]
    fn ours_test_emphasis_edge_whitespace_and_punctuation() {
        html_2md_compare(
            "<p><strong>Does the above sound like you? – </strong>For your</p>",
            "**Does the above sound like you? –** For your",
        );
    }

    #[test]
    fn ours_test_emphasis_of_escaped_character_only() {
        html_2md_compare(
            "<p>diapers, or furniture<strong>*</strong></p>",
            "diapers, or furniture\\*",
        );
    }

    #[test]
    fn ours_test_emphasis_adjacent_same_kind_merge() {
        html_2md_compare(
            "<p><b>a</b><b>b</b> <i>c</i><i>d</i> <b>e</b><i>f</i></p>",
            "**ab** *cd* **e***f*",
        );
    }

    #[test]
    fn ours_test_emphasis_adjacent_different_kind_not_merged() {
        html_2md_compare(
            "<p><em>to him. </em><strong><sup>39</sup></strong><em>She had</em></p>",
            "*to him.* **39***She had*",
        );
    }

    #[test]
    fn ours_test_emphasis_repeated_nesting_merges_level_by_level() {
        html_2md_compare(
            "<p><em><strong>4+ stars H</strong></em><em><strong>otels</strong></em></p>",
            "***4+ stars H***_**otels**_",
        );
    }

    #[test]
    fn ours_test_emphasis_nested_same_kind_written_once() {
        html_2md_compare(
            "<p><strong><strong><em>I<strong><em><strong>TV1 / STV, </strong></em></strong>8.30<strong><em><strong>pm stream</strong></em></strong></em></strong></strong></p>",
            "***ITV1 / STV, 8.30pm stream***",
        );
    }

    #[test]
    fn ours_test_emphasis_across_blocks_applies_per_block() {
        html_2md_compare(
            "<strong><h2>Did you know?</h2><p><em>Toxic</em> markers</p></strong>",
            "## **Did you know?**\n\n***Toxic* markers**",
        );
    }

    #[test]
    fn ours_test_emphasis_across_list_items() {
        html_2md_compare(
            "<b><ul><li>first</li><li>second <i>it</i></li></ul></b>",
            "- **first**\n- **second *it***",
        );
    }

    #[test]
    fn ours_test_emphasis_hard_break_kept_after_marker() {
        html_2md_compare(
            "<p>price tag.</p><p><strong>Industry Context<br></strong></p><p>Cash</p>",
            "price tag.\n\n**Industry Context**  \n\nCash",
        );
    }

    #[test]
    fn ours_test_emphasis_around_code_span_and_link() {
        html_2md_compare(
            "<p><em>Inherits from <code>XRLayer</code>.</em> x <b>see <a href=\"/x\">link</a>.</b> y</p>",
            "*Inherits from `XRLayer`.* x **see [link](/x).** y",
        );
    }

    #[test]
    fn ours_test_no_emphasis_inside_code_span() {
        html_2md_compare(
            "<p>The <code><strong>reportError</strong>()</code> method</p>",
            "The `reportError()` method",
        );
    }

    #[test]
    fn ours_test_empty_code_span_skipped() {
        html_2md_compare("<p>Contact: <code></code> TG</p>", "Contact: TG");
    }

    #[test]
    fn ours_test_link_without_href_keeps_spaces() {
        html_2md_compare(
            "<p>Copyright © <a>2026 </a>Lead2Goals. See<a> the docs</a>, then <a><b>go </b></a>on</p>",
            "Copyright © 2026 Lead2Goals. See the docs, then **go** on",
        );
    }

    #[test]
    fn ours_test_link_without_href_keeps_blocks() {
        html_2md_compare(
            "<a><div>Facebook</div></a><a><div><i></i></div><div>Twitter</div></a>",
            "Facebook\n\nTwitter",
        );
    }

    #[test]
    fn ours_test_link_trailing_space_after_destination() {
        html_2md_compare(
            "<p>Copyright © <a href=\"/x\">2026 </a>Lead2Goals.</p>",
            "Copyright © [2026](/x) Lead2Goals.",
        );
    }

    #[test]
    fn ours_test_destination_with_spaces_in_angle_brackets() {
        html_2md_compare(
            "<p><a href=\"/a b\">x</a> <img src=\"background-image: url('https://x.net/a.jpg');\" alt=\"\"></p>",
            "[x](</a b>) ![](<background-image: url('https://x.net/a.jpg');>)",
        );
    }

    #[test]
    fn ours_test_nested_list_without_text_in_parent_item() {
        html_2md_compare(
            "<ul><li><ul><li>child</li><li>child2</li></ul></li><li>next</li></ul>",
            "-\n  - child\n  - child2\n- next",
        );
    }

    #[test]
    fn ours_test_nested_lists_indent_to_content_column() {
        html_2md_compare(
            "<ol><li><p>CONNECT</p><ul><li><p>SYDNEY</p><ul><li>EMAIL</li></ul></li></ul></li></ol>",
            "1. CONNECT\n\n   - SYDNEY\n\n     - EMAIL",
        );
    }

    #[test]
    fn ours_test_list_item_inline_then_block() {
        html_2md_compare(
            "<ol><li><span>01</span><h3>Focus, always</h3></li><li><span>02</span><h3>Design</h3></li></ol>",
            "1. 01\n   ### Focus, always\n2. 02\n   ### Design",
        );
    }

    #[test]
    fn ours_test_text_after_list_is_a_new_paragraph() {
        html_2md_compare(
            "<div><ul><li>a</li></ul>tail text</div>",
            "- a\n\ntail text",
        );
    }

    #[test]
    fn ours_test_definition_list_and_orphan_item() {
        html_2md_compare(
            "<dl><dt>Series:</dt><dd><a>Choir</a></dd><li>Event Category: Facility</li></dl>",
            "Series:\n\nChoir\n\n- Event Category: Facility",
        );
    }

    #[test]
    fn ours_test_html5_blocks_are_separated() {
        html_2md_compare(
            "<header>Top</header><main>Body</main><nav>Links</nav><footer>Bottom</footer>",
            "Top\n\nBody\n\nLinks\n\nBottom",
        );
    }

    #[test]
    fn ours_test_table_row_headers_are_cells() {
        html_2md_compare(
            "<table><tr><th>Weight</th><td>1 lbs</td></tr><tr><th>Dimensions</th><td>8.75 in</td></tr></table>",
            "|   |   |\n| - | - |\n| Weight | 1 lbs |\n| Dimensions | 8.75 in |",
        );
    }

    #[test]
    fn ours_test_table_caption_before_table() {
        html_2md_compare(
            "<table><caption><strong>Comparison</strong></caption><thead><tr><th>A</th><th>B</th></tr></thead><tbody><tr><td>1</td><td>2</td></tr></tbody></table>",
            "**Comparison**\n\n| A | B |\n| - | - |\n| 1 | 2 |",
        );
    }

    #[test]
    fn ours_test_table_with_unequal_rows_padded() {
        html_2md_compare(
            "<table><tr><td>550</td><td>390</td></tr><tr><td>x</td></tr></table><span>You may need</span>",
            "|   |   |\n| - | - |\n| 550 | 390 |\n| x |  |\n\nYou may need",
        );
    }

    #[test]
    fn ours_test_table_cell_blocks_inline() {
        html_2md_compare(
            "<table><tr><td><a><h3>2018 - Ballagás</h3></a></td><td><blockquote>A támogatók</blockquote></td></tr><tr><td><p>p1</p><p>p2</p></td><td><ul><li>a</li><li>b</li></ul></td></tr></table>",
            "|   |   |\n| - | - |\n| 2018 - Ballagás | A támogatók |\n| p1<br>p2 | + a<br>+ b |",
        );
    }

    #[test]
    fn ours_test_line_start_block_syntax_escaped() {
        html_2md_compare(
            "<p>valeurs U sont<br>- fenêtre simple</p><p>-5 degrees</p><p>==========</p><p>~~~~~~~~ text</p><p><em>- Friedrich Schiller</em></p>",
            "valeurs U sont  \n\\- fenêtre simple\n\n-5 degrees\n\n\\==========\n\n\\~\\~\\~\\~\\~\\~\\~\\~ text\n\n*\\- Friedrich Schiller*",
        );
    }

    #[test]
    fn ours_test_entities_and_backslashes_escaped() {
        html_2md_compare(
            "<p>lo sceglier&amp;ograve; di sicuro, AT&amp;T &amp; co, \\\"Kate\\\" and C:\\path</p>",
            "lo sceglier\\&ograve; di sicuro, AT&T & co, \\\\\"Kate\\\\\" and C:\\\\path",
        );
    }

    #[test]
    fn ours_test_multibyte_whitespace_trimmed() {
        html_2md_compare(
            "<p>a　</p><pre>b　</pre>",
            "a\u{3000}\n\n```\nb\u{3000}\n```",
        );
    }

    #[test]
    fn ours_test_emphasis_nested_edge_punctuation_before_letter() {
        html_2md_compare(
            "<p><b><i>Note:</i></b>e and <b><i>Note:</i></b> f</p>",
            "***Note***:e and ***Note:*** f",
        );
    }

    #[test]
    fn ours_test_emphasis_leading_punctuation_kept_after_space() {
        html_2md_compare(
            "<p><strong>[v1]</strong> and x<strong>[v2]</strong>y</p>",
            "**[v1\\]** and x[**v2**\\]y",
        );
    }

    #[test]
    fn ours_test_emphasis_back_to_back_different_kinds() {
        html_2md_compare(
            "<p><strong>Was <em>Monster Lai</em></strong><em>r</em>?</p>",
            "**Was *Monster Lai***_r_?",
        );
        // `_` cannot close before a letter: that emphasis keeps its text only
        html_2md_compare(
            "<p><strong>Was <em>Lai</em></strong><em>r</em>s</p>",
            "**Was *Lai***rs",
        );
    }

    #[test]
    fn ours_test_emphasis_next_to_zero_width_characters() {
        // A zero-width character is neither whitespace nor punctuation
        html_2md_compare(
            "<p>x&#8203;<i>–Nolan</i> and <b>«Οι».</b>&#8205;</p>",
            "x\u{200b}–*Nolan* and **«Οι**».\u{200d}",
        );
    }

    #[test]
    fn ours_test_emphasis_holds_hard_breaks() {
        html_2md_compare(
            "<p><strong>Hours<br>Mon - Fri</strong></p><p>Before <b><br>after:</b> x</p>",
            "**Hours  \nMon - Fri**\n\nBefore  \n**after:** x",
        );
    }

    #[test]
    fn ours_test_emphasis_after_emphasis_starting_with_space() {
        html_2md_compare(
            "<p><strong>1905:</strong><em> May Sutton</em> and <b>«a»</b>، و</p>",
            "**1905:** *May Sutton* and **«a»**، و",
        );
    }

    #[test]
    fn ours_test_heading_breaks_and_blocks_become_spaces() {
        html_2md_compare(
            "<h2>Let Us Fund<br>Your Next Project</h2><h3><p>Para</p><div>div</div></h3>",
            "## Let Us Fund Your Next Project\n\n### Para div",
        );
    }

    #[test]
    fn ours_test_title_skipped() {
        html_2md_compare("<div><title>Hidden</title><p>Body</p></div>", "Body");
    }

    #[test]
    fn ours_test_list_item_inline_words_not_line_start() {
        html_2md_compare(
            "<ul><li><span><i>e</i> = 2.71</span> and Vol.<span> </span>3. London</li></ul>",
            "- *e* = 2.71 and Vol. 3. London",
        );
        html_2md_compare("<ul><li>1. one</li></ul>", "- 1\\. one");
        html_2md_compare("<ul><li>= x</li></ul>", "- \\= x");
    }

    #[test]
    fn ours_test_table_spans_laid_out_on_grid() {
        html_2md_compare(
            "<table><tr><td colspan=\"2\">Wide</td><td rowspan=\"2\">R</td></tr><tr><td>a</td><td>b</td></tr></table>",
            "|   |   |   |\n| - | - | - |\n| Wide |  | R |\n| a | b |  |",
        );
    }

    #[test]
    fn ours_test_table_span_wider_than_any_row_of_cells() {
        // 4 columns, but no row has more than 2 cells: `y` starts after `b`
        html_2md_compare(
            "<table><tr><td colspan=\"3\">x</td><td>y</td></tr><tr><td colspan=\"2\">a</td><td colspan=\"2\">b</td></tr></table>",
            "|   |   |   |\n| - | - | - |\n| x |  | y |\n| a | b |  |",
        );
        // a span to fill the width adds no columns
        html_2md_compare(
            "<table><tr><th>A</th><th>B</th></tr><tr><td colspan=\"100\">x</td></tr></table>",
            "| A | B |\n| - | - |\n| x |  |",
        );
    }

    #[test]
    fn ours_test_one_blank_line_before_table_and_fence() {
        html_2md_compare(
            "<p>before</p><table><tr><th>A</th></tr><tr><td>1</td></tr></table><p>code:</p><pre>x</pre>",
            "before\n\n| A |\n| - |\n| 1 |\n\ncode:\n\n```\nx\n```",
        );
    }

    #[test]
    fn ours_test_table_header_only() {
        html_2md_compare(
            "<table><tr><th>A</th><th>B</th></tr></table>",
            "| A | B |\n| - | - |",
        );
    }

    #[test]
    fn ours_test_list_inside_list_belongs_to_item_before() {
        html_2md_compare(
            "<ul><li>A</li><ul><li>B</li><li>C</li></ul><li>D</li></ul>",
            "- A\n  - B\n  - C\n- D",
        );
    }

    #[test]
    fn ours_test_items_wrapped_inside_list() {
        html_2md_compare(
            "<ul><div><li>a</li><li>b</li></div><a><li>c</li></a></ul>",
            "- a\n- b\n- c",
        );
    }

    #[test]
    fn ours_test_text_inside_list_set_apart() {
        html_2md_compare(
            "<ul><li>Job</li>Apply for job</ul><p>after</p>",
            "- Job\n\nApply for job\n\nafter",
        );
    }

    #[test]
    fn ours_test_inline_content_inside_list_stays_together() {
        html_2md_compare(
            "<ul><p>TOPICS:</p><a>Amy Acton</a><a>Vivek</a><li>x</li><b>a:</b>c</ul>",
            "TOPICS:\n\nAmy ActonVivek\n\n- x\n\n**a**:c",
        );
    }

    #[test]
    fn ours_test_heading_items_become_words() {
        html_2md_compare(
            "<h6><ul><li><a>Career</a></li><span>|</span><li><a>Contact us</a></li></ul></h6>",
            "###### Career | Contact us",
        );
    }

    #[test]
    fn ours_test_emphasis_merge_keeps_inner_run_valid() {
        html_2md_compare(
            "<p>needed: <em><strong>Luke 10:42 …</strong></em><em>but few.</em></p>",
            "needed: ***Luke 10:42*** …_but few._",
        );
    }

    #[test]
    fn ours_test_emphasis_spanning_blocks_applies_to_inline_runs() {
        html_2md_compare(
            "<b><div>intro <i>it</i> text<p>para</p>tail</div></b>",
            "**intro *it* text**\n\n**para**\n\n**tail**",
        );
        html_2md_compare("<b><h2><p>x</p></h2></b>", "## **x**");
    }

    #[test]
    fn ours_test_emphasis_with_breaks_at_edges_in_cell() {
        html_2md_compare(
            "<table><tr><td><strong>Chest <br></strong></td><td><b><br>Annual<br>Report</b></td></tr></table>",
            "|   |   |\n| - | - |\n| **Chest** | <br>**Annual  <br>Report** |",
        );
    }

    #[test]
    fn ours_test_svg_skipped() {
        html_2md_compare(
            "<div><svg><text>0204060</text></svg><p>after</p></div>",
            "after",
        );
    }

    #[test]
    fn ours_test_pre_breaks_and_blocks_kept() {
        html_2md_compare(
            "<pre><div>line one.</div><div>line two</div>tail<br>• a</pre>",
            "```\nline one.\nline two\ntail\n• a\n```",
        );
        // a `<pre>` holding paragraphs is a styled container, not code
        html_2md_compare(
            "<pre><p>For the cat lover!</p><p><strong>By: </strong>Ross</p></pre>",
            "For the cat lover!\n\n**By:** Ross",
        );
    }

    #[test]
    fn ours_test_thematic_break_in_item() {
        html_2md_compare(
            "<ul><li>x</li><li><hr></li><li>y</li></ul>",
            "- x\n-\n  ---\n- y",
        );
    }

    #[test]
    fn ours_test_list_item_blocks_in_link_without_href() {
        // the paragraph would otherwise leave the item, and the nested list
        // after it would become a code block
        html_2md_compare(
            "<ul><li><a><p>a</p></a><ul><li><a><p>b</p></a></li></ul></li></ul>",
            "- a\n\n  - b",
        );
    }

    #[test]
    fn ours_test_punctuation_emphasis_continues_the_one_before() {
        html_2md_compare(
            "<p><a><b>Data Lineage</b></a><b>:</b> x</p>",
            "**Data Lineage:** x",
        );
        // moved out before a letter, where the closing marker would not close
        html_2md_compare("<p><b>a</b><b>:</b>x</p>", "**a**:x");
    }

    #[test]
    fn ours_test_emphasis_not_merged_into_longer_run() {
        // merging `**` into a closing `***` leaves runs CommonMark pairs wrongly
        html_2md_compare(
            "<p><b><em>a </em></b><a><b><em>b</em></b></a><b><em> c</em></b></p>",
            "***a*** ***b*** ***c***",
        );
    }

    #[test]
    fn ours_test_emphasis_punctuation_moved_out_across_link_without_href() {
        html_2md_compare("<p><a><em>x (y)</em></a><a>Follow</a></p>", "*x (y*)Follow");
    }

    #[test]
    fn ours_test_list_item_inline_wrapper_and_empty_blocks() {
        // an empty block does not take the item's first line
        html_2md_compare(
            "<ul><li><figure> </figure>a</li><li>b</li></ul>",
            "- a\n- b",
        );
        // blocks inside an inline element stay in the item
        html_2md_compare(
            "<ul><li><label><p>a</p><span>b</span></label></li><li>c</li></ul>",
            "- a\n\n  b\n- c",
        );
        // a list opening the item follows its marker with no blank line
        html_2md_compare(
            "<ul><li><div><ol start=\"3\"><li>c</li></ol></div></li></ul>",
            "- 3. c",
        );
    }

    #[test]
    fn ours_test_words_kept_apart_at_line_breaks_and_cells() {
        // a line break opening an emphasis in a heading
        html_2md_compare(
            "<h2>Round 2<strong><br>Navigating</strong></h2>",
            "## Round 2 **Navigating**",
        );
        // the cells of a table holding a table, which is not written as a table
        html_2md_compare(
            "<table><tr><td>a</td><td>b</td></tr><tr><td><table><tr><td>c</td></tr></table></td></tr></table>",
            "a b  \n\n|   |\n| - |\n| c |",
        );
    }

    #[test]
    fn ours_test_line_start_escaped_after_whitespace() {
        // whitespace before the text is trimmed, leaving it at the line start
        html_2md_compare(
            "<ol><li>a</li>\n <span>&gt;</span>\n <li>b</li></ol>",
            "1. a\n\n\\>\n\n2. b",
        );
    }

    #[test]
    fn ours_test_review_fixes() {
        // punctuation-only emphasis after digits does not form a list marker
        html_2md_compare("<p>1<em>.</em></p>", "1\\.");
        html_2md_compare("<p>1<b>)</b> a</p>", "1\\) a");
        // emphasis around a code span or link needs room for its markers
        html_2md_compare("<p>a<b><code>c</code></b> d</p>", "a`c` d");
        html_2md_compare("<p><b>x<code>a</code></b>y</p>", "x`a`y");
        html_2md_compare("<p>a <b><code>c</code></b> d</p>", "a **`c`** d");
        // a nested emphasis of the same kind still moves punctuation out
        html_2md_compare("<p><i>Note <b>x.</b><i>y</i></i>z</p>", "*Note **x**.y*z");
        // emphasis around blocks keeps them in the item
        html_2md_compare(
            "<ul><li><b><p>a</p><p>b</p></b></li></ul>",
            "- **a**\n\n  **b**",
        );
        // a code block in a cell keeps the table one row per line
        html_2md_compare(
            "<table><tr><td>x<pre>a\nb</pre></td></tr></table>",
            "|   |\n| - |\n| x<br>`a`<br>`b` |",
        );
    }

    #[test]
    fn ours_test_spec_round_trip_gaps() {
        // emphasis around punctuation keeps its markers where they parse
        html_2md_compare("<p><em>-</em></p>", "*\\-*");
        html_2md_compare("<p>foo <strong>_</strong></p>", "foo **\\_**");
        html_2md_compare("<p>x <b>(</b>y</p>", "x (y");
        // emoji count as punctuation, as in CommonMark 0.31
        html_2md_compare("<p><b>📝</b>Please</p>", "📝Please");
        html_2md_compare("<p>a <b>😊 b</b> c</p>", "a **😊 b** c");
        // punctuation emphasized twice after a letter loses both
        html_2md_compare("<p>aniya<strong><em>.</em></strong></p>", "aniya.");
        // and emphasis holding nothing writes no markers
        html_2md_compare(
            "<table><tr><td>2010<i><br></i> - x</td></tr></table>",
            "|   |\n| - |\n| 2010<br> - x |",
        );
        // a list right after a list of the same kind writes the other marker
        html_2md_compare(
            "<ul><li>a</li></ul><ul><li>b</li></ul><ul><li>c</li></ul>",
            "- a\n\n+ b\n\n- c",
        );
        html_2md_compare(
            "<ol><li>a</li></ol><ol start=\"2\"><li>b</li></ol>",
            "1. a\n\n2) b",
        );
        // a list opening an item follows its marker with no blank line
        html_2md_compare(
            "<ol><li><ul><li><ol start=\"2\"><li>foo</li></ol></li></ul></li></ol>",
            "1.\n   -\n     2. foo",
        );
        // a code language is kept as written, with what a fence cannot hold dropped
        html_2md_compare(
            "<pre><code class=\"language-föö\">foo</code></pre>",
            "```föö\nfoo\n```",
        );
        html_2md_compare("<pre><code class=\"language-;\"></code></pre>", "```;\n```");
        // an escaped backtick does not merge with the code span after it
        html_2md_compare("<p>a`<code>b</code></p>", "a\\``b`");
    }

    #[test]
    fn ours_test_strikethrough_tasks_and_link_formatting() {
        html_2md_compare(
            "<p><del>Hi</del> Hello, <s>$20</s> $15</p>",
            "~~Hi~~ Hello, ~~$20~~ $15",
        );
        html_2md_compare(
            "<ul><li><input type=\"checkbox\" checked> done</li><li><input type=\"checkbox\"> todo</li></ul>",
            "- [x] done\n- [ ] todo",
        );
        // a checkbox outside a list item has no task syntax
        html_2md_compare("<p><input type=\"checkbox\"> x</p>", "x");
        html_2md_compare(
            "<p>See <a href=\"/api\"><code>Vec::new</code></a> and <a href=\"/x\"><em>the guide</em></a>.</p>",
            "See [`Vec::new`](/api) and [*the guide*](/x).",
        );
    }

    #[test]
    fn ours_test_review_round_two() {
        // a removable closing run inside another is removed with it
        html_2md_compare("<p>x <b><i>-</i></b>y</p>", "x \\-y");
        html_2md_compare("<p>x <del><b>-</b></del>y</p>", "x \\-y");
        // a run closing inside another moves its punctuation out of both,
        // across links too
        html_2md_compare(
            "<p>(“<a><b><i>AI</i></b></a><b><i>” and “</i></b><a><b><i>D</i></b></a><b><i>”).</i></b> W</p>",
            "(“***AI***__*” and*__ “***D***”). W",
        );
        // `~~` and `*` runs never read as one
        html_2md_compare("<p><del><i>x</i></del>y</p>", "*x*y");
        html_2md_compare("<p><i><del>x</del></i>y</p>", "~~x~~y");
        html_2md_compare("<p>a <del><b>x</b></del> b</p>", "a ~~**x**~~ b");
        // an emoji joined by a zero-width joiner is punctuation too
        html_2md_compare("<p>x <b>👨\u{200d}👩</b>y</p>", "x 👨\u{200d}👩y");
        // a code span ending in a backslash does not escape the next one
        html_2md_compare("<p><code>a\\</code><code>b</code></p>", "` a\\ ` `b`");
        // a link inside a link is written as its text
        html_2md_compare(
            "<p><a href=\"/1\">x<table><tr><td><a href=\"/2\">y</a></td></tr></table></a></p>",
            "[x y](/1)",
        );
        // a link's text is past any line start, and only shown text spaces it
        html_2md_compare("<p><a href=\"/u\">1. x</a></p>", "[1. x](/u)");
        html_2md_compare("<p><a href=\"/u\">x<script> </script></a>y</p>", "[x](/u)y");
        // a checkbox is a task only right after an item's marker, outside cells
        html_2md_compare(
            "<ul><li>x <input type=checkbox> a</li><li><input type=checkbox> c</li></ul>",
            "- x a\n- [ ] c",
        );
        // text in any script may come before a list
        html_2md_compare("<p>تولّت ส ©</p><ul><li>a</li></ul>", "تولّت ส ©\n\n- a");
        // an empty list sets nothing apart
        html_2md_compare(
            "<ul><li>a</li></ul><ul></ul><ul><li>c</li></ul>",
            "- a\n\n+ c",
        );
    }

    #[test]
    fn ours_test_item_start_and_orphan_items() {
        // a line break or spaces opening an item do not move its text off the
        // marker's line or content column
        html_2md_compare(
            "<ul><li><i></i><br><span>Schedule</span></li><li><br>Guests</li></ul>",
            "- Schedule\n- Guests",
        );
        html_2md_compare(
            "<ul><li><span><i> </i></span><span>Op:</span></li><ul><li>Reg</li></ul></ul>",
            "- Op:\n  - Reg",
        );
        // an item outside a list indents its blocks under it
        html_2md_compare(
            "<li><strong>Definitions</strong><ol><li>TPML means</li></ol></li>",
            "- **Definitions**\n  1. TPML means",
        );
    }

    #[test]
    fn ours_test_nested_writes_keep_saved_offsets() {
        // a nested write rewriting bytes before an item's saved offsets
        for html in [
            "<b><ul><li><ul><li><p>x　</p></li></ul></li></ul></b>",
            "<del><li><ul><li>\u{200d}<h2>👧</h2>",
            "<b><ul><li><ul><li><p>日</p><hr></li></ul></li></ul></b>",
            "<ul><li><pre><ul><li><p>x　</p></li></ul></pre></li></ul>",
            "<ul><li><em><table></table></em></li></ul>",
        ] {
            let _ = Document::from(html).md(None);
        }
        html_2md_compare(
            "<ul><li><b>x</b><b>日<p>b</p></b></li></ul>",
            "- **x日**\n\n  **b**",
        );
        // emphasis around a list applies to each item's text
        html_2md_compare(
            "<b><ul><li>a<ul><li>x</li></ul></li><li>b</li></ul></b>",
            "- **a**\n  - **x**\n- **b**",
        );
    }

    #[test]
    fn ours_test_no_merge_before_inner_run_on_punctuation() {
        // merged into the bold before it, the italic would open after a letter
        html_2md_compare(
            "<p><a><strong>Flowers</strong></a><strong><em>, which will.</em></strong></p>",
            "**Flowers**__*, which will.*__",
        );
    }
}
