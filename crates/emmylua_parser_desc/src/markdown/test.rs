#[cfg(test)]
mod tests {
    use crate::markdown::MarkdownParser;
    #[allow(unused)]
    use crate::testlib::{print_result, test};
    use googletest::prelude::*;

    #[gtest]
    fn test_md() -> Result<()> {
        let code = r#"
--- # Inline code
---
--- `code`
--- `` code ` with ` backticks ``
--- `code``with``backticks`
--- `broken code
--- [link]
--- [link `with backticks]`]
--- [link [with brackets] ]
--- [link](explicit_href)
--- [link](explicit()href)
--- [link](<explicit)href>)
--- Paragraph with `code`!
--- Paragraph with [link]!
--- \` escaped backtick
--- *em* em*in*text
--- _em_ em_in_text
--- **strong** strong**in**text
--- __strong__ strong__in__text
--- broken *em
--- broken em*
--- broken **strong
--- broken strong**
--- ***both*** both***in***text
--- ***both end* separately**
--- ***both end** separately*
--- *both **start separately***
--- **both *start separately***
--- *`foo`*
---
--- # Blocks
---
--- ## Thematic breaks
---
--- - - -
--- _ _ _
--- * * *
--- - _ -
---
--- ## Lists
---
--- - List
--- * List 2
--- + List 3
--- -Broken list
---
--- -    List with indented text
---
--- -     List with code
---       Continuation
---
---       Continuation 2
---
--- -测试 <- not a list
---
--- - 测试 <- list
---
--- -
---   List that starts with empty string
---
---  Not list
---
---   -  not code
---
---         still not code
---
---     code
---
--- ## Numbered lists
---
--- 1. List
--- 2: List
--- 3) List
---   Not list
---
--- ## Code
---
---     This is code
---      This is also code
---       function foo() end
---
--- ## Fenced code
---
--- ```syntax
--- code
--- ```
--- not code
--- ~~~syntax
--- code
--- ```
--- still code
--- ~~~
---
--- ````code with 4 fences
--- ```
--- ````
---
--- ```inline code```
--- not code
---
--- ```lua
--- function foo()
---     local long_string = [[
---         content
---     ]]
--- end
--- ```
---
--- ## Quotes
---
--- > Quote
--- > Continues
---
--- > Quote 2
---
--- ## Disabled MySt extensions
---
--- $$
--- math
--- $$
---
--- ```{directive}
--- ```
---
--- ## Link anchor
---
--- [link]: https://example.com
"#;
        let expected = r#"
--- <Scope><Markup>#</Markup> Inline code</Scope>
---
--- <Markup>`</Markup><Code>code</Code><Markup>`</Markup>
--- <Markup>``</Markup><Code> code ` with ` backticks </Code><Markup>``</Markup>
--- <Markup>`</Markup><Code>code``with``backticks</Code><Markup>`</Markup>
--- `broken code
--- <Link>[link]</Link>
--- <Link>[link `with backticks]`]</Link>
--- <Link>[link [with brackets] ]</Link>
--- <Link>[link](explicit_href)</Link>
--- <Link>[link](explicit()href)</Link>
--- <Link>[link](<explicit)href>)</Link>
--- Paragraph with <Markup>`</Markup><Code>code</Code><Markup>`</Markup>!
--- Paragraph with <Link>[link]</Link>!
--- <Markup>\`</Markup> escaped backtick
--- <Em><Markup>*</Markup>em<Markup>*</Markup></Em> em<Em><Markup>*</Markup>in<Markup>*</Markup></Em>text
--- <Em><Markup>_</Markup>em<Markup>_</Markup></Em> em_in_text
--- <Strong><Markup>**</Markup>strong<Markup>**</Markup></Strong> strong<Strong><Markup>**</Markup>in<Markup>**</Markup></Strong>text
--- <Strong><Markup>__</Markup>strong<Markup>__</Markup></Strong> strong__in__text
--- broken *em
--- broken em*
--- broken **strong
--- broken strong**
--- <Em><Strong><Markup>***</Markup>both<Markup>***</Markup></Strong></Em> both<Em><Strong><Markup>***</Markup>in<Markup>***</Markup></Strong></Em>text
--- <Em><Strong><Markup>***</Markup>both end<Markup>*</Markup></Strong></Em><Strong> separately<Markup>**</Markup></Strong>
--- <Em><Strong><Markup>***</Markup>both end<Markup>**</Markup></Strong></Em><Em> separately<Markup>*</Markup></Em>
--- <Em><Markup>*</Markup>both <Strong><Markup>**</Markup>start separately<Markup>***</Markup></Strong></Em>
--- <Strong><Markup>**</Markup>both <Em><Markup>*</Markup>start separately<Markup>***</Markup></Em></Strong>
--- <Em><Markup>*</Markup><Markup>`</Markup><Code>foo</Code><Markup>`</Markup><Markup>*</Markup></Em>
---
--- <Scope><Markup>#</Markup> Blocks</Scope>
---
--- <Scope><Markup>##</Markup> Thematic breaks</Scope>
---
--- <Scope><Markup>-</Markup> <Markup>-</Markup> <Markup>-</Markup></Scope>
--- <Scope><Markup>_</Markup> <Markup>_</Markup> <Markup>_</Markup></Scope>
--- <Scope><Markup>*</Markup> <Markup>*</Markup> <Markup>*</Markup></Scope>
--- <Scope><Markup>-</Markup> _ -
---
--- </Scope><Scope><Markup>##</Markup> Lists</Scope>
---
--- <Scope><Markup>-</Markup> List
--- </Scope><Scope><Markup>*</Markup> List 2
--- </Scope><Scope><Markup>+</Markup> List 3
--- </Scope>-Broken list
---
--- <Scope><Markup>-</Markup>    List with indented text
---
--- </Scope><Scope><Markup>-</Markup> <Scope>    <CodeBlock>List with code</CodeBlock>
---       <CodeBlock>Continuation</CodeBlock>
---
---       <CodeBlock>Continuation 2</CodeBlock>
---
--- </Scope></Scope>-测试 <- not a list
---
--- <Scope><Markup>-</Markup> 测试 <- list
---
--- </Scope><Scope><Markup>-</Markup>
---   List that starts with empty string
---
--- </Scope> Not list
---
--- <Scope>  <Markup>-</Markup>  not code
---
---         still not code
---
--- </Scope><Scope>    <CodeBlock>code</CodeBlock>
---
--- </Scope><Scope><Markup>##</Markup> Numbered lists</Scope>
---
--- <Scope><Markup>1.</Markup> List
--- </Scope><Scope><Markup>2:</Markup> List
--- </Scope><Scope><Markup>3)</Markup> List
--- </Scope>  Not list
---
--- <Scope><Markup>##</Markup> Code</Scope>
---
--- <Scope>    <CodeBlock>This is code</CodeBlock>
---     <CodeBlock> This is also code</CodeBlock>
---     <CodeBlock>  function foo() end</CodeBlock>
---
--- </Scope><Scope><Markup>##</Markup> Fenced code</Scope>
---
--- <Scope><Markup>```</Markup><CodeBlock>syntax</CodeBlock>
--- <CodeBlock>code</CodeBlock>
--- <Markup>```</Markup></Scope>
--- not code
--- <Scope><Markup>~~~</Markup><CodeBlock>syntax</CodeBlock>
--- <CodeBlock>code</CodeBlock>
--- <CodeBlock>```</CodeBlock>
--- <CodeBlock>still code</CodeBlock>
--- <Markup>~~~</Markup></Scope>
---
--- <Scope><Markup>````</Markup><CodeBlock>code with 4 fences</CodeBlock>
--- <CodeBlock>```</CodeBlock>
--- <Markup>````</Markup></Scope>
---
--- <Markup>```</Markup><Code>inline code</Code><Markup>```</Markup>
--- not code
---
--- <Scope><Markup>```</Markup><CodeBlock>lua</CodeBlock>
--- <CodeBlockHl(Keyword)>function</CodeBlockHl(Keyword)> <CodeBlockHl(Function)>foo</CodeBlockHl(Function)><CodeBlockHl(Operators)>()</CodeBlockHl(Operators)>
---     <CodeBlockHl(Keyword)>local</CodeBlockHl(Keyword)> <CodeBlockHl(Variable)>long_string</CodeBlockHl(Variable)> <CodeBlockHl(Operators)>=</CodeBlockHl(Operators)> <CodeBlockHl(String)>[[</CodeBlockHl(String)>
--- <CodeBlockHl(String)>        content</CodeBlockHl(String)>
--- <CodeBlockHl(String)>    ]]</CodeBlockHl(String)>
--- <CodeBlockHl(Keyword)>end</CodeBlockHl(Keyword)>
--- <Markup>```</Markup></Scope>
---
--- <Scope><Markup>##</Markup> Quotes</Scope>
---
--- <Scope><Markup>></Markup> Quote
--- <Markup>></Markup> Continues
---</Scope>
--- <Scope><Markup>></Markup> Quote 2
---</Scope>
--- <Scope><Markup>##</Markup> Disabled MySt extensions</Scope>
---
--- $$
--- math
--- $$
---
--- <Scope><Markup>```</Markup><CodeBlock>{directive}</CodeBlock>
--- <Markup>```</Markup></Scope>
---
--- <Scope><Markup>##</Markup> Link anchor</Scope>
---
--- <Scope><Link>[link]</Link><Markup>:</Markup> <Link>https://example.com</Link></Scope>
"#;

        // print_result(&code, Box::new(MdParser::new(None)));
        test(&code, Box::new(MarkdownParser::new(None)), &expected).or_fail()?;
        Ok(())
    }

    #[gtest]
    fn test_myst() -> Result<()> {
        let code = r#"
--- # Inline
---
--- {lua:obj}`a.b.c`, {lua:obj}`~a.b.c`,
--- {lua:obj}`<a.b.c>`, {lua:obj}`<~a.b.c>`, {lua:obj}`title <~a.b.c>`.
--- $inline math$, text, $$more inline math$$, a simple $dollar,
--- $$even more inline math$$.
---
--- # Directives
---
--- ```{directive}
--- ```
--- ```{directive}
--- Body
--- ```
--- ```{directive}
--- :param: value
--- Body
--- ```
--- ```{directive}
--- ---
--- param
--- ---
--- Body
--- ```
--- ````{directive1}
--- Body
--- ```{directive2}
--- Body
--- ```
--- Body
--- ````
--- ```{code-block} lua
--- function foo() end
--- ```
---
--- # Math
---
--- $$
--- \frac{1}{2}
--- $$
---
--- Text
---
--- $$
--- \frac{1}{2}
--- $$ (anchor)
"#;

        let expected = r#"
--- <Scope><Markup>#</Markup> Inline</Scope>
---
--- <Markup>{</Markup><Arg>lua:obj</Arg><Markup>}`</Markup><Ref>a.b.c</Ref><Markup>`</Markup>, <Markup>{</Markup><Arg>lua:obj</Arg><Markup>}`</Markup><Code>~</Code><Ref>a.b.c</Ref><Markup>`</Markup>,
--- <Markup>{</Markup><Arg>lua:obj</Arg><Markup>}`</Markup><Code><</Code><Ref>a.b.c</Ref><Code>></Code><Markup>`</Markup>, <Markup>{</Markup><Arg>lua:obj</Arg><Markup>}`</Markup><Code><~</Code><Ref>a.b.c</Ref><Code>></Code><Markup>`</Markup>, <Markup>{</Markup><Arg>lua:obj</Arg><Markup>}`</Markup><Code>title <~</Code><Ref>a.b.c</Ref><Code>></Code><Markup>`</Markup>.
--- <Markup>$</Markup><Code>inline math</Code><Markup>$</Markup>, text, <Markup>$$</Markup><Code>more inline math</Code><Markup>$$</Markup>, a simple $dollar,
--- <Markup>$$</Markup><Code>even more inline math</Code><Markup>$$</Markup>.
---
--- <Scope><Markup>#</Markup> Directives</Scope>
---
--- <Scope><Markup>```{</Markup><Arg>directive</Arg><Markup>}</Markup>
--- <Markup>```</Markup></Scope>
--- <Scope><Markup>```{</Markup><Arg>directive</Arg><Markup>}</Markup>
--- Body
--- <Markup>```</Markup></Scope>
--- <Scope><Markup>```{</Markup><Arg>directive</Arg><Markup>}</Markup>
--- <Markup>:</Markup><Arg>param</Arg><Markup>:</Markup> <CodeBlock>value</CodeBlock>
--- Body
--- <Markup>```</Markup></Scope>
--- <Scope><Markup>```{</Markup><Arg>directive</Arg><Markup>}</Markup>
--- <Markup>---</Markup>
--- <CodeBlock>param</CodeBlock>
--- <Markup>---</Markup>
--- Body
--- <Markup>```</Markup></Scope>
--- <Scope><Markup>````{</Markup><Arg>directive1</Arg><Markup>}</Markup>
--- Body
--- <Scope><Markup>```{</Markup><Arg>directive2</Arg><Markup>}</Markup>
--- Body
--- <Markup>```</Markup></Scope>
--- Body
--- <Markup>````</Markup></Scope>
--- <Scope><Markup>```{</Markup><Arg>code-block</Arg><Markup>}</Markup> <CodeBlock>lua</CodeBlock>
--- <CodeBlockHl(Keyword)>function</CodeBlockHl(Keyword)> <CodeBlockHl(Function)>foo</CodeBlockHl(Function)><CodeBlockHl(Operators)>()</CodeBlockHl(Operators)> <CodeBlockHl(Keyword)>end</CodeBlockHl(Keyword)>
--- <Markup>```</Markup></Scope>
---
--- <Scope><Markup>#</Markup> Math</Scope>
---
--- <Scope><Markup>$$</Markup>
--- <CodeBlock>\frac{1}{2}</CodeBlock>
--- <Markup>$$</Markup></Scope>
---
--- Text
---
--- <Scope><Markup>$$</Markup>
--- <CodeBlock>\frac{1}{2}</CodeBlock>
--- <Markup>$$</Markup> <Arg>(anchor)</Arg></Scope>
"#;

        test(
            &code,
            Box::new(MarkdownParser::new_myst(None, None)),
            &expected,
        )
        .or_fail()?;
        Ok(())
    }

    #[gtest]
    fn test_javadoc_link() -> Result<()> {
        let code = r#"
--- This is a {@link MyClass#method} reference.
--- Another {@link com.example.Class#field} reference.
--- Simple {@link Object} reference.
--- Invalid {not a link} and {@incomplete link.
--- With escape {@link Class\#method} test.
"#;

        let expected = r#"
--- This is a <CodeBlockHl(Operators)>{</CodeBlockHl(Operators)><CodeBlockHl(Decorator)>@link</CodeBlockHl(Decorator)> <JavadocLink>MyClass#method</JavadocLink><CodeBlockHl(Operators)>}</CodeBlockHl(Operators)> reference.
--- Another <CodeBlockHl(Operators)>{</CodeBlockHl(Operators)><CodeBlockHl(Decorator)>@link</CodeBlockHl(Decorator)> <JavadocLink>com.example.Class#field</JavadocLink><CodeBlockHl(Operators)>}</CodeBlockHl(Operators)> reference.
--- Simple <CodeBlockHl(Operators)>{</CodeBlockHl(Operators)><CodeBlockHl(Decorator)>@link</CodeBlockHl(Decorator)> <JavadocLink>Object</JavadocLink><CodeBlockHl(Operators)>}</CodeBlockHl(Operators)> reference.
--- Invalid {not a link} and {@incomplete link.
--- With escape <CodeBlockHl(Operators)>{</CodeBlockHl(Operators)><CodeBlockHl(Decorator)>@link</CodeBlockHl(Decorator)> <JavadocLink>Class\#method</JavadocLink><CodeBlockHl(Operators)>}</CodeBlockHl(Operators)> test.
"#;

        test(&code, Box::new(MarkdownParser::new(None)), &expected).or_fail()?;
        Ok(())
    }

    #[gtest]
    fn test_myst_primary_domain() -> Result<()> {
        let code = r#"--- See {obj}`ref`"#;

        let expected = r#"
            --- See <Markup>{</Markup><Arg>obj</Arg><Markup>}`</Markup><Ref>ref</Ref><Markup>`</Markup>
        "#;

        test(
            &code,
            Box::new(MarkdownParser::new_myst(Some("lua".to_string()), None)),
            &expected,
        )
        .or_fail()?;
        Ok(())
    }

    #[gtest]
    fn test_myst_search_at_offset() -> Result<()> {
        let code = r#"--- See {lua:obj}`x` {lua:obj}`ref`"#;
        let expected = r#"--- See {lua:obj}`x` {lua:obj}`<Ref>ref</Ref>`"#;
        test(
            &code,
            Box::new(MarkdownParser::new_myst(None, Some(31))),
            &expected,
        )
        .or_fail()?;
        test(
            &code,
            Box::new(MarkdownParser::new_myst(None, Some(32))),
            &expected,
        )
        .or_fail()?;
        test(
            &code,
            Box::new(MarkdownParser::new_myst(None, Some(34))),
            &expected,
        )
        .or_fail()?;

        // let code = r#"--- See {lua:obj}`x` {lua:obj}`"#;
        // let expected = r#"--- See {lua:obj}`x` {lua:obj}`<Ref></Ref>"#;
        // test(
        //     &code,
        //     Box::new(MarkdownParser::new_myst(None, Some(31))),
        //     &expected,
        // )
        // .or_fail()?;

        // let code = r#"--- See {lua:obj}`x` {lua:obj}``..."#;
        // let expected = r#"--- See {lua:obj}`x` {lua:obj}`<Ref>`...</Ref>"#;
        // test(
        //     &code,
        //     Box::new(MarkdownParser::new_myst(None, Some(31))),
        //     &expected,
        // )
        // .or_fail()?;
        Ok(())
    }

    #[gtest]
    fn test_intra_doc_links() -> Result<()> {
        let code = r#"
--- See [`Widget`] and [`Widget.name`].
--- Plain [Widget] and [text](url) stay links.
--- Inline `Widget` is just code.
"#;

        let expected = r#"
--- See <Link>[`<Ref>Widget</Ref>`]</Link> and <Link>[`<Ref>Widget.name</Ref>`]</Link>.
--- Plain <Link>[Widget]</Link> and <Link>[text](url)</Link> stay links.
--- Inline <Markup>`</Markup><Code>Widget</Code><Markup>`</Markup> is just code.
"#;

        test(&code, Box::new(MarkdownParser::new(None)), &expected).or_fail()?;
        Ok(())
    }

    #[gtest]
    fn test_intra_doc_link_contexts() -> Result<()> {
        let code = r#"
--- Explicit [`Widget`](https://example.com) stays a link.
--- Plain [Widget] stays a link.
--- [`Widget`] and [`Widget.name`].
"#;

        let expected = r#"
--- Explicit <Link>[`Widget`](https://example.com)</Link> stays a link.
--- Plain <Link>[Widget]</Link> stays a link.
--- <Link>[`<Ref>Widget</Ref>`]</Link> and <Link>[`<Ref>Widget.name</Ref>`]</Link>.
"#;

        test(&code, Box::new(MarkdownParser::new(None)), &expected).or_fail()?;
        Ok(())
    }

    #[gtest]
    fn test_intra_doc_links_skip_code() -> Result<()> {
        let code = r#"
--- ```txt
--- See [`Widget`].
--- ```
---
--- Inline `[`Widget`]` stays code.
"#;

        let expected = r#"
--- <Scope><Markup>```</Markup><CodeBlock>txt</CodeBlock>
--- <CodeBlock>See [`Widget`].</CodeBlock>
--- <Markup>```</Markup></Scope>
---
--- Inline <Markup>`</Markup><Code>[</Code><Markup>`</Markup>Widget<Markup>`</Markup><Code>]</Code><Markup>`</Markup> stays code.
"#;

        test(&code, Box::new(MarkdownParser::new(None)), &expected).or_fail()?;
        Ok(())
    }

    #[gtest]
    fn test_intra_doc_link_in_param_description() -> Result<()> {
        let code = r#"
---@param value string See [`Widget`].
local function f(value) end
"#;

        let expected = r#"
---@param value string See <Link>[`<Ref>Widget</Ref>`]</Link>.
local function f(value) end
"#;

        test(&code, Box::new(MarkdownParser::new(None)), &expected).or_fail()?;
        Ok(())
    }

    #[gtest]
    fn test_intra_doc_link_in_return_description() -> Result<()> {
        let code = r#"
---@return string See [`Widget.name`].
local function f() end
"#;

        let expected = r#"
---@return string See <Link>[`<Ref>Widget.name</Ref>`]</Link>.
local function f() end
"#;

        test(&code, Box::new(MarkdownParser::new(None)), &expected).or_fail()?;
        Ok(())
    }

    #[gtest]
    fn test_md_no_indent() -> Result<()> {
        let code = r#"
---```lua
---
--- local t = 213
---```
---
--- .. code-block:: lua
---
---    local t = 123
---    yes = 1123
local t = 123
"#;

        let expected = r#"
---<Scope><Markup>```</Markup><CodeBlock>lua</CodeBlock>
---
--- <CodeBlockHl(Keyword)>local</CodeBlockHl(Keyword)> <CodeBlockHl(Variable)>t</CodeBlockHl(Variable)> <CodeBlockHl(Operators)>=</CodeBlockHl(Operators)> <CodeBlockHl(Number)>213</CodeBlockHl(Number)>
---<Markup>```</Markup></Scope>
---
--- .. code-block:: lua
---
---<Scope>    <CodeBlock>local t = 123</CodeBlock>
---    <CodeBlock>yes = 1123</CodeBlock></Scope>
local t = 123
"#;

        test(&code, Box::new(MarkdownParser::new(None)), &expected).or_fail()?;
        Ok(())
    }
}
