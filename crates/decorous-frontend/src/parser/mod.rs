mod code_blocks;
mod ctx;
pub mod errors;
mod lexer;

use std::path::Path;

use decorous_errors::Diagnostic;
use rslint_parser::{parse_module, SyntaxNode};

use crate::{
    ast::{
        Attribute, AttributeValue, Code, Comment, DecorousAst, Element, EventHandler, ForBlock,
        IfBlock, Mustache, Node, NodeType, SpecialBlock, Text, UseBlock,
    },
    css,
    errors::{ParseError, ParseErrorType},
    location::Location,
    parser::code_blocks::CodeBlocks,
};
pub use ctx::*;
use lexer::{Allowed, Lexer, Token, TokenKind};

type Result<T> = std::result::Result<T, ParseError<Location>>;

pub struct Parser<'src, 'ctx> {
    lexer: Lexer<'src>,
    current_token: Token<'src>,
    code_blocks: CodeBlocks<'src>,
    ctx: Ctx<'ctx>,
    did_error: bool,
}

macro_rules! expect {
    ($self:expr, $kind:ident(_)) => {{
        $self.next_token();
        let TokenKind::$kind(binding) = $self.current_token.kind else {
            return error!($self, TokenKind::$kind(Default::default()).display_kind());
        };
        Ok(binding)
    }};
    ($self:expr, $kind:ident) => {{
        $self.next_token();
        let TokenKind::$kind = $self.current_token.kind else {
            return error!($self, TokenKind::$kind.display_kind());
        };
        Ok(())
    }};
}

macro_rules! error {
    ($self:expr, $expected:expr) => {
        Err($self.error_on_current(ParseErrorType::Expected($expected)))
    };
    ($self:expr, $($expected:expr),*) => {
        Err($self.error_on_current(ParseErrorType::ExpectedAny(&[$($expected),*])))
    };
}

impl<'src, 'ctx> Parser<'src, 'ctx> {
    pub fn new(src: &'src str) -> Self {
        let mut parser = Self {
            lexer: Lexer::new(src),
            current_token: Token {
                kind: TokenKind::Eof,
                loc: Location::default(),
            },
            code_blocks: CodeBlocks::new(),
            ctx: Ctx::default(),
            did_error: false,
        };

        parser.next_token();
        parser
    }

    pub fn with_ctx(mut self, ctx: Ctx<'ctx>) -> Self {
        self.ctx = ctx;
        self
    }

    pub fn parse(mut self) -> Result<DecorousAst<'src>> {
        self.parse_code_blocks()?;
        let nodes = self.parse_nodes(|tok| {
            Ok(matches!(
                tok.kind,
                TokenKind::CodeBlockIndicator | TokenKind::Eof
            ))
        })?;
        self.parse_code_blocks()?;

        if self.did_error {
            return Err(ParseError::new(
                Location::default(),
                ParseErrorType::DidError,
                None,
            ));
        }

        let (script, css, wasm, comptime) = self.code_blocks.into_parts();

        Ok(DecorousAst {
            nodes,
            script,
            css,
            wasm,
            comptime,
        })
    }

    fn next_token(&mut self) {
        self.current_token = self.lexer.next_token();
    }

    fn next_token_allow(&mut self, allow: Allowed) {
        self.current_token = self.lexer.next_token_allow(allow);
    }

    fn error_on_current(&self, kind: ParseErrorType) -> ParseError<Location> {
        ParseError::new(self.current_token.loc, kind, None)
    }

    fn current_offset(&self) -> usize {
        self.current_token.loc.offset()
    }

    fn parse_node(&mut self) -> Result<Node<'src, Location>> {
        let begin_loc = self.current_offset();
        let ty = match self.current_token.kind {
            TokenKind::ElemBegin(_) => NodeType::Element(self.parse_elem()?),
            TokenKind::Mustache(_) => NodeType::Mustache(self.parse_mustache()?),
            TokenKind::SpecialBlockStart(_) => NodeType::SpecialBlock(self.parse_special_block()?),
            TokenKind::Text(t) => NodeType::Text(Text(t)),
            TokenKind::Comment(comment) => NodeType::Comment(Comment(comment)),
            TokenKind::Eof => {
                return Err(self.error_on_current(ParseErrorType::UnclosedTag(String::new())))
            }

            _ => {
                return error!(
                    self,
                    "the beginning of an element", "a JavaScript expression", "plain text"
                );
            }
        };
        self.next_token();

        Ok(Node::new(
            ty,
            Location::new(begin_loc, self.current_offset() - begin_loc),
        ))
    }

    fn parse_nodes<F>(&mut self, mut stop_pred: F) -> Result<Vec<Node<'src, Location>>>
    where
        F: FnMut(Token) -> std::result::Result<bool, ParseError<Location>>,
    {
        let mut is_first = true;
        let mut nodes = vec![];
        while !stop_pred(self.current_token)? {
            let mut node = self.parse_node()?;
            if is_first {
                // If the first node is a text node with a leading space, strip it
                if let NodeType::Text(Text(t)) = &mut node.node_type {
                    *t = t.strip_prefix(' ').unwrap_or(t);
                    // Avoid pushing the node as a child if it's empty
                    if t.is_empty() {
                        is_first = false;
                        continue;
                    }
                }
            }
            is_first = false;
            nodes.push(node);
        }

        if let Some(NodeType::Text(Text(t))) = nodes.last_mut().map(|node| &mut node.node_type) {
            if t.chars().all(|c| c.is_whitespace()) {
                nodes.pop();
            } else {
                // If the last node is a text node, strip the trailing space
                *t = t.strip_suffix(' ').unwrap_or(t);
                // Pop it if it's empty
                if t.is_empty() {
                    nodes.pop();
                }
            }
        }

        Ok(nodes)
    }

    fn parse_elem(&mut self) -> Result<Element<'src, Location>> {
        let TokenKind::ElemBegin(tag_name) = self.current_token.kind else {
            panic!("should be called with ElemBegin");
        };
        let tag_loc = self.current_token.loc;

        let attrs = if self.lexer.peek_token_allow(Allowed::LBRACKET).kind == TokenKind::Lbracket {
            self.next_token_allow(Allowed::LBRACKET);
            self.parse_attrs()?
        } else {
            vec![]
        };

        self.next_token_allow(Allowed::COLON);
        if self.current_token.kind == TokenKind::Colon {
            // Plus one because of the colon token
            let loc = self.current_offset() + 1;
            let text = expect!(self, Text(_))?;
            return Ok(Element {
                tag: tag_name,
                attrs,
                children: vec![Node::new(
                    NodeType::Text(Text(text.trim_end())),
                    Location::new(loc, text.len()),
                )],
            });
        }

        let children = self.parse_nodes(|tok| {
            if tok.kind == TokenKind::Eof {
                return Err(ParseError::new(
                    tag_loc,
                    ParseErrorType::UnclosedTag(tag_name.to_owned()),
                    None,
                ));
            }

            // Try to close the tag
            if let TokenKind::ElemEnd(end_name) = tok.kind {
                return if end_name == tag_name {
                    Ok(true)
                } else {
                    Err(ParseError::new(
                        tok.loc,
                        ParseErrorType::InvalidClosingTag(tag_name.to_owned()),
                        None,
                    ))
                };
            }
            Ok(false)
        })?;

        Ok(Element {
            tag: tag_name,
            attrs,
            children,
        })
    }

    fn parse_mustache(&mut self) -> Result<Mustache> {
        let TokenKind::Mustache(js_text) = self.current_token.kind else {
            panic!("should be called with Mustache");
        };

        self.parse_js_expr(js_text).map(Mustache)
    }

    fn parse_js_expr(&mut self, js_text: &str) -> Result<SyntaxNode> {
        let parse = rslint_parser::parse_module(js_text, 0);
        if parse.errors().is_empty() {
            Ok(parse.syntax().first_child().unwrap_or(parse.syntax()))
        } else {
            let error = &parse.errors()[0];
            let range = &error.primary.as_ref().unwrap().span.range;
            let start = self.current_offset() + range.start;
            self.ctx.errs.emit(
                Diagnostic::builder(format!("JavaScript error: {}", error.title), start)
                    .add_helper(decorous_errors::Helper {
                        msg: "the error occurred here".into(),
                        span: start..start + range.len(),
                    })
                    .build(),
            );
            self.did_error = true;
            Ok(parse.syntax())
        }
    }

    fn parse_js_block(&mut self, js_text: &str) -> Result<SyntaxNode> {
        let res = parse_module(js_text, 0);
        if res.errors().is_empty()
            || (res.errors().len() == 1
                && res.errors().first().is_some_and(|err| {
                    // HACK: This essentially swallows the error... Not very stable, but
                    // I didn't find a well defined error identification system in the docs
                    // of rslint_errors.
                    //
                    // https://docs.rs/rslint_errors/0.2.0/rslint_errors/struct.Diagnostic.html
                    err.title.as_str() == "Duplicate statement labels are not allowed"
                }))
        {
            Ok(res.syntax())
        } else {
            let error = &res.errors()[0];
            let range = &error.primary.as_ref().unwrap().span.range;
            let start = self.current_offset() + range.start;
            self.ctx.errs.emit(
                Diagnostic::builder(format!("JavaScript error: {}", error.title), start)
                    .add_helper(decorous_errors::Helper {
                        msg: "the error occurred here".into(),
                        span: start..start + range.len(),
                    })
                    .build(),
            );
            self.did_error = true;
            Ok(res.syntax())
        }
    }

    fn parse_attrs(&mut self) -> Result<Vec<Attribute<'src>>> {
        assert_eq!(TokenKind::Lbracket, self.current_token.kind);
        let lbracket_loc = self.current_token.loc;

        self.lexer.attrs_mode(true);
        let mut attrs = vec![];
        self.next_token();
        while self.current_token.kind != TokenKind::Rbracket {
            if self.current_token.kind == TokenKind::Eof {
                return Err(ParseError::new(
                    lbracket_loc,
                    ParseErrorType::UnclosedAttrs,
                    None,
                ));
            }
            let attr = self.parse_attr()?;
            attrs.push(attr);
            self.next_token();
        }
        self.lexer.attrs_mode(false);

        Ok(attrs)
    }

    fn parse_attr(&mut self) -> Result<Attribute<'src>> {
        match self.current_token.kind {
            TokenKind::At => self.parse_event_handler(),
            TokenKind::Ident(_) => self.parse_generic_attr(),
            TokenKind::Colon => self.parse_binding(),
            _ => error!(
                self,
                "an attribute name",
                "colon binding (i.e. `:bind`)",
                "event handler (i.e. `@event`)"
            ),
        }
    }

    fn parse_event_handler(&mut self) -> Result<Attribute<'src>> {
        assert_eq!(TokenKind::At, self.current_token.kind);

        let event = expect!(self, Ident(_))?;
        expect!(self, Equals)?;
        let expr_text = expect!(self, Mustache(_))?;

        Ok(Attribute::EventHandler(EventHandler {
            event,
            expr: self.parse_js_expr(expr_text)?,
        }))
    }

    fn parse_generic_attr(&mut self) -> Result<Attribute<'src>> {
        let TokenKind::Ident(key) = self.current_token.kind else {
            panic!("should be called with Ident");
        };

        if self.lexer.peek_token().kind != TokenKind::Equals {
            return Ok(Attribute::KeyValue(key, None));
        }

        // Equals
        self.next_token();
        self.next_token();

        let attr = match self.current_token.kind {
            TokenKind::Quotes(quotes) => {
                Attribute::KeyValue(key, Some(AttributeValue::Literal(quotes.into())))
            }
            TokenKind::Mustache(mustache) => Attribute::KeyValue(
                key,
                Some(AttributeValue::JavaScript(self.parse_js_expr(mustache)?)),
            ),
            _ => {
                return error!(self, "a quoted literal", "a JavaScript expression");
            }
        };

        Ok(attr)
    }

    fn parse_binding(&mut self) -> Result<Attribute<'src>> {
        assert_eq!(TokenKind::Colon, self.current_token.kind);

        let bind = expect!(self, Ident(_))?;
        expect!(self, Colon)?;

        Ok(Attribute::Binding(bind))
    }

    fn parse_special_block(&mut self) -> Result<SpecialBlock<'src, Location>> {
        let TokenKind::SpecialBlockStart(block_name) = self.current_token.kind else {
            panic!("should only call with SpecialBlockStart");
        };

        let block = match block_name {
            "for" => SpecialBlock::For(self.parse_for_block()?),
            "if" => SpecialBlock::If(self.parse_if_block()?),
            "use" => SpecialBlock::Use(self.parse_use_block()?),
            _ => {
                return error!(self, "a for block", "an if block", "a use block");
            }
        };

        Ok(block)
    }

    fn parse_for_block(&mut self) -> Result<ForBlock<'src, Location>> {
        self.lexer.attrs_mode(true);
        let binding = expect!(self, Ident(_))?;
        self.lexer.allow(Allowed::IN);
        expect!(self, In)?;
        self.lexer.attrs_mode(false);

        let js_text = self.lexer.text_until('}');
        self.next_token();
        let iterator = self.parse_js_expr(js_text)?;

        let inner = self.parse_nodes(|tok| match tok.kind {
            TokenKind::SpecialBlockEnd(end_name) if end_name == "for" => Ok(true),
            TokenKind::SpecialBlockEnd(_) => Err(ParseError::new(
                tok.loc,
                ParseErrorType::InvalidClosingTag("for".to_owned()),
                None,
            )),
            _ => Ok(false),
        })?;

        Ok(ForBlock {
            binding,
            index: None,
            expr: iterator,
            inner,
        })
    }

    fn parse_if_block(&mut self) -> Result<IfBlock<'src, Location>> {
        let js_text = self.lexer.text_until('}');
        self.next_token();
        let condition = self.parse_js_expr(js_text)?;

        let inner = self.parse_nodes(|tok| match tok.kind {
            TokenKind::SpecialBlockEnd(end_name) if end_name == "if" => Ok(true),
            TokenKind::SpecialExtender(extender) if extender == "else" => Ok(true),
            TokenKind::SpecialBlockEnd(_) => Err(ParseError::new(
                tok.loc,
                ParseErrorType::InvalidClosingTag("if".to_owned()),
                None,
            )),
            TokenKind::SpecialExtender(_) => Err(ParseError::new(
                tok.loc,
                ParseErrorType::InvalidExtender("else"),
                None,
            )),
            _ => Ok(false),
        })?;

        let else_block = if matches!(self.current_token.kind, TokenKind::SpecialExtender(_)) {
            self.next_token();
            let inner = self.parse_nodes(|tok| match tok.kind {
                TokenKind::SpecialBlockEnd(end_name) if end_name == "if" => Ok(true),
                TokenKind::SpecialBlockEnd(_) => Err(ParseError::new(
                    tok.loc,
                    ParseErrorType::InvalidClosingTag("if".to_owned()),
                    None,
                )),
                _ => Ok(false),
            })?;
            Some(inner)
        } else {
            None
        };

        Ok(IfBlock {
            expr: condition,
            inner,
            else_block,
        })
    }

    fn parse_use_block(&mut self) -> Result<UseBlock<'src>> {
        self.lexer.attrs_mode(true);
        let path = expect!(self, Quotes(_))?;
        expect!(self, Rbrace)?;
        self.lexer.attrs_mode(false);

        Ok(UseBlock {
            path: Path::new(path),
        })
    }

    fn parse_code_blocks(&mut self) -> Result<()> {
        let mut did_parse = false;
        while self.current_token.kind == TokenKind::CodeBlockIndicator {
            did_parse = true;
            let offset = self.current_offset();
            let err_convert = |err| |_| ParseError::new(Location::new(offset, 1), err, None);
            let code = self.parse_code_block()?;

            match code.lang {
                _ if code.comptime => {
                    self.code_blocks
                        .set_static_wasm(code)
                        .map_err(err_convert(ParseErrorType::CannotHaveTwoStatics))?;
                }
                "js" => {
                    let syntax_node = self.parse_js_block(code.body)?;
                    self.code_blocks
                        .set_script(syntax_node)
                        .map_err(err_convert(ParseErrorType::CannotHaveTwoScripts))?;
                }
                "css" => {
                    let css_parser = css::Parser::new(code.body);
                    let ast = css_parser.parse().map_err(|err| {
                        // TODO: help
                        let _help = err.help().cloned();
                        self.error_on_current(ParseErrorType::CssParsingError(err.into()))
                    })?;
                    self.code_blocks
                        .set_css(ast)
                        .map_err(err_convert(ParseErrorType::CannotHaveTwoStyles))?;
                }
                _ => {
                    match self
                        .ctx
                        .preprocessor
                        .preprocess(code.lang, code.body)
                        .map_err(|err| {
                            self.error_on_current(ParseErrorType::PreprocError(Box::new(err)))
                        })? {
                        Override::Js(js_text) => {
                            let syntax_node = self.parse_js_block(&js_text)?;
                            self.code_blocks
                                .set_script(syntax_node)
                                .map_err(err_convert(ParseErrorType::CannotHaveTwoScripts))?;
                        }
                        Override::Css(css_text) => {
                            let css_parser = css::Parser::new(&css_text);
                            let ast = css_parser.parse().map_err(|err| {
                                // TODO: help
                                let _help = err.help().cloned();
                                self.error_on_current(ParseErrorType::CssParsingError(err.into()))
                            })?;
                            self.code_blocks
                                .set_css(ast)
                                .map_err(err_convert(ParseErrorType::CannotHaveTwoStyles))?;
                        }
                        Override::None => {
                            self.code_blocks
                                .set_wasm(code)
                                .map_err(err_convert(ParseErrorType::CannotHaveTwoWasmBlocks))?;
                        }
                    }
                }
            }
        }
        if did_parse {
            self.next_token();
        }

        Ok(())
    }

    fn parse_code_block(&mut self) -> Result<Code<'src>> {
        assert_eq!(TokenKind::CodeBlockIndicator, self.current_token.kind);

        let offset = self.current_offset();
        self.lexer.attrs_mode(true);
        let lang = expect!(self, Ident(_))?;
        let comptime = if self.lexer.peek_token().kind == TokenKind::Colon {
            self.next_token();
            let ident = expect!(self, Ident(_))?;
            if ident != "static" {
                self.ctx.errs.emit(
                    Diagnostic::builder("expected the static keyword", self.current_offset())
                        .note("the static keyword evaluates the code block at compile time")
                        .add_helper(decorous_errors::Helper {
                            msg: "you might've wanted to change this to `static`".into(),
                            span: self.current_token.loc.into(),
                        })
                        .build(),
                );
                self.did_error = true;
                false
            } else {
                true
            }
        } else {
            false
        };

        let body = self.lexer.text_until_str("---");
        if self.lexer.peek_token().kind == TokenKind::CodeBlockIndicator {
            self.next_token();
        }
        self.lexer.attrs_mode(false);

        Ok(Code {
            lang,
            body,
            offset,
            comptime,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! test {
        ($($input:expr),+) => {
            $(
                let parser = Parser::new($input);
                let ast = parser.parse();
                insta::assert_debug_snapshot!(ast);
             )+
        };
    }

    #[test]
    fn can_parse_attribute() {
        test!(
            "#div[hello=\"hello world\"]/div",
            "#div[hello]/div",
            "#div[hello   =   \"hello world\"]/div",
            "#div[hello   =   \"hello world\"]/div",
            "#div[hello=\"你好\"]/div",
            "#div[hello={ctx[1]}]/div",
            "#div[@click={() => x += 1}]/div",
            "#div[@click   =  \"wrong\"]/div",
            "#div[@click]/div"
        );
    }

    #[test]
    fn can_parse_elements() {
        test!(
            "#div /div",
            "#div[hello=\"world\"]/div",
            "#div[   hello=\"world\"    ] /div",
            "#div[   hello=\"world    ] /div",
            "#div[]/div",
            "#div#li/li/div",
            "#div #ul    /ul/div",
            "#div text {mustache} hello #div/div /div",
            "#div[hello=\"world\"/div",
            "#div[hello=\"world\"]/notdiv",
            "#div]/div",
            "#div",
            "#span[x=\"green\"]:hello world"
        );
    }

    #[test]
    fn can_parse_multiple_elements() {
        test!(
            "#div/div#div hello /div hello",
            "",
            "hello",
            "/div",
            "#div/div// hello!"
        );
    }

    #[test]
    fn can_parse_scripts() {
        test!(
            "---js console.log(\"hello\")---",
            "---css body { height: 100vh; } ---",
            "---wasm let x = 3; ---"
        );
    }

    #[test]
    fn can_parse_entire_input() {
        test!(
            "---js let x = 3; --- #div {x} /div",
            "#div/notdiv#div2/notdiv2",
            "#div",
            "\\/",
            "#div \\#hello \\{ /div",
            "/",
            "#div / /div",
            "#div[@attr={hello]/div",
            "#div[@attr=\"hello\"]/div",
            "#div[@attr]/div",
            "#p Hi /p Hello, {name}",
            "---css body { height: 100vh; } ---"
        );
    }

    #[test]
    fn mustaches_allow_for_curly_braces() {
        test!("{() => { console.log(\"hi\"); }  }");
    }

    #[test]
    fn can_parse_special_blocks() {
        test!(
            "{#for i in [1, 2, 3]} #p hello /p {/for}",
            "{#if x == 3} #p hello /p {/if}",
            "{#if x == 3} #p hello /p {:else} hello {/if}"
        );
    }

    #[test]
    fn css_can_appear_after_template() {
        test!(
            "#p Hello /p 
              ---css p { color: blue; } ---"
        );
    }

    #[test]
    fn can_parse_bindings() {
        test!(
            "#div[:hello:]/div",
            "#div[:invalid]/div",
            "#input[:bind: attr=\"value\"]/input"
        );
    }

    #[test]
    fn css_parse_errors_are_given_offset() {
        test!("#p hi /p ---css p { color: red } ---");
    }

    #[test]
    fn javascript_parse_errors_have_proper_location_offsets() {
        test!("---js let x = ; ---");
    }

    #[test]
    fn can_preprocess() {
        struct Preproc;

        impl Preprocessor for Preproc {
            fn preprocess(
                &self,
                lang: &str,
                body: &str,
            ) -> std::result::Result<Override, PreprocessError> {
                let body = match lang {
                    "ts" => Override::Js(format!("console.log(\"{body}\");")),
                    "sass" => Override::Css(format!("p {{ color: {body}; }}")),
                    _ => Override::None,
                };

                Ok(body)
            }
        }

        let input = "---sass hello --- ---ts typescript? ---";
        let parser = Parser::new(input).with_ctx(Ctx {
            preprocessor: &Preproc,
            ..Default::default()
        });
        let ast = parser.parse();
        insta::assert_debug_snapshot!(ast);
    }

    #[test]
    fn cannot_have_two_code_blocks_of_same_type() {
        test!(
            "---js let x = 0; --- ---js let x = 0; ---",
            "---css p { color: red; } --- ---css p { color: red; } ---",
            "---rust let x = 0; --- ---rust let x = 0; ---"
        );
    }

    #[test]
    fn cannot_have_two_preprocessed_scripts() {
        struct Preproc;
        impl Preprocessor for Preproc {
            fn preprocess(
                &self,
                lang: &str,
                _body: &str,
            ) -> std::result::Result<Override, PreprocessError> {
                let body = match lang {
                    "sass" => Override::Css("p { color: red; }".to_owned()),
                    _ => Override::None,
                };

                Ok(body)
            }
        }
        let input = "---sass hello --- ---sass sass ---";
        let parser = Parser::new(input).with_ctx(Ctx {
            preprocessor: &Preproc,
            ..Default::default()
        });
        let ast = parser.parse();
        insta::assert_debug_snapshot!(ast);
    }

    #[test]
    fn can_parse_use_decls() {
        test!("{#use \"path\"} #p hello /p");
    }

    #[test]
    fn can_parse_static_blocks() {
        test!("---js:static console.log(\"hello\"); ---");
    }
}
