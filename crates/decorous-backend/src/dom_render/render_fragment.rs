use decorous_frontend::{
    ast::{
        traverse_with, Attribute, AttributeValue, CollapsedChildrenType, Node, NodeType,
        SpecialBlock,
    },
    utils, DeclaredVariables, FragmentMetadata,
};
use itertools::Itertools;
use std::{borrow::Cow, fmt::Write};

use crate::codegen_utils::{self, force_write, force_writeln};

pub(crate) fn render_fragment(
    nodes: &[Node<'_, FragmentMetadata>],
    root: Option<u32>,
    declared: &DeclaredVariables,
    name: &str,
) -> String {
    let mut decls = String::new();
    let mut mounts = String::new();
    let mut updates = String::new();
    let mut detaches = String::new();
    traverse_with(
        nodes,
        &mut |elem| elem.inner_collapsed().is_none(),
        &mut |node| {
            render_decl(&mut decls, node, declared);
            render_mount(&mut mounts, node, root, declared);
            render_update(&mut updates, node, declared);
            render_detach(&mut detaches, node, root);
        },
    );

    let rendered = format!(
        include_str!("./templates/fragment.js"),
        id = name,
        decls = decls,
        mounts = mounts,
        update_body = updates,
        detach_body = detaches
    );

    rendered
}

fn render_decl(f: &mut String, node: &Node<'_, FragmentMetadata>, declared: &DeclaredVariables) {
    let id = node.metadata().id();
    match node.node_type() {
        NodeType::Text(t) => force_writeln!(
            f,
            "const e{id} = document.createTextNode(\"{}\");",
            collapse_whitespace(t)
        ),
        NodeType::Mustache(mustache) => {
            let replaced = codegen_utils::replace_namerefs(
                mustache,
                &utils::get_unbound_refs(mustache),
                declared,
                node.metadata().scope(),
            );
            force_writeln!(f, "const e{id} = document.createTextNode({replaced});");
        }
        NodeType::Element(elem) => {
            force_writeln!(
                f,
                "const e{id} = document.createElement(\"{}\");",
                elem.tag()
            );

            match elem.inner_collapsed() {
                Some(CollapsedChildrenType::Text(t)) => {
                    writeln!(f, "e{id}.textContent = \"{}\";", collapse_whitespace(t))
                }
                Some(CollapsedChildrenType::Html(html)) => {
                    writeln!(f, "e{id}.innerHTML = \"{html}\";")
                }
                None => Ok(()),
            }
            .expect("string format should not fail");

            for attr in elem.attrs() {
                match attr {
                    Attribute::KeyValue(key, Some(AttributeValue::JavaScript(js))) => {
                        force_writeln!(
                            f,
                            "e{id}.setAttribute(\"{key}\", {});",
                            codegen_utils::replace_namerefs(
                                js,
                                &utils::get_unbound_refs(js),
                                declared,
                                node.metadata().scope()
                            )
                        );
                    }
                    Attribute::KeyValue(key, None) => {
                        force_writeln!(f, "e{id}.setAttribute(\"{key}\", \"\")");
                    }
                    Attribute::KeyValue(key, Some(AttributeValue::Literal(literal))) => {
                        force_writeln!(
                            f,
                            "e{id}.setAttribute(\"{key}\", \"{}\")",
                            collapse_whitespace(literal)
                        );
                    }
                    Attribute::EventHandler(event_handler) => {
                        let unbound = utils::get_unbound_refs(event_handler.expr());
                        let replaced = codegen_utils::replace_namerefs(
                            event_handler.expr(),
                            &unbound,
                            declared,
                            node.metadata().scope(),
                        );
                        // Scope args holds the amount of unbound variables in the expression that
                        // are from a scope (created by something like a {#for} block)
                        let scope_args = unbound
                            .iter()
                            .filter_map(|nref| {
                                let tok = nref.ident_token().unwrap();
                                let Some(scope) = node.metadata().scope() else {
                                    return None;
                                };
                                if !declared.is_scope_var(tok.text(), scope) {
                                    return None;
                                }
                                declared.get_var(tok.text(), node.metadata().scope())
                            })
                            .collect_vec();

                        // In the case scope_args is empty, attach the event handler as normal
                        if scope_args.is_empty() {
                            force_writeln!(
                                f,
                                "e{id}.addEventListener(\"{}\", {})",
                                event_handler.event(),
                                replaced
                            );

                            return;
                        }

                        // In the case scope_args is not empty, there are a few things we need to
                        // do on the JS side.
                        // 1. Create a copy or reference of the scoped variable. This is because
                        //    scoped variables can be changed in a way that is not facilitated by
                        //    the user's JavaScript. For example, a for block changes the iteration
                        //    variable every iteration. If we didn't make a copy, the value would
                        //    be lost.
                        // 2. Create an event listener wrapper that passes in the scoped variable
                        //    copy. Scoped variables cannot be accessed in `__init_ctx`, becauese
                        //    they're ultimately not created in `__init_ctx`. Thus, they must be
                        //    passed in as arguments to the closures that use them. This means we
                        //    need to create a new event listener that wraps the users closure,
                        //    and always passes the scoped variable in for the first argument.
                        //    Then, we pass in the arguments that the user actually expected when
                        //    they created the closure.
                        let mut added_args = String::new();
                        for (i, arg_idx) in scope_args.iter().enumerate() {
                            force_writeln!(f, "const arg{i} = ctx[{arg_idx}];");
                            force_write!(added_args, "arg{i},");
                        }
                        force_writeln!(
                            f,
                            "e{id}.addEventListener(\"{}\", (...args) => {}({added_args} ...args))",
                            event_handler.event(),
                            replaced
                        );
                    }
                    Attribute::Binding(_) => todo!(),
                }
            }
        }
        NodeType::SpecialBlock(_) => {
            force_writeln!(f, "const e{id}_anchor = document.createTextNode(\"\");");
        }
        NodeType::Comment(_) => {}
    }
}

fn render_mount(
    f: &mut String,
    node: &Node<'_, FragmentMetadata>,
    root: Option<u32>,
    declared: &DeclaredVariables,
) {
    let id = node.metadata().id();

    match node.node_type() {
        // In the special case of an if block, create AND mount here
        NodeType::SpecialBlock(SpecialBlock::If(if_block)) => {
            let block = render_fragment(if_block.inner(), Some(id), declared, &id.to_string());
            force_writeln!(f, "{block}");
            if let Some(else_block) = if_block.else_block() {
                let block = render_fragment(else_block, Some(id), declared, &format!("{id}_else"));
                force_writeln!(f, "{block}");
            }

            let replacement = codegen_utils::replace_namerefs(
                if_block.expr(),
                &utils::get_unbound_refs(if_block.expr()),
                declared,
                node.metadata().scope(),
            );

            force_writeln!(f, "mount(target, e{id}_anchor, anchor);");
            if if_block.else_block().is_some() {
                writeln!(
                    f,
                    "let e{id};\nlet e{id}_on = false;\nif ({replacement}) {{ e{id} = create_{id}_block(e{id}_anchor.parentNode, e{id}_anchor); e{id}_on = true; }} else {{ e{id} = create_{id}_else_block(e{id}_anchor.parentNode, e{id}_anchor); }}",
                )
            } else {
                writeln!(
                    f,
                    "let e{id} = {replacement} && create_{id}_block(e{id}_anchor.parentNode, e{id}_anchor);"
                )
            }
            .expect("string format should not fail");
        }

        // In the special case of a for block, create AND mount here
        NodeType::SpecialBlock(SpecialBlock::For(for_block)) => {
            let block = render_fragment(for_block.inner(), Some(id), declared, &id.to_string());
            force_writeln!(f, "{block}");

            let expr = codegen_utils::replace_namerefs(
                for_block.expr(),
                &utils::get_unbound_refs(for_block.expr()),
                declared,
                node.metadata().scope(),
            );

            let var_idx = declared
                .all_scopes()
                .get(&node.metadata().id())
                .unwrap()
                .get(for_block.binding())
                .unwrap();
            force_writeln!(f, "mount(target, e{id}_anchor, anchor);");
            force_writeln!(f,
            "let e{id}_blocks = [];\nlet i = 0;\nfor (const v of ({expr})) {{ ctx[{var_idx}] = v; e{id}_blocks[i] = create_{id}_block(e{id}_anchor.parentNode, e{id}_anchor); i += 1; }}");
        }

        _ => {
            if node.metadata().parent_id() == root {
                force_writeln!(f, "mount(target, e{id}, anchor);");
            } else if let Some(parent_id) = node.metadata().parent_id() {
                force_writeln!(f, "e{parent_id}.appendChild(e{id});");
            } else {
                panic!("BUG: node's parent should never be None while root is Some");
            }
        }
    }
}

fn render_update(f: &mut String, node: &Node<'_, FragmentMetadata>, declared: &DeclaredVariables) {
    let id = node.metadata().id();
    match node.node_type() {
        NodeType::Mustache(mustache) => {
            let unbound = utils::get_unbound_refs(mustache);
            let dirty_indices =
                codegen_utils::calc_dirty(&unbound, declared, node.metadata().scope());
            let new_text = codegen_utils::replace_namerefs(
                mustache,
                &unbound,
                declared,
                node.metadata().scope(),
            );
            if !dirty_indices.is_empty() {
                force_writeln!(f, "if ({dirty_indices}) e{id}.data = {new_text};");
            }
        }

        NodeType::Element(elem) => {
            for attr in elem.attrs() {
                let Attribute::KeyValue(key, Some(AttributeValue::JavaScript(js))) = attr else {
                    continue;
                };

                let unbound = utils::get_unbound_refs(js);
                let dirty_indices =
                    codegen_utils::calc_dirty(&unbound, declared, node.metadata().scope());
                let replacement = codegen_utils::replace_namerefs(
                    js,
                    &unbound,
                    declared,
                    node.metadata().scope(),
                );
                if !dirty_indices.is_empty() {
                    force_writeln!(
                        f,
                        "if ({dirty_indices}) e{id}.setAttribute(\"{key}\", {replacement});"
                    );
                }
            }
        }

        NodeType::SpecialBlock(SpecialBlock::If(if_block)) => {
            let unbound = utils::get_unbound_refs(if_block.expr());
            let replaced = codegen_utils::replace_namerefs(
                if_block.expr(),
                &unbound,
                declared,
                node.metadata().scope(),
            );
            if if_block.else_block().is_some() {
                force_writeln!(f,
                "if ({replaced}) {{ if (e{id} && e{id}_on) {{ e{id}.u(dirty); }} else {{ e{id}_on = true; e{id}.d(); e{id} = create_{id}_block(e{id}_anchor.parentNode, e{id}_anchor); }} }} else if (e{id}_on) {{ e{id}_on = false; e{id}.d(); e{id} = create_{id}_else_block(e{id}_anchor.parentNode, e{id}_anchor); }}");
            } else {
                force_writeln!(f,
                "if ({replaced}) {{ if (e{id}) {{ e{id}.u(dirty); }} else {{ e{id} = create_{id}_block(e{id}_anchor.parentNode, e{id}_anchor); }} }} else if (e{id}) {{ e{id}.d(); e{id} = null; }}");
            }
        }

        NodeType::SpecialBlock(SpecialBlock::For(for_block)) => {
            let var_idx = declared
                .all_scopes()
                .get(&node.metadata().id())
                .unwrap()
                .get(for_block.binding())
                .unwrap();
            let unbound = utils::get_unbound_refs(for_block.expr());
            let expr = codegen_utils::replace_namerefs(
                for_block.expr(),
                &unbound,
                declared,
                node.metadata().scope(),
            );
            force_writeln!(f,
            "let i = 0; for (const v of ({expr})) {{ if (i >= e{id}_blocks.length) {{ e{id}_blocks[i] = create_{id}_block(e{id}_anchor.parentNode, e{id}_anchor) }}; ctx[{var_idx}] = v; e{id}_blocks[i].u(dirty); i += 1; }} e{id}_blocks.slice(i).forEach(b => b.d()); e{id}_blocks.length = i;");
        }

        _ => {}
    }
}

fn render_detach(f: &mut String, node: &Node<'_, FragmentMetadata>, root: Option<u32>) {
    // Only detach root elems
    if root != node.metadata().parent_id() {
        return;
    }
    let id = node.metadata().id();
    match node.node_type() {
        NodeType::SpecialBlock(SpecialBlock::If(_)) => {
            force_writeln!(f,
            "if (e{id}) e{id}.d();\ne{id}_anchor.parentNode.removeChild(e{id}_anchor);");
        }

        NodeType::SpecialBlock(SpecialBlock::For(_)) => force_writeln!(f,
        "for (let i = 0; i < e{id}_blocks.length; i++) {{ e{id}_blocks[i].d() }}\ne{id}_anchor.parentNode.removeChild(e{id}_anchor);"),

        NodeType::Comment(_) => {}

        _ => {
            force_writeln!(f, "e{}.parentNode.removeChild(e{0});", node.metadata().id());
        }
    }
}

fn collapse_whitespace(s: &str) -> Cow<str> {
    match s {
        "\n" => Cow::Borrowed(" "),
        s if s.contains('\n') => Cow::Owned(s.replace('\n', "\\n")),
        s => Cow::Borrowed(s),
    }
}
