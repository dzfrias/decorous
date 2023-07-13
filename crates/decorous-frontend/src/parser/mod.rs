mod errors;

use nom::{
    branch::alt,
    bytes::complete::{escaped, tag, take, take_until, take_while},
    character::complete::{
        alpha1, alphanumeric1, anychar, char, multispace0, multispace1, none_of, one_of,
    },
    combinator::{all_consuming, cut, eof, map, opt, peek, recognize, value},
    error::ParseError as NomParseError,
    multi::{many0, many0_count, separated_list0},
    sequence::{delimited, pair, preceded, terminated},
    IResult, InputIter, InputTake,
};
use nom_locate::{position, LocatedSpan};
use rslint_parser::{parse_module, SyntaxNode};

use self::errors::{ParseError, ParseErrorType, Report};
use crate::ast::{
    Attribute, AttributeValue, DecorousAst, Element, EventHandler, Location, Node, NodeType,
};

type Result<'a, Output> = IResult<NomSpan<'a>, Output, Report<NomSpan<'a>>>;
type NomSpan<'a> = LocatedSpan<&'a str>;

/// Helper macro for creating an IResult error by hand.
macro_rules! nom_err {
    ($input:expr, $severity:ident, $err_type:expr) => {
        Err(nom::Err::$severity(Report::from(ParseError::new(
            $input, $err_type,
        ))))
    };
}

pub fn parse<'a>(input: &'a str) -> std::result::Result<DecorousAst<'a>, Report<NomSpan<'a>>> {
    let result = cut(alt((
        all_consuming(_parse),
        failure_case(char('/'), |_| ParseErrorType::ExpectedClosingTag),
    )))(input.into());
    match result {
        Err(err) => {
            use nom::Err::Failure;
            match err {
                Failure(report) => Err(report),
                _ => unreachable!(),
            }
        }
        Ok((_, ast)) => Ok(ast),
    }
}

fn _parse<'a>(input: NomSpan<'a>) -> Result<DecorousAst<'a>> {
    let (input, script) = opt(ws(script))(input)?;
    let (input, nodes) = nodes(input)?;
    let (input, css) = opt(ws(style))(input)?;
    Ok((input, DecorousAst::new(nodes, script, css)))
}

fn script(input: NomSpan) -> Result<SyntaxNode> {
    let (input, _) = tag("---js")(input)?;
    let (input, loc) = position(input)?;
    let (input, body) = take_until("---")(input)?;
    let (input, _) = take(3usize)(input)?;
    match parse_js(&body, loc.location_offset()) {
        Ok(node) => Ok((input, node)),
        Err(err) => nom_err!(input, Failure, err),
    }
}

fn style(input: NomSpan) -> Result<&'_ str> {
    let (input, _) = tag("---css")(input)?;
    let (input, body) = take_until("---")(input)?;
    let (input, _) = take(3usize)(input)?;
    Ok((input, &body))
}

fn nodes(input: NomSpan) -> Result<Vec<Node<'_, Location>>> {
    many0(node)(input)
}

fn node(input: NomSpan) -> Result<Node<'_, Location>> {
    let (input, pos) = position(input)?;
    // Do not succeed. This tells many0 to stop before it errors (many0 errors when there's no more
    // input).
    if input.is_empty() {
        return nom_err!(
            input,
            Error,
            ParseErrorType::Nom(nom::error::ErrorKind::Eof)
        );
    }
    let (input, node) = alt((
        map(element, NodeType::Element),
        map(comment, |c| NodeType::Comment(&c)),
        map(mustache, NodeType::Mustache),
        map(
            escaped(none_of("/#\\{"), '\\', one_of(r#"/#{}"#)),
            |text: NomSpan| NodeType::Text(&text),
        ),
    ))(input)?;
    let (input, end_pos) = position(input)?;
    let location = Location::from_spans(pos, end_pos);

    Ok((input, Node::new(node, location)))
}

fn attributes<'a>(input: NomSpan<'a>) -> Result<Vec<Attribute<'a>>> {
    delimited(
        ws_trailing(tag("[")),
        separated_list0(multispace1, attribute),
        alt((
            ws_leading(tag("]")),
            failure_case(bad_char, |_| ParseErrorType::ExpectedCharacter(']')),
        )),
    )(input)
}

fn element(input: NomSpan) -> Result<Element<'_, Location>> {
    let (input, _) = tag("#")(input)?;
    let (input, tag_name) = alphanumeric1(input)?;
    let (input, attrs) = terminated(opt(attributes), multispace0)(input)?;
    let (input, children) = alt((
        preceded(
            char(':'),
            map(parse_text, |text: NomSpan| {
                vec![Node::new(NodeType::Text(&text), Location::default())]
            }),
        ),
        terminated(
            map(nodes, |mut nodes| {
                if let Some(NodeType::Text(t)) = nodes.last_mut().map(|node| node.node_type_mut()) {
                    *t = t.strip_suffix(" ").unwrap_or(t);
                    if t.is_empty() {
                        nodes.pop();
                    }
                }
                nodes
            }),
            alt((
                preceded(
                    char('/'),
                    alt((
                        tag(*tag_name.fragment()),
                        failure_case(identifier, |tag| {
                            ParseErrorType::InvalidClosingTag(tag.to_string())
                        }),
                    )),
                ),
                failure_case(bad_char, |_| {
                    ParseErrorType::UnclosedTag(tag_name.to_string())
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
            failure_case(bad_char, |_| ParseErrorType::ExpectedCharacter('=')),
        ))(input)?;
        if peek(char::<NomSpan, Report<NomSpan>>('{'))(input).is_err() {
            return nom_err!(input, Failure, ParseErrorType::ExpectedCharacter('{'));
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
                char('\"'),
                alt((
                    terminated(parse_str, char('\"')),
                    failure_case(bad_char, |_| ParseErrorType::ExpectedCharacter('"')),
                )),
            ),
            |t| Attribute::KeyValue(&attr, Some(AttributeValue::Literal(&t))),
        ),
    ))(input)?;

    Ok((input, attribute))
}

fn comment(input: NomSpan) -> Result<NomSpan> {
    preceded(tag("//"), take_while(|c| c != '\n'))(input)
}

fn mustache(input: NomSpan) -> Result<SyntaxNode> {
    let (input, loc) = position(input)?;
    let (input, js_text) = delimited(
        char('{'),
        alt((
            take_ignoring_nested('{', '}'),
            failure_case(bad_char, |_| ParseErrorType::ExpectedCharacter('}')),
        )),
        char('}'),
    )(input)?;

    match parse_js(&js_text, loc.location_offset()) {
        Ok(s) => Ok((input, s.first_child().map_or(s, |child| child))),
        Err(err) => Err(nom::Err::Failure(Report::from(ParseError::new(input, err)))),
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
    escaped(none_of("/#\\{"), '\\', one_of(r#"/#{}"#))(input)
}

// --General purpose parsers--

fn failure_case<'i, Free, Parsed, P, F>(
    parser: P,
    err_constructor: F,
) -> impl Fn(NomSpan<'i>) -> Result<'i, Free>
where
    P: Fn(NomSpan<'i>) -> Result<'i, Parsed>,
    F: Fn(Parsed) -> ParseErrorType,
{
    use nom::Err::{Error, Failure, Incomplete};
    move |i: NomSpan<'i>| match parser(i) {
        Ok((_, parsed)) => {
            let report = Report::from(ParseError::new(i, err_constructor(parsed)));
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
        nom_test_all_insta!(script, ["---js console.log(\"hello\")---",]);
        nom_test_all_insta!(style, ["---css body { height: 100vh; } ---"]);
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
            ]
        )
    }

    #[test]
    fn mustaches_allow_for_curly_braces() {
        nom_test_all_insta!(mustache, ["{() => { console.log(\"hi\"); }  }"])
    }
}
