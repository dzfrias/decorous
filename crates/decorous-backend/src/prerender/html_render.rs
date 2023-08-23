use std::io;

use decorous_frontend::{
    ast::{
        Attribute, AttributeValue, Comment, Element, Mustache, Node, NodeType, SpecialBlock, Text,
    },
    Component, DeclaredVariables, FragmentMetadata,
};

pub fn render_html<W: io::Write>(out: &mut W, component: &Component) -> io::Result<()> {
    for node in component.fragment_tree() {
        node.html_fmt(out, &(), component.declared_vars())?;
    }

    Ok(())
}

trait HtmlFmt<T: io::Write> {
    type Metadata;

    fn html_fmt(
        &self,
        f: &mut T,
        metadata: &Self::Metadata,
        declared: &DeclaredVariables,
    ) -> io::Result<()>;
}

impl<'a, T: io::Write> HtmlFmt<T> for Text<'a> {
    type Metadata = FragmentMetadata;

    fn html_fmt(
        &self,
        f: &mut T,
        _: &Self::Metadata,
        _declared: &DeclaredVariables,
    ) -> io::Result<()> {
        write!(f, "{}", self)
    }
}

impl<'a, T: io::Write> HtmlFmt<T> for Comment<'a> {
    type Metadata = FragmentMetadata;

    fn html_fmt(
        &self,
        f: &mut T,
        _: &Self::Metadata,
        _declared: &DeclaredVariables,
    ) -> io::Result<()> {
        write!(f, "<!--{}-->", self.0)
    }
}

impl<T: io::Write> HtmlFmt<T> for Mustache {
    type Metadata = FragmentMetadata;

    fn html_fmt(
        &self,
        f: &mut T,
        metadata: &Self::Metadata,
        _declared: &DeclaredVariables,
    ) -> io::Result<()> {
        write!(f, "<span id=\"{}\"></span>", metadata.id())
    }
}

impl<'a, T: io::Write> HtmlFmt<T> for SpecialBlock<'a, FragmentMetadata> {
    type Metadata = FragmentMetadata;

    fn html_fmt(
        &self,
        f: &mut T,
        metadata: &Self::Metadata,
        _declared: &DeclaredVariables,
    ) -> io::Result<()> {
        write!(f, "<span id=\"{}\"></span>", metadata.id())
    }
}

impl<'a, T: io::Write> HtmlFmt<T> for Element<'a, FragmentMetadata> {
    type Metadata = FragmentMetadata;

    fn html_fmt(
        &self,
        f: &mut T,
        metadata: &Self::Metadata,
        declared: &DeclaredVariables,
    ) -> io::Result<()> {
        write!(f, "<{}", self.tag())?;
        let mut has_dynamic = false;
        let mut overwrite = false;
        for attr in self.attrs() {
            match attr {
                Attribute::KeyValue(key, Some(AttributeValue::Literal(literal))) => {
                    if *key == "id" {
                        overwrite = true;
                    }
                    write!(f, " {key}=\"{literal}\"")?;
                }
                // Do nothing. Dynamic attributes can't be baked statically into the HTML
                Attribute::KeyValue(_, Some(AttributeValue::JavaScript(_)))
                | Attribute::EventHandler(_)
                | Attribute::Binding(_) => has_dynamic = true,
                Attribute::KeyValue(key, None) => {
                    write!(f, " {key}=\"\"")?;
                }
            }
        }
        if metadata.parent_id().is_none() && !declared.css_mustaches().is_empty() {
            has_dynamic = true;
        }
        if has_dynamic && !overwrite {
            write!(f, " id=\"{}\"", metadata.id())?;
        }
        write!(f, ">")?;
        for child in self.children() {
            child.html_fmt(f, &(), declared)?;
        }
        write!(f, "</{}>", self.tag())?;

        Ok(())
    }
}

impl<'a, T: io::Write> HtmlFmt<T> for Node<'a, FragmentMetadata> {
    type Metadata = ();

    fn html_fmt(
        &self,
        f: &mut T,
        _: &Self::Metadata,
        _declared: &DeclaredVariables,
    ) -> io::Result<()> {
        match self.node_type() {
            NodeType::Text(text) => text.html_fmt(f, self.metadata(), _declared),
            NodeType::Element(elem) => elem.html_fmt(f, self.metadata(), _declared),
            NodeType::Comment(comment) => comment.html_fmt(f, self.metadata(), _declared),
            NodeType::Mustache(mustache) => mustache.html_fmt(f, self.metadata(), _declared),
            NodeType::SpecialBlock(block) => block.html_fmt(f, self.metadata(), _declared),
        }
    }
}
