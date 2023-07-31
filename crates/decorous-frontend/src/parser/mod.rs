pub mod errors;

use std::borrow::Cow;

use nom::{
    branch::alt,
    bytes::complete::{escaped, tag, take, take_until, take_while},
    character::complete::{
        alpha1, alphanumeric1, anychar, char, multispace0, multispace1, none_of, one_of,
    },
    combinator::{all_consuming, cut, eof, map, not, opt, peek, recognize, value},
    error::ParseError as NomParseError,
    multi::{many0, many0_count, separated_list0},
    sequence::{delimited, pair, preceded, terminated},
    IResult, InputIter, InputTake,
};
use nom_locate::{position, LocatedSpan};
use rslint_parser::{parse_module, SyntaxNode};

use self::errors::{Help, ParseError, ParseErrorType, Report};
use crate::{
    ast::{
        Attribute, AttributeValue, Code, Comment, DecorousAst, Element, EventHandler, ForBlock,
        IfBlock, Mustache, Node, NodeType, SpecialBlock, Text,
    },
    css::{self, ast::Css},
    location::Location,
};

type Result<'a, Output> = IResult<NomSpan<'a>, Output, Report<NomSpan<'a>>>;
type NomSpan<'a> = LocatedSpan<&'a str>;

/// Helper macro for creating an `IResult` error by hand.
macro_rules! nom_err {
    ($input:expr, $severity:ident, $err_type:expr, $help:expr) => {
        Err(nom::Err::$severity(Report::from(ParseError::new(
            $input, $err_type, $help,
        ))))
    };
}

/// Parses a string decorous syntax into an AST.
///
/// A successful parse will yield a [`DecorousAst`]. An unsuccessful one will yield a [`Report`].
pub fn parse(input: &str) -> std::result::Result<DecorousAst, Report<Location>> {
    let result = cut(alt((
        all_consuming(_parse),
        failure_case(char('/'), |_| {
            (
                ParseErrorType::ExpectedClosingTag,
                Some(Help::with_message("escape the slash with \\/")),
            )
        }),
    )))(input.into());
    match result {
        Err(err) => {
            use nom::Err::Failure;
            match err {
                // NomSpans from the report are turned into Locations, so to not leak into the
                // public interface of this module.
                Failure(report) => Err(report.into()),
                _ => unreachable!(),
            }
        }
        Ok((_, ast)) => Ok(ast),
    }
}

fn _parse(input: NomSpan) -> Result<DecorousAst> {
    let (input, (mut script, mut css, mut wasm)) = parse_code_blocks(input)?;
    let (input, nodes) = nodes(input)?;
    let (input, new) = ws(parse_code_blocks)(input)?;
    if css.is_none() {
        css = new.1;
    }
    if wasm.is_none() {
        wasm = new.2;
    }
    if script.is_none() {
        script = new.0;
    }
    Ok((input, DecorousAst::new(nodes, script, css, wasm)))
}

fn parse_code_blocks(
    input: LocatedSpan<&str>,
) -> Result<(Option<SyntaxNode>, Option<Css>, Option<Code>)> {
    let (input, code_blocks) = many0(ws(code))(input)?;
    let mut script = None;
    let mut css = None;
    let mut wasm = None;
    for b in code_blocks {
        match b.lang() {
            "js" => {
                script = match parse_js(b.body(), b.offset()) {
                    Ok(node) => Some(node),
                    Err(err) => return nom_err!(input, Failure, err, None),
                };
            }
            "css" => {
                let p = css::Parser::new(b.body());
                css = Some(match p.parse() {
                    Ok(css) => css,
                    Err(err) => {
                        return nom_err!(input, Failure, ParseErrorType::CssParsingError(err), None)
                    }
                });
            }
            _ => wasm = Some(b),
        }
    }
    Ok((input, (script, css, wasm)))
}

fn code(input: NomSpan) -> Result<Code> {
    let (input, _) = tag("---")(input)?;
    let (input, lang) = take_while(|c: char| !c.is_whitespace())(input)?;
    let (input, loc) = position(input)?;
    let (input, body) = take_until("---")(input)?;
    let (input, _) = take(3usize)(input)?;
    Ok((input, Code::new(&lang, &body, loc.location_offset())))
}

fn nodes(input: NomSpan) -> Result<Vec<Node<'_, Location>>> {
    many0(node)(input)
}

fn node(input: NomSpan) -> Result<Node<'_, Location>> {
    let (input, pos) = position(input)?;
    // Do not succeed if empty
    if input.is_empty() {
        return nom_err!(
            input,
            Error,
            ParseErrorType::Nom(nom::error::ErrorKind::Eof),
            None
        );
    }
    if peek(ws(tag::<&str, NomSpan, Report<NomSpan>>("---")))(input).is_ok() {
        return nom_err!(
            input,
            Error,
            ParseErrorType::Nom(nom::error::ErrorKind::Tag),
            None
        );
    }
    let (input, node) = alt((
        map(element, NodeType::Element),
        map(comment, |c| NodeType::Comment(Comment(&c))),
        map(special_block, NodeType::SpecialBlock),
        map(mustache, |js| NodeType::Mustache(Mustache(js))),
        map(
            escaped(none_of("/#\\{"), '\\', one_of(r#"/#{}"#)),
            |text: NomSpan| NodeType::Text(Text(&text)),
        ),
    ))(input)?;
    let (input, end_pos) = position(input)?;
    let location = Location::from_spans(pos, end_pos);

    Ok((input, Node::new(node, location)))
}

fn attributes(input: NomSpan) -> Result<Vec<Attribute>> {
    let (input, start_pos) = position(input)?;
    let start_line = start_pos.location_line();
    delimited(
        ws_trailing(char('[')),
        separated_list0(multispace1, attribute),
        alt((
            ws_leading(char(']')),
            failure_case(bad_char, move |_| {
                (
                    ParseErrorType::ExpectedCharacter(']'),
                    Some(Help::with_line(
                        start_line,
                        "attribute bracket was never closed",
                    )),
                )
            }),
        )),
    )(input)
}

fn element(input: NomSpan) -> Result<Element<'_, Location>> {
    let (input, _) = char('#')(input)?;
    let (input, start_pos) = position(input)?;
    let (input, tag_name) = alphanumeric1(input)?;
    let (input, attrs) = terminated(opt(attributes), multispace0)(input)?;
    let (input, children) = alt((
        preceded(
            char(':'),
            map(parse_text, |text: NomSpan| {
                vec![Node::new(NodeType::Text(Text(&text)), Location::default())]
            }),
        ),
        terminated(
            map(nodes, |mut nodes| {
                if let Some(NodeType::Text(Text(t))) =
                    nodes.last_mut().map(|node| node.node_type_mut())
                {
                    *t = t.strip_suffix(' ').unwrap_or(t);
                    if t.is_empty() {
                        nodes.pop();
                    }
                }
                nodes
            }),
            alt((
                terminated(
                    preceded(
                        char('/'),
                        alt((
                            tag(*tag_name.fragment()),
                            failure_case(identifier, |_| {
                                (
                                    ParseErrorType::InvalidClosingTag(tag_name.to_string()),
                                    Some(Help::with_message("replace this tag with the expected!")),
                                )
                            }),
                        )),
                    ),
                    opt(char(' ')),
                ),
                failure_case(bad_char, |_| {
                    (
                        ParseErrorType::UnclosedTag(
                            tag_name.to_string(),
                            start_pos.location_line(),
                        ),
                        Some(Help::with_line(
                            start_pos.location_line(),
                            "tag never closed",
                        )),
                    )
                }),
            )),
        ),
    ))(input)?;
    Ok((
        input,
        Element::new(&tag_name, attrs.unwrap_or_default(), children),
    ))
}

fn attribute(input: NomSpan) -> Result<Attribute<'_>> {
    let (input, event_handler) = map(opt(char('@')), |evt| evt.is_some())(input)?;
    let (input, attr) = identifier(input)?;
    if event_handler {
        let (input, _) = alt((
            ws(char('=')),
            failure_case(bad_char, |_| {
                (
                    ParseErrorType::ExpectedCharacter('='),
                    Some(Help::with_message(
                        "value-less event handlers are not allowed",
                    )),
                )
            }),
        ))(input)?;
        if peek(char::<NomSpan, Report<NomSpan>>('{'))(input).is_err() {
            return nom_err!(
                input,
                Failure,
                ParseErrorType::ExpectedCharacter('{'),
                Some(Help::with_message(
                    "expected because event handlers must always have curly braces as their values"
                ))
            );
        }
        return map(mustache, |node| {
            Attribute::EventHandler(EventHandler::new(&attr, node))
        })(input);
    }

    let (input, eq_char) = opt(ws(char('=')))(input)?;
    if eq_char.is_none() {
        return Ok((input, Attribute::KeyValue(&attr, None)));
    }
    let (input, attribute) = alt((
        map(mustache, |node| {
            Attribute::KeyValue(&attr, Some(AttributeValue::JavaScript(node)))
        }),
        map(
            preceded(
                alt((
                    char('\"'),
                    failure_case(bad_char, |_| {
                        (
                            ParseErrorType::ExpectedCharacter('"'),
                            Some(Help::with_message("expected either { or \"")),
                        )
                    }),
                )),
                alt((
                    terminated(parse_str, char('\"')),
                    failure_case(bad_char, |_| {
                        (
                            ParseErrorType::ExpectedCharacter('"'),
                            Some(Help::with_message("quote was never closed")),
                        )
                    }),
                )),
            ),
            |t| Attribute::KeyValue(&attr, Some(AttributeValue::Literal(Cow::Borrowed(&t)))),
        ),
    ))(input)?;

    Ok((input, attribute))
}

fn comment(input: NomSpan) -> Result<NomSpan> {
    preceded(tag("//"), take_while(|c| c != '\n'))(input)
}

fn mustache(input: NomSpan) -> Result<SyntaxNode> {
    // Make sure it doesn't parse "{/", as that's the beginning of the end of a special block
    not(alt((tag("{/"), tag("{:"))))(input)?;
    preceded(char('{'), mustache_inner)(input)
}

fn special_block(input: NomSpan) -> Result<SpecialBlock<'_, Location>> {
    let (input, block_type) = preceded(tag("{#"), identifier)(input)?;
    match *block_type.fragment() {
        "for" => {
            let (input, binding) = delimited(multispace1, identifier, multispace0)(input)?;
            let (input, true_binding) = opt(preceded(ws_trailing(char(',')), identifier))(input)?;
            let (input, _) = delimited(multispace0, tag("in"), multispace1)(input)?;
            let (input, expr) = terminated(mustache_inner, opt(char(' ')))(input)?;
            let (input, inner) = terminated(nodes, tag("{/for}"))(input)?;
            if let Some(bind) = true_binding {
                Ok((
                    input,
                    SpecialBlock::For(ForBlock::new(&bind, Some(&binding), expr, inner)),
                ))
            } else {
                Ok((
                    input,
                    SpecialBlock::For(ForBlock::new(&binding, None, expr, inner)),
                ))
            }
        }
        "if" => {
            let (input, expr) = terminated(mustache_inner, opt(char(' ')))(input)?;
            let (input, (inner, else_block)) = pair(
                nodes,
                alt((
                    map(tag("{/if}"), |_| None),
                    map(
                        delimited(
                            delimited(multispace0, tag("{:else}"), opt(char(' '))),
                            nodes,
                            tag("{/if}"),
                        ),
                        Some,
                    ),
                )),
            )(input)?;
            Ok((
                input,
                SpecialBlock::If(IfBlock::new(expr, inner, else_block)),
            ))
        }
        "each" => nom_err!(
            input,
            Failure,
            ParseErrorType::InvalidSpecialBlockType(String::from("each")),
            Some(Help::with_message(
                "you might've meant `for`. each loops do not exist in decorous"
            ))
        ),
        block => nom_err!(
            input,
            Failure,
            ParseErrorType::InvalidSpecialBlockType(block.to_owned()),
            None
        ),
    }
}

fn mustache_inner(input: NomSpan) -> Result<SyntaxNode> {
    let (input, loc) = position(input)?;
    let (input, js_text) = terminated(
        alt((
            take_ignoring_nested('{', '}'),
            failure_case(bad_char, |_| {
                (
                    ParseErrorType::ExpectedCharacter('}'),
                    Some(Help::with_message("mustache was never closed")),
                )
            }),
        )),
        char('}'),
    )(input)?;

    match parse_js(&js_text, loc.location_offset()) {
        Ok(s) => Ok((input, s.first_child().map_or(s, |child| child))),
        Err(err) => nom_err!(input, Failure, err, None),
    }
}

fn parse_js(text: &str, offset: usize) -> std::result::Result<SyntaxNode, ParseErrorType> {
    let res = parse_module(text, 0);
    if res.errors().is_empty() {
        Ok(res.syntax())
    } else {
        // A bit of a weird way to get owned diagnostics...
        let errors = res.ok().expect_err("should have errors");
        Err(ParseErrorType::JavaScriptDiagnostics { errors, offset })
    }
}

fn parse_str(i: NomSpan) -> Result<NomSpan> {
    escaped(none_of("\""), '\\', one_of("\"n\\"))(i)
}

fn parse_text(input: NomSpan) -> Result<NomSpan> {
    escaped(none_of("/#\\{\n"), '\\', one_of(r#"/#{}"#))(input)
}

// --General purpose parsers--

fn failure_case<'i, Free, Parsed, P, F>(
    parser: P,
    err_constructor: F,
) -> impl Fn(NomSpan<'i>) -> Result<'i, Free>
where
    P: Fn(NomSpan<'i>) -> Result<'i, Parsed>,
    F: Fn(Parsed) -> (ParseErrorType, Option<Help>),
{
    use nom::Err::{Error, Failure, Incomplete};
    move |i: NomSpan<'i>| match parser(i) {
        Ok((_, parsed)) => {
            let (err, help) = err_constructor(parsed);
            let report = Report::from(ParseError::new(i, err, help));
            Err(Failure(report))
        }
        Err(Failure(e)) => Err(Failure(e)),
        Err(Error(e)) => Err(Error(e)),
        Err(Incomplete(need)) => Err(Incomplete(need)),
    }
}

fn take_ignoring_nested(left: char, right: char) -> impl Fn(NomSpan) -> Result<NomSpan> {
    move |i: NomSpan| {
        let mut index = 0;
        let mut needed = 1;
        let mut chars = i.iter_elements();

        while needed > 0 {
            let c = chars.next().unwrap_or_default();
            if c == left {
                needed += 1;
            }
            if c == right {
                needed -= 1;
            }
            if c == '\0' {
                return Err(nom::Err::Error(Report::from_error_kind(
                    i,
                    nom::error::ErrorKind::Eof,
                )));
            }
            if needed > 0 {
                index += c.len_utf8();
            }
        }

        let (new_i, old_i) = i.take_split(index);
        Ok((new_i, old_i))
    }
}

/// Matches eof or any character
fn bad_char(input: NomSpan) -> Result<char> {
    alt((value('\0', eof), anychar))(input)
}

fn ws<'a, F: 'a, O>(inner: F) -> impl FnMut(NomSpan<'a>) -> Result<O>
where
    F: Fn(NomSpan<'a>) -> Result<O>,
{
    delimited(multispace0, inner, multispace0)
}

fn ws_leading<'a, F: 'a, O>(inner: F) -> impl FnMut(NomSpan<'a>) -> Result<O>
where
    F: Fn(NomSpan<'a>) -> Result<O>,
{
    preceded(multispace0, inner)
}

fn ws_trailing<'a, F: 'a, O>(inner: F) -> impl FnMut(NomSpan<'a>) -> Result<O>
where
    F: Fn(NomSpan<'a>) -> Result<O>,
{
    terminated(inner, multispace0)
}

fn identifier(input: NomSpan) -> Result<NomSpan> {
    recognize(pair(
        alt((alpha1, tag("_"))),
        many0_count(alt((alphanumeric1, tag("_")))),
    ))(input)
}

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! nom_test_all_insta {
        ($func:ident, $inputs:expr) => {
            for input in $inputs {
                insta::assert_debug_snapshot!($func(input.into()));
            }
        };
    }

    #[test]
    fn can_parse_attribute() {
        nom_test_all_insta!(
            attribute,
            [
                "hello=\"hello world\"",
                "hello",
                "hello   =   \"hello world\"",
                "hello   =   \"hello world\"",
                "hello=\"你好\"",
                "hello={ctx[1]}",
                "@click={() => x += 1}",
                "@click   =  \"wrong\"",
                "@click",
            ]
        );
    }

    #[test]
    fn can_parse_elements() {
        nom_test_all_insta!(
            element,
            [
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
            ]
        );
    }

    #[test]
    fn can_parse_multiple_elements() {
        nom_test_all_insta!(
            nodes,
            [
                "#div/div#div hello /div hello",
                "",
                "hello",
                "/div",
                "#div/div// hello!"
            ]
        );
    }

    #[test]
    fn can_parse_scripts() {
        nom_test_all_insta!(parse_code_blocks, ["---js console.log(\"hello\")---",]);
        nom_test_all_insta!(parse_code_blocks, ["---css body { height: 100vh; } ---"]);
        nom_test_all_insta!(parse_code_blocks, ["---wasm let x = 3; ---"]);
    }

    #[test]
    fn can_parse_entire_input() {
        nom_test_all_insta!(
            parse,
            [
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
                "---css body { height: 100vh; } ---",
            ]
        )
    }

    #[test]
    fn mustaches_allow_for_curly_braces() {
        nom_test_all_insta!(mustache, ["{() => { console.log(\"hi\"); }  }"])
    }

    #[test]
    fn can_parse_special_blocks() {
        nom_test_all_insta!(
            parse,
            [
                "{#for i in [1, 2, 3]} #p hello /p {/for}",
                "{#for idx, elem in [1, 2, 3]} #p hello /p {/for}",
                "{#if x == 3} #p hello /p {/if}",
                "{#if x == 3} #p hello /p {:else} hello {/if}"
            ]
        )
    }

    #[test]
    fn css_can_appear_after_template() {
        nom_test_all_insta!(
            parse,
            ["#p Hello /p 
            ---css p { color: blue; } ---"]
        )
    }
}
