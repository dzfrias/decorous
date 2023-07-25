use std::{fmt::Write, rc::Rc};

use decorous_frontend::{
    ast::{Attribute, AttributeValue, Mustache, Node, NodeType, SpecialBlock},
    Component, FragmentMetadata,
};
use rslint_parser::{parse_text, SmolStr, SyntaxNode};

use crate::prerender::node_analyzer::NodeAnalyzer;

pub type IdVec<'a, T> = Vec<(&'a FragmentMetadata, T)>;
pub type IdSlice<'a, T> = &'a [(&'a FragmentMetadata, T)];

#[derive(Debug, Clone)]
pub struct ReactiveData<'ast> {
    mustaches: IdVec<'ast, SyntaxNode>,
    key_values: IdVec<'ast, Rc<[(SmolStr, SyntaxNode)]>>,
    event_listeners: IdVec<'ast, Rc<[(SmolStr, SyntaxNode)]>>,
    special_blocks: IdVec<'ast, &'ast SpecialBlock<'ast, FragmentMetadata>>,
}

#[derive(Debug)]
pub struct ReactivityAnalyzer<'ast> {
    reactive_data: ReactiveData<'ast>,
    style_cache: Option<String>,
}

impl<'ast> ReactivityAnalyzer<'ast> {
    pub fn new() -> Self {
        Self {
            reactive_data: ReactiveData {
                mustaches: vec![],
                special_blocks: vec![],
                key_values: vec![],
                event_listeners: vec![],
            },
            style_cache: None,
        }
    }

    /// Returns the contents of the style cache. Computes style cache if not already computed.
    fn use_style_cache(&mut self, component: &Component<'ast>) -> &str {
        if let Some(ref style) = self.style_cache {
            style.as_str()
        } else {
            let style = {
                // The minimum length of each part of the eventual style
                const MIN_LEN: usize = "--decor-0: ${}; ".len();
                let mut style = String::with_capacity(
                    component.declared_vars().css_mustaches().len() * MIN_LEN,
                );
                for (mustache, id) in component.declared_vars().css_mustaches() {
                    crate::codegen_utils::force_write!(style, "--decor-{id}: ${{{mustache}}}; ");
                }
                style
            };
            self.style_cache = Some(style);
            self.style_cache.as_ref().unwrap().as_str()
        }
    }
}

impl<'a> NodeAnalyzer<'a> for ReactivityAnalyzer<'a> {
    type AccumulatedOutput = ReactiveData<'a>;

    fn visit(&mut self, node: &'a Node<'_, FragmentMetadata>, component: &'a Component) {
        match node.node_type() {
            NodeType::Element(elem) => {
                let inline_styles_candidate = node.metadata().parent_id().is_none()
                    && !component.declared_vars().css_mustaches().is_empty();

                // PERF: small vec?
                let mut kvs = vec![];
                let mut listeners = vec![];
                let mut has_style = false;
                for attr in elem.attrs() {
                    match attr {
                        // The style cache is used for reactive CSS. If a component is toplevel,
                        // and there is reactive CSS, the component should have var(--decor) inline
                        // styles. We must merge the existing inline styles (if any).
                        Attribute::KeyValue(key, Some(AttributeValue::Literal(lit)))
                            if key == &"style" && inline_styles_candidate =>
                        {
                            let style = self.use_style_cache(&component);
                            let new_js = parse_text(&format!("`{lit} {style}`"), 0).syntax();
                            kvs.push((SmolStr::new_inline("style"), new_js));
                            has_style = true;
                        }

                        // The above case handled static inline styles, but if the programmer
                        // decides to use dynamic inline styles, we must merge their styles
                        // with outs
                        Attribute::KeyValue(key, Some(AttributeValue::JavaScript(js)))
                            if key == &"style" && inline_styles_candidate =>
                        {
                            let style = self.use_style_cache(&component);
                            let new_js = parse_text(&format!("`${{{js}}} {style}`"), 0).syntax();
                            kvs.push((SmolStr::new_inline("style"), new_js));
                            has_style = true;
                        }

                        Attribute::KeyValue(key, Some(AttributeValue::JavaScript(js))) => {
                            kvs.push((SmolStr::new(key), js.clone()));
                        }
                        Attribute::EventHandler(event_handler) => listeners.push((
                            SmolStr::new(event_handler.event()),
                            event_handler.expr().clone(),
                        )),

                        _ => {}
                    };
                }

                if !has_style && inline_styles_candidate {
                    let style = self.use_style_cache(&component);
                    kvs.push((
                        SmolStr::new_inline("style"),
                        parse_text(&format!("`{style}`"), 0).syntax(),
                    ))
                }

                if !kvs.is_empty() {
                    self.reactive_data
                        .key_values
                        .push((node.metadata(), kvs.into()));
                }
                if !listeners.is_empty() {
                    self.reactive_data
                        .event_listeners
                        .push((node.metadata(), listeners.into()));
                }
            }
            NodeType::Mustache(Mustache(js)) => {
                self.reactive_data
                    .mustaches
                    .push((node.metadata(), js.clone()));
            }
            NodeType::SpecialBlock(block) => self
                .reactive_data
                .special_blocks
                .push((node.metadata(), block)),
            _ => {}
        };
    }

    fn accumulated_output(self) -> Self::AccumulatedOutput {
        self.reactive_data
    }
}

impl<'ast> ReactiveData<'ast> {
    pub fn mustaches(&self) -> IdSlice<SyntaxNode> {
        self.mustaches.as_ref()
    }

    pub fn special_blocks(&self) -> IdSlice<&SpecialBlock<'_, FragmentMetadata>> {
        self.special_blocks.as_ref()
    }

    pub fn key_values(&self) -> IdSlice<Rc<[(SmolStr, SyntaxNode)]>> {
        self.key_values.as_ref()
    }

    pub fn event_listeners(&self) -> IdSlice<Rc<[(SmolStr, SyntaxNode)]>> {
        self.event_listeners.as_ref()
    }

    pub fn flat_listeners(
        &self,
    ) -> impl Iterator<Item = (&FragmentMetadata, &SmolStr, &SyntaxNode)> + '_ {
        self.event_listeners()
            .iter()
            .flat_map(|(meta, listeners)| listeners.iter().map(move |(ev, expr)| (*meta, ev, expr)))
    }

    pub fn flat_kvs(
        &self,
    ) -> impl Iterator<Item = (&FragmentMetadata, &SmolStr, &SyntaxNode)> + '_ {
        self.key_values()
            .iter()
            .flat_map(|(meta, kvs)| kvs.iter().map(move |(k, expr)| (*meta, k, expr)))
    }
}
