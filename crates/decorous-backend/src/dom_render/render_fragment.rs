use decorous_frontend::{
    ast::{
        Attribute, AttributeValue, CollapsedChildrenType, Element, ForBlock, IfBlock, Mustache,
        Node, NodeType, SpecialBlock, Text, UseBlock,
    },
    utils, Component, FragmentMetadata,
};
use heck::ToSnekCase;
use itertools::Itertools;
use std::{
    borrow::Cow,
    fmt::{Display, Write},
    io::{self, Write as IoWrite},
    str,
};

use crate::codegen_utils::{self, force_write, replace_namerefs, sort_if_testing};

macro_rules! default_mount_and_detach {
    ($state:expr, $out:expr, $meta:expr) => {
        let id = $meta.id();

        if $state.root == $meta.parent_id() {
            $out.write_detachln(format_args!("e{id}.parentNode.removeChild(e{id});"));
        }

        if $meta.parent_id() == $state.root {
            $out.write_mountln(format_args!("mount(target, e{id}, anchor);"));
        } else if let Some(parent_id) = $meta.parent_id() {
            $out.write_mountln(format_args!("e{parent_id}.appendChild(e{id});"));
        } else {
            panic!("BUG: node's parent should never be None while root is Some");
        }
    };
}

pub(crate) fn render_fragment<W>(
    nodes: &[Node<'_, FragmentMetadata>],
    mut state: State<'_>,
    out: &mut W,
) -> io::Result<()>
where
    W: io::Write,
{
    let mut output = Output::default();

    render_fragment_to_out(nodes, &mut state, &mut output);

    write!(
        out,
        include_str!("./templates/fragment.js"),
        id = state.name,
        decls = unsafe { str::from_utf8_unchecked(&output.decls) },
        mounts = unsafe { str::from_utf8_unchecked(&output.mounts) },
        update_body = unsafe { str::from_utf8_unchecked(&output.updates) },
        detach_body = unsafe { str::from_utf8_unchecked(&output.detaches) }
    )
}

fn render_fragment_to_out(
    nodes: &[Node<'_, FragmentMetadata>],
    state: &mut State<'_>,
    out: &mut Output,
) {
    for (block, id) in state.component.declared_vars.all_reactive_blocks() {
        let unbound = utils::get_unbound_refs(block);
        let dirty = codegen_utils::calc_dirty(&unbound, &state.component.declared_vars, None);
        out.write_updateln(format_args!("if ({dirty}) {{ ctx[{id}](); }}"));
    }

    for node in nodes {
        node.render(state, out, &());
    }

    if state.root.is_none() {
        render_reactive_css(state, out);
    }
}

#[derive(Debug, Clone)]
pub(crate) struct State<'ast> {
    pub component: &'ast Component<'ast>,
    #[allow(unused)]
    pub name: Cow<'static, str>,
    pub root: Option<u32>,
    pub uses: Vec<String>,
}

#[derive(Debug, Default)]
pub struct Output {
    decls: Vec<u8>,
    mounts: Vec<u8>,
    updates: Vec<u8>,
    detaches: Vec<u8>,
}

impl io::Write for Output {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.decls.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.decls.flush()
    }
}

impl Output {
    fn write_declln(&mut self, b: impl Display) {
        let _ = writeln!(self.decls, "{b}");
    }

    fn write_mountln(&mut self, b: impl Display) {
        let _ = writeln!(self.mounts, "{b}");
    }

    fn write_updateln(&mut self, b: impl Display) {
        let _ = writeln!(self.updates, "{b}");
    }

    fn write_detachln(&mut self, b: impl Display) {
        let _ = writeln!(self.detaches, "{b}");
    }
}

trait Render {
    type Metadata;

    fn render(&self, state: &mut State, out: &mut Output, meta: &Self::Metadata);
}

impl Render for Node<'_, FragmentMetadata> {
    type Metadata = ();

    fn render(&self, state: &mut State, out: &mut Output, _meta: &Self::Metadata) {
        match &self.node_type {
            NodeType::Text(t) => t.render(state, out, &self.metadata),
            NodeType::Mustache(m) => m.render(state, out, &self.metadata),
            NodeType::Element(elem) => elem.render(state, out, &self.metadata),
            NodeType::SpecialBlock(block) => block.render(state, out, &self.metadata),
            NodeType::Comment(_) => {}
        }
    }
}

impl Render for Text<'_> {
    type Metadata = FragmentMetadata;

    fn render(&self, state: &mut State, out: &mut Output, meta: &Self::Metadata) {
        out.write_declln(format_args!(
            "const e{} = document.createTextNode(\"{}\");",
            meta.id(),
            collapse_whitespace(self.0)
        ));

        default_mount_and_detach!(state, out, meta);
    }
}

impl Render for Mustache {
    type Metadata = FragmentMetadata;

    fn render(&self, state: &mut State, out: &mut Output, meta: &Self::Metadata) {
        let unbound = utils::get_unbound_refs(&self.0);
        let replaced = codegen_utils::replace_namerefs(
            &self.0,
            &unbound,
            &state.component.declared_vars,
            meta.scope(),
        );
        let id = meta.id();

        // Decl
        out.write_declln(format_args!(
            "const e{id} = document.createTextNode({replaced});"
        ));

        // Update
        let dirty =
            codegen_utils::calc_dirty(&unbound, &state.component.declared_vars, meta.scope());
        if !dirty.is_empty() {
            out.write_updateln(format_args!("if ({dirty}) e{id}.data = {replaced};"));
        }

        default_mount_and_detach!(state, out, meta);
    }
}

impl Render for Element<'_, FragmentMetadata> {
    type Metadata = FragmentMetadata;

    fn render(&self, state: &mut State, out: &mut Output, meta: &Self::Metadata) {
        let id = meta.id();

        let js_name = self.js_valid_tag_name();
        if state.uses.iter().any(|tag| tag == js_name.as_ref()) {
            out.write_declln(format_args!(
                "const e{id}_anchor = document.createTextNode(\"\");"
            ));
            if meta.parent_id() == state.root {
                out.write_mountln(format_args!("mount(target, e{id}_anchor, anchor);"));
            } else if let Some(parent_id) = meta.parent_id() {
                out.write_mountln(format_args!("e{parent_id}.appendChild(e{id}_anchor);"));
            } else {
                panic!("BUG: node's parent should never be None while root is Some");
            }
            out.write_mountln(format_args!("__decor_{js_name}(target, e{id}_anchor);"));
            if state.root != meta.parent_id() {
                out.write_detachln(format_args!(
                    "e{id}_anchor.parentNode.removeChild(e{id}_anchor);"
                ));
            }

            return;
        }

        // Decl
        out.write_declln(format_args!(
            "const e{id} = document.createElement(\"{}\");",
            self.tag
        ));
        match collapse_children(self) {
            Some(CollapsedChildrenType::Text(t)) => {
                out.write_declln(format_args!(
                    "e{id}.textContent = \"{}\";",
                    collapse_whitespace(t)
                ));
            }
            Some(CollapsedChildrenType::Html(html)) => {
                out.write_declln(format_args!("e{id}.innerHTML = `{html}`;"));
            }
            None => {
                for child in &self.children {
                    child.render(state, out, &());
                }
            }
        }
        for attr in &self.attrs {
            attr.render(state, out, meta);
        }

        default_mount_and_detach!(state, out, meta);
    }
}

impl Render for SpecialBlock<'_, FragmentMetadata> {
    type Metadata = FragmentMetadata;

    fn render(&self, state: &mut State, out: &mut Output, meta: &Self::Metadata) {
        match self {
            Self::If(if_block) => if_block.render(state, out, meta),
            Self::For(for_block) => for_block.render(state, out, meta),
            Self::Use(use_block) => use_block.render(state, out, meta),
        }
    }
}

impl Render for ForBlock<'_, FragmentMetadata> {
    type Metadata = FragmentMetadata;

    fn render(&self, state: &mut State, out: &mut Output, meta: &Self::Metadata) {
        let id = meta.id();

        render_fragment(
            &self.inner,
            State {
                name: id.to_string().into(),
                root: Some(id),
                uses: vec![],
                ..*state
            },
            out,
        )
        .expect("write to memory should not fail");

        // Decl
        out.write_declln(format_args!(
            "const e{id}_anchor = document.createTextNode(\"\");"
        ));

        // Mount
        let unbound = utils::get_unbound_refs(&self.expr);
        let expr = codegen_utils::replace_namerefs(
            &self.expr,
            &unbound,
            &state.component.declared_vars,
            meta.scope(),
        );
        let var_idx = state
            .component
            .declared_vars
            .all_scopes()
            .get(&id)
            .unwrap()
            .get(self.binding)
            .unwrap();
        out.write_mountln(format_args!("mount(target, e{id}_anchor, anchor);"));
        out.write_mountln(format_args!("let e{id}_blocks = [];\nlet i = 0;\nfor (const v of ({expr})) {{ ctx[{var_idx}] = v; e{id}_blocks[i] = create_{id}_block(e{id}_anchor.parentNode, e{id}_anchor); i += 1; }}"));

        // Update
        out.write_updateln(format_args!("let i = 0; for (const v of ({expr})) {{ if (i >= e{id}_blocks.length) {{ e{id}_blocks[i] = create_{id}_block(e{id}_anchor.parentNode, e{id}_anchor) }}; ctx[{var_idx}] = v; e{id}_blocks[i].u(dirty); i += 1; }} e{id}_blocks.slice(i).forEach(b => b.d()); e{id}_blocks.length = i;"));

        // Detach
        out.write_detachln(format_args!("for (let i = 0; i < e{id}_blocks.length; i++) {{ e{id}_blocks[i].d() }}\ne{id}_anchor.parentNode.removeChild(e{id}_anchor);"));
    }
}

impl Render for UseBlock<'_> {
    type Metadata = FragmentMetadata;

    fn render(&self, state: &mut State, _out: &mut Output, _meta: &Self::Metadata) {
        let Some(name) = self.path.file_stem() else {
            return;
        };

        state.uses.push(name.to_string_lossy().to_snek_case());
    }
}

impl Render for IfBlock<'_, FragmentMetadata> {
    type Metadata = FragmentMetadata;

    fn render(&self, state: &mut State, out: &mut Output, meta: &Self::Metadata) {
        let id = meta.id();
        let unbound = utils::get_unbound_refs(&self.expr);
        let replacement = codegen_utils::replace_namerefs(
            &self.expr,
            &unbound,
            &state.component.declared_vars,
            meta.scope(),
        );

        let _ = render_fragment(
            &self.inner,
            State {
                name: id.to_string().into(),
                root: Some(id),
                uses: vec![],
                ..*state
            },
            out,
        );
        if let Some(else_block) = &self.else_block {
            let _ = render_fragment(
                else_block,
                State {
                    name: format!("{id}_else").into(),
                    root: Some(id),
                    uses: vec![],
                    ..*state
                },
                out,
            );
        }

        // Decl
        out.write_declln(format_args!(
            "const e{id}_anchor = document.createTextNode(\"\");"
        ));

        // Mount
        out.write_mountln(format_args!("mount(target, e{id}_anchor, anchor);"));

        if self.else_block.is_some() {
            out.write_mountln(format_args!("let e{id};\nlet e{id}_on = false;\nif ({replacement}) {{ e{id} = create_{id}_block(e{id}_anchor.parentNode, e{id}_anchor); e{id}_on = true; }} else {{ e{id} = create_{id}_else_block(e{id}_anchor.parentNode, e{id}_anchor); }}"));
            out.write_updateln(format_args!("if ({replacement}) {{ if (e{id} && e{id}_on) {{ e{id}.u(dirty); }} else {{ e{id}_on = true; e{id}.d(); e{id} = create_{id}_block(e{id}_anchor.parentNode, e{id}_anchor); }} }} else if (e{id}_on) {{ e{id}_on = false; e{id}.d(); e{id} = create_{id}_else_block(e{id}_anchor.parentNode, e{id}_anchor); }}"));
        } else {
            out.write_mountln(format_args!("let e{id} = {replacement} && create_{id}_block(e{id}_anchor.parentNode, e{id}_anchor);"));
            out.write_updateln(format_args!("if ({replacement}) {{ if (e{id}) {{ e{id}.u(dirty); }} else {{ e{id} = create_{id}_block(e{id}_anchor.parentNode, e{id}_anchor); }} }} else if (e{id}) {{ e{id}.d(); e{id} = null; }}"));
        }

        // Detach
        out.write_detachln(format_args!(
            "if (e{id}) e{id}.d();\ne{id}_anchor.parentNode.removeChild(e{id}_anchor);"
        ));
    }
}

impl Render for Attribute<'_> {
    type Metadata = FragmentMetadata;

    fn render(&self, state: &mut State, out: &mut Output, meta: &Self::Metadata) {
        let id = meta.id();

        match self {
            Self::KeyValue(key, Some(AttributeValue::JavaScript(js))) => {
                let unbound = utils::get_unbound_refs(js);
                let replacement = codegen_utils::replace_namerefs(
                    js,
                    &unbound,
                    &state.component.declared_vars,
                    meta.scope(),
                );
                out.write_declln(format_args!(
                    "e{id}.setAttribute(\"{key}\", {replacement});"
                ));
                let dirty = codegen_utils::calc_dirty(
                    &unbound,
                    &state.component.declared_vars,
                    meta.scope(),
                );
                out.write_updateln(format_args!(
                    "if ({dirty}) e{id}.setAttribute(\"{key}\", {replacement});"
                ));
            }
            Self::KeyValue(key, None) => {
                out.write_declln(format_args!("e{id}.setAttribute(\"{key}\", \"\")"));
            }
            Self::KeyValue(key, Some(AttributeValue::Literal(literal))) => {
                out.write_declln(format_args!(
                    "e{id}.setAttribute(\"{key}\", \"{}\")",
                    collapse_whitespace(literal)
                ));
            }

            Self::EventHandler(event_handler) => {
                let unbound = utils::get_unbound_refs(&event_handler.expr);
                let replaced = codegen_utils::replace_namerefs(
                    &event_handler.expr,
                    &unbound,
                    &state.component.declared_vars,
                    meta.scope(),
                );
                // Scope args holds the amount of unbound variables in the expression that
                // are from a scope (created by something like a {#for} block)
                let scope_args = unbound
                    .iter()
                    .filter_map(|nref| {
                        let tok = nref.ident_token().unwrap();
                        let Some(scope) = meta.scope() else {
                            return None;
                        };
                        if !state
                            .component
                            .declared_vars
                            .is_scope_var(tok.text(), scope)
                        {
                            return None;
                        }
                        state
                            .component
                            .declared_vars
                            .get_var(tok.text(), meta.scope())
                    })
                    .collect_vec();

                // In the case scope_args is empty, attach the event handler as normal
                if scope_args.is_empty() {
                    out.write_declln(format_args!(
                        "e{id}.addEventListener(\"{}\", {replaced})",
                        event_handler.event
                    ));

                    return;
                }

                const ARG_LEN: usize = "arg0".len();
                let mut added_args = String::with_capacity(scope_args.len() * ARG_LEN);
                for (i, arg_idx) in scope_args.iter().enumerate() {
                    out.write_declln(format_args!("const arg{i} = ctx[{arg_idx}];"));
                    force_write!(added_args, "arg{i},");
                }
                out.write_declln(format_args!("e{id}.addEventListener(\"{}\", (...args) => {replaced}({added_args} ...args));", event_handler.event));
            }

            Self::Binding(binding) => {
                state
                    .component
                    .declared_vars
                    .get_var(*binding, None)
                    .map_or_else(
                        || todo!("unbound var lint"),
                        |var_id| {
                            out.write_declln(format_args!("e{id}.value = ctx[{var_id}];"));

                            let dirty_idx = ((var_id + 7) / 8).saturating_sub(1) as usize;
                            let bitmask = 1 << (var_id % 8);
                            out.write_updateln(format_args!(
                                "if (dirty[{dirty_idx}] & {bitmask}) e{id}.value = ctx[{var_id}];"
                            ));
                        },
                    );
                let binding_idx = state
                    .component
                    .declared_vars
                    .get_binding(*binding)
                    .expect("BUG: every binding should have a entry in declared vars");
                out.write_declln(format_args!(
                    "e{id}.addEventListener(\"input\", ctx[{binding_idx}]);"
                ));
            }
        }
    }
}

fn render_reactive_css(state: &mut State, output: &mut Output) {
    // No reactive CSS
    if state.component.declared_vars.css_mustaches().is_empty() {
        return;
    }

    let mut all_unbound = vec![];
    let mut final_attr = "`".to_owned();
    for (mustache, id) in sort_if_testing!(
        state.component.declared_vars.css_mustaches().iter(),
        |a, b| a.1.cmp(b.1)
    ) {
        let unbound = utils::get_unbound_refs(mustache);
        let replacement =
            replace_namerefs(mustache, &unbound, &state.component.declared_vars, None);
        all_unbound.extend(unbound);
        force_write!(final_attr, "--decor-{}: ${{{}}}; ", id, replacement);
    }
    final_attr.push('`');
    let all_dirty = codegen_utils::calc_dirty(&all_unbound, &state.component.declared_vars, None);
    output.write_updateln(format_args!(
        "if ({all_dirty}) target.setAttribute(\"style\", {final_attr});"
    ));
    output.write_mountln(format_args!(
        "target.setAttribute(\"style\", {final_attr});"
    ));
}

fn collapse_whitespace(s: &str) -> Cow<str> {
    match s {
        "\n" | "\r\n" => Cow::Borrowed(" "),
        s if s.contains('\n') || s.contains("\r\n") => {
            let mut joined = String::with_capacity(s.len());
            // We use .lines() to also account for \r\n
            for line in s.lines() {
                force_write!(joined, "{line}\\n");
            }

            joined.into()
        }
        s => Cow::Borrowed(s),
    }
}

fn collapse_children<'a>(
    elem: &'a Element<'a, FragmentMetadata>,
) -> Option<CollapsedChildrenType<'a>> {
    if elem.children.len() == 1 {
        if let NodeType::Text(t) = elem.children.first().unwrap().node_type {
            return Some(CollapsedChildrenType::Text(&t));
        }
    }
    if !elem.children.is_empty()
        && elem.descendents().all(|node| match &node.node_type {
            NodeType::Text(_) | NodeType::Comment(_) => true,
            // For elements, check if any attributes have mustache tags
            NodeType::Element(elem) => elem.attrs.iter().all(|attr| match attr {
                Attribute::KeyValue(_, None) => true,
                Attribute::KeyValue(_, Some(val)) => {
                    matches!(val, AttributeValue::Literal(_))
                }
                Attribute::Binding(_) | Attribute::EventHandler(_) => false,
            }),
            NodeType::Mustache(_) | NodeType::SpecialBlock(_) => false,
        })
    {
        return Some(CollapsedChildrenType::Html(elem.children.iter().join("")));
    }

    None
}
