#![allow(dead_code, unused_imports)]

mod errors;

use nom::{
    branch::alt,
    bytes::complete::{escaped, tag, take, take_until, take_while, take_while1},
    character::complete::{
        alpha1, alphanumeric1, anychar, char, multispace0, multispace1, none_of, one_of,
    },
    combinator::{cut, eof, map, opt, recognize, value},
    multi::{many0, many0_count, separated_list0},
    sequence::{delimited, pair, preceded, terminated},
    IResult,
};
use nom_locate::{position, LocatedSpan};
use rslint_parser::{parse_module, SyntaxNode};

use self::errors::{ParseError, ParseErrorType, Report};
use crate::ast::{Attribute, AttributeValue, Element, EventHandler, Location, Node, NodeType};

type Result<'a, Output> = IResult<NomSpan<'a>, Output, Report<NomSpan<'a>>>;
type NomSpan<'a> = LocatedSpan<&'a str>;

#[derive(Debug)]
pub struct NewLocation {
    offset: usize,
    length: usize,
    line: u32,
    column: usize,
}

impl NewLocation {
    pub fn from_spans(span1: NomSpan, span2: NomSpan) -> Self {
        Self {
            offset: span1.location_offset(),
            length: span2.location_offset() - span1.location_offset(),
            line: span1.location_line(),
            column: span1.get_column(),
        }
    }

    pub fn offset(&self) -> usize {
        self.offset
    }

    pub fn length(&self) -> usize {
        self.length
    }

    pub fn line(&self) -> u32 {
        self.line
    }

    pub fn column(&self) -> usize {
        self.column
    }
}

// pub fn parse<'a>(input: NomSpan<'a>) -> Result<Vec<Node<'a, NewLocation>>> {
//     let (input, script) = opt(ws(script))(input)?;
//     let (input, nodes) = nodes(input)?;
//     // TODO: Turn into decorous ast
// }

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

fn bad_char(input: NomSpan) -> Result<char> {
    alt((value('\0', eof), anychar))(input)
}

fn parse_str(i: NomSpan) -> Result<NomSpan> {
    escaped(none_of("\""), '\\', one_of("\"n\\"))(i)
}

fn any_word(input: NomSpan) -> Result<NomSpan> {
    recognize(take_while(|c: char| c.is_whitespace()))(input)
}

fn script(input: NomSpan) -> Result<SyntaxNode> {
    let (input, _) = tag("---js")(input)?;
    let (input, body) = take_until("---")(input)?;
    let (input, _) = take(3usize)(input)?;
    // TODO: Errors
    let parsed = parse_module(&body, 0);
    Ok((input, parsed.syntax()))
}

fn nodes(input: NomSpan) -> Result<Vec<Node<'_, NewLocation>>> {
    many0(node)(input)
}

fn node(input: NomSpan) -> Result<Node<'_, NewLocation>> {
    let (input, pos) = position(input)?;
    let (input, node) = alt((
        map(element, NodeType::Element),
        map(comment, |c| NodeType::Comment(&c)),
        map(mustache, NodeType::Mustache),
        map(
            take_while1(|c| c != '/' && c != '#' && c != '{'),
            |text: NomSpan| NodeType::Text(&text),
        ),
    ))(input)?;
    let (input, end_pos) = position(input)?;
    let location = NewLocation::from_spans(pos, end_pos);

    Ok((input, Node::new(node, location)))
}

fn element(input: NomSpan) -> Result<Element<'_, NewLocation>> {
    let (input, _) = tag("#")(input)?;
    let (input, tag_name) = alphanumeric1(input)?;
    let (input, attrs) = terminated(
        opt(delimited(
            ws_trailing(tag("[")),
            separated_list0(multispace1, attribute),
            alt((
                ws_leading(tag("]")),
                failure_case(bad_char, |_| ParseErrorType::ExpectedCharacter(']')),
            )),
        )),
        multispace0,
    )(input)?;
    let (input, children) = nodes(input)?;
    let (input, _) = preceded(
        char('/'),
        alt((
            tag(*tag_name.fragment()),
            failure_case(identifier, |tag| {
                ParseErrorType::InvalidClosingTag(tag.to_string())
            }),
        )),
    )(input)?;
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
    // FIX: Allow for curly braces in JavaScript
    let (input, js_text) = delimited(tag("{"), take_while(|c| c != '}'), tag("}"))(input)?;
    Ok((input, parse_module(&js_text, 0).syntax()))
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
                "#div"
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
        nom_test_all_insta!(script, ["---js console.log(\"hello\")---"])
    }
}
