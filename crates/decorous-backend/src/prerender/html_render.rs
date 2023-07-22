use std::io;

use decorous_frontend::{
    ast::{
        Attribute, AttributeValue, Comment, Element, Mustache, Node, NodeType, SpecialBlock, Text,
    },
    FragmentMetadata,
};

use crate::RenderBackend;

pub struct HtmlPrerenderer;

impl RenderBackend for HtmlPrerenderer {
    fn render<T: io::Write>(
        out: &mut T,
        component: &decorous_frontend::Component,
    ) -> io::Result<()> {
        for node in component.fragment_tree() {
            node.html_fmt(out, &())?;
        }

        Ok(())
    }
}

trait HtmlFmt<T: io::Write> {
    type Metadata;

    fn html_fmt(&self, f: &mut T, metadata: &Self::Metadata) -> io::Result<()>;
}

impl<'a, T: io::Write> HtmlFmt<T> for Text<'a> {
    type Metadata = FragmentMetadata;

    fn html_fmt(&self, f: &mut T, _: &Self::Metadata) -> io::Result<()> {
        write!(f, "{}", self)
    }
}

impl<'a, T: io::Write> HtmlFmt<T> for Comment<'a> {
    type Metadata = FragmentMetadata;

    fn html_fmt(&self, f: &mut T, _: &Self::Metadata) -> io::Result<()> {
        write!(f, "<!--{}-->", self.0)
    }
}

impl<T: io::Write> HtmlFmt<T> for Mustache {
    type Metadata = FragmentMetadata;

    fn html_fmt(&self, f: &mut T, metadata: &Self::Metadata) -> io::Result<()> {
        write!(f, "<span id=\"{}\"></span>", metadata.id())
    }
}

impl<'a, T: io::Write> HtmlFmt<T> for SpecialBlock<'a, FragmentMetadata> {
    type Metadata = FragmentMetadata;

    fn html_fmt(&self, f: &mut T, metadata: &Self::Metadata) -> io::Result<()> {
        write!(f, "<span id=\"{}\"></span>", metadata.id())
    }
}

impl<'a, T: io::Write> HtmlFmt<T> for Element<'a, FragmentMetadata> {
    type Metadata = FragmentMetadata;

    fn html_fmt(&self, f: &mut T, metadata: &Self::Metadata) -> io::Result<()> {
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
                | Attribute::EventHandler(_) => has_dynamic = true,
                Attribute::KeyValue(key, None) => {
                    write!(f, " {key}=\"\"")?;
                }
                Attribute::Binding(_) => todo!(),
            }
        }
        if has_dynamic && !overwrite {
            write!(f, " id=\"{}\"", metadata.id())?;
        }
        write!(f, ">")?;
        for child in self.children() {
            child.html_fmt(f, &())?;
        }
        write!(f, "</{}>", self.tag())?;

        Ok(())
    }
}

impl<'a, T: io::Write> HtmlFmt<T> for Node<'a, FragmentMetadata> {
    type Metadata = ();

    fn html_fmt(&self, f: &mut T, _: &Self::Metadata) -> io::Result<()> {
        match self.node_type() {
            NodeType::Text(text) => text.html_fmt(f, self.metadata()),
            NodeType::Element(elem) => elem.html_fmt(f, self.metadata()),
            NodeType::Comment(comment) => comment.html_fmt(f, self.metadata()),
            NodeType::Mustache(mustache) => mustache.html_fmt(f, self.metadata()),
            NodeType::SpecialBlock(block) => block.html_fmt(f, self.metadata()),
        }
    }
}
