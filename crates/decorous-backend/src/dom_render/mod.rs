use decorous_frontend::{
    ast::{
        traverse_with, Attribute, AttributeValue, CollapsedChildrenType, Node, NodeType,
        SpecialBlock,
    },
    utils, Component, DeclaredVariables, FragmentMetadata,
};
use itertools::Itertools;
use rslint_parser::AstNode;
use std::{borrow::Cow, fmt::Write, io};

use crate::codegen_utils;

pub fn render<T: io::Write>(component: &Component, render_to: &mut T) -> io::Result<()> {
    // Hoisted syntax nodes should come first
    for hoist in component.hoist() {
        writeln!(render_to, "{hoist}")?;
    }

    writeln!(
        render_to,
        "const dirty = new Uint8Array(new ArrayBuffer({}));",
        // Ceiling division to get the amount of bytes needed in the ArrayBuffer
        ((component.declared_vars().len() + 7) / 8)
    )?;
    write!(
        render_to,
        "{}",
        render_fragment(
            component.fragment_tree(),
            None,
            component.declared_vars(),
            "main"
        )
    )?;

    writeln!(render_to, "function __init_ctx() {{")?;
    writeln!(
        render_to,
        "{}",
        component
            .toplevel_nodes()
            .iter()
            .map(|node| {
                if node.substitute_assign_refs {
                    codegen_utils::replace_assignments(
                        &node.node,
                        &utils::get_unbound_refs(&node.node),
                        component.declared_vars(),
                    )
                } else {
                    node.node.to_string()
                }
            })
            .join("\n")
    )?;

    for (arrow_expr, idx) in component.declared_vars().all_arrow_exprs() {
        writeln!(
            render_to,
            "let __closure{idx} = {};",
            codegen_utils::replace_assignments(
                arrow_expr.syntax(),
                &utils::get_unbound_refs(arrow_expr.syntax()),
                component.declared_vars()
            )
        )?;
    }

    let mut ctx = vec![Cow::Borrowed(""); component.declared_vars().len()];
    for (name, idx) in component.declared_vars().all_vars() {
        ctx[*idx as usize] = Cow::Borrowed(name);
    }
    for idx in component.declared_vars().all_arrow_exprs().values() {
        ctx[*idx as usize] = Cow::Owned(format!("__closure{idx}"));
    }
    writeln!(render_to, "return [{}];", ctx.join(","))?;
    writeln!(render_to, "}}")?;

    writeln!(render_to, "const ctx = __init_ctx();")?;
    writeln!(
        render_to,
        "const fragment = create_main_block(document.getElementById(\"app\"));"
    )?;
    writeln!(render_to, "let updating = false;")?;
    writeln!(
        render_to,
        "function __schedule_update(ctx_idx, val) {{
ctx[ctx_idx] = val;
dirty[Math.max(Math.ceil(ctx_idx / 8) - 1, 0)] |= 1 << (ctx_idx % 8);
if (updating) return;
updating = true;
Promise.resolve().then(() => {{
fragment.u(dirty);
updating = false;
dirty.fill(0);
}});
}}"
    )?;

    Ok(())
}

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
        NodeType::Text(t) => writeln!(
            f,
            "const e{id} = document.createTextNode(\"{}\");",
            collapse_whitespace(t)
        )
        .expect("string format should not fail"),
        NodeType::Mustache(mustache) => {
            let replaced = codegen_utils::replace_namerefs(
                mustache,
                &utils::get_unbound_refs(mustache),
                declared,
            );
            writeln!(f, "const e{id} = document.createTextNode({replaced});")
                .expect("string format should not fail");
        }
        NodeType::Element(elem) => {
            writeln!(
                f,
                "const e{id} = document.createElement(\"{}\");",
                elem.tag()
            )
            .expect("string format should not fail");

            match elem.inner_collapsed() {
                Some(CollapsedChildrenType::Text(t)) => {
                    writeln!(f, "e{id}.textContent = \"{}\";", collapse_whitespace(t))
                }
                Some(CollapsedChildrenType::Html(html)) => {
                    writeln!(f, "e{id}.innerHtml = \"{html}\";")
                }
                None => Ok(()),
            }
            .expect("string format should not fail");

            for attr in elem.attrs() {
                match attr {
                    Attribute::KeyValue(key, Some(AttributeValue::JavaScript(js))) => writeln!(
                        f,
                        "e{id}.setAttribute(\"{key}\", {});",
                        codegen_utils::replace_namerefs(js, &utils::get_unbound_refs(js), declared)
                    )
                    .expect("string format should not fail"),
                    Attribute::KeyValue(key, None) => {
                        writeln!(f, "e{id}.setAttribute(\"{key}\", \"\")")
                            .expect("string format should not fail");
                    }
                    Attribute::KeyValue(key, Some(AttributeValue::Literal(literal))) => {
                        writeln!(
                            f,
                            "e{id}.setAttribute(\"{key}\", \"{}\")",
                            collapse_whitespace(literal)
                        )
                        .expect("string format should not fail");
                    }
                    Attribute::EventHandler(event_handler) => {
                        writeln!(
                            f,
                            "e{id}.addEventListener(\"{}\", {})",
                            event_handler.event(),
                            codegen_utils::replace_namerefs(
                                event_handler.expr(),
                                &utils::get_unbound_refs(event_handler.expr()),
                                declared
                            )
                        )
                        .expect("string format should not fail");
                    }
                    Attribute::Binding(_) => todo!(),
                }
            }
        }
        NodeType::SpecialBlock(SpecialBlock::If(_)) => {
            writeln!(f, "const e{id}_anchor = document.createTextNode(\"\");")
                .expect("string format should not fail");
        }
        NodeType::SpecialBlock(SpecialBlock::For(_)) => todo!("for block"),
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

    // In the special case of an if block, create AND mount here
    if let NodeType::SpecialBlock(SpecialBlock::If(if_block)) = node.node_type() {
        let block = render_fragment(if_block.inner(), Some(id), declared, &id.to_string());
        writeln!(f, "{block}").expect("string format should not fail");
        if let Some(else_block) = if_block.else_block() {
            let block = render_fragment(else_block, Some(id), declared, &format!("{id}_else"));
            writeln!(f, "{block}").expect("string format should not fail");
        }
        let replacement = codegen_utils::replace_namerefs(
            if_block.expr(),
            &utils::get_unbound_refs(if_block.expr()),
            declared,
        );

        writeln!(f, "mount(target, e{id}_anchor, anchor);").expect("string format should not fail");
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

        return;
    }

    if node.metadata().parent_id() == root {
        writeln!(f, "mount(target, e{id}, anchor);").expect("string format should not fail");
    } else if let Some(parent_id) = node.metadata().parent_id() {
        writeln!(f, "e{parent_id}.appendChild(e{id});").expect("string format should not fail;");
    } else {
        panic!("BUG: node's parent should never be None while root is Some");
    }
}

fn render_update(f: &mut String, node: &Node<'_, FragmentMetadata>, declared: &DeclaredVariables) {
    let id = node.metadata().id();
    match node.node_type() {
        NodeType::Mustache(mustache) => {
            let unbound = utils::get_unbound_refs(mustache);
            let dirty_indices = codegen_utils::calc_dirty(&unbound, declared);
            let new_text = codegen_utils::replace_namerefs(mustache, &unbound, declared);
            writeln!(f, "if ({dirty_indices}) e{id}.data = {new_text};",)
                .expect("string format should work");
        }
        NodeType::Element(elem) => {
            for attr in elem.attrs() {
                let Attribute::KeyValue(key, Some(AttributeValue::JavaScript(js))) = attr else {
                    continue;
                };

                let unbound = utils::get_unbound_refs(js);
                let dirty_indices = codegen_utils::calc_dirty(&unbound, declared);
                let replacement = codegen_utils::replace_namerefs(js, &unbound, declared);
                writeln!(
                    f,
                    "if ({dirty_indices}) e{id}.setAttribute(\"{key}\", {replacement});",
                )
                .expect("string format should not fail");
            }
        }
        NodeType::SpecialBlock(SpecialBlock::If(if_block)) => {
            let unbound = utils::get_unbound_refs(if_block.expr());
            let replaced = codegen_utils::replace_namerefs(if_block.expr(), &unbound, declared);
            if if_block.else_block().is_some() {
                writeln!(
                        f,
                        "if ({replaced}) {{ if (e{id} && e{id}_on) {{ e{id}.u(dirty); }} else {{ e{id}_on = true; e{id}.d(); e{id} = create_{id}_block(e{id}_anchor.parentNode, e{id}_anchor); }} }} else if (e{id}_on) {{ e{id}_on = false; e{id}.d(); e{id} = create_{id}_else_block(e{id}_anchor.parentNode, e{id}_anchor); }}"
                    )
                    .expect("string formatting should not fail");
            } else {
                writeln!(
                        f,
                        "if ({replaced}) {{ if (e{id}) {{ e{id}.u(dirty); }} else {{ e{id} = create_{id}_block(e{id}_anchor.parentNode, e{id}_anchor); }} }} else if (e{id}) {{ e{id}.d(); e{id} = null; }}"
                    )
                    .expect("string formatting should not fail");
            }
        }
        _ => {}
    }
}

fn render_detach(f: &mut String, node: &Node<'_, FragmentMetadata>, root: Option<u32>) {
    if matches!(node.node_type(), NodeType::Comment(_)) || root != node.metadata().parent_id() {
        return;
    }
    if let NodeType::SpecialBlock(SpecialBlock::If(_)) = node.node_type() {
        writeln!(
            f,
            "if (e{}) e{0}.d();\ne{0}_anchor.parentNode.removeChild(e{0}_anchor);",
            node.metadata().id()
        )
        .expect("string format should not fail");
        return;
    }

    writeln!(f, "e{}.parentNode.removeChild(e{0});", node.metadata().id())
        .expect("string format should not fail");
}

fn collapse_whitespace(s: &str) -> Cow<str> {
    match s {
        "\n" => Cow::Borrowed(" "),
        s if s.contains('\n') => Cow::Owned(s.replace('\n', "\\n")),
        s => Cow::Borrowed(s),
    }
}

#[cfg(test)]
mod tests {
    use decorous_frontend::parse;

    use super::*;

    fn make_component(input: &str) -> Component {
        Component::new(parse(input).expect("should be valid input"))
    }

    macro_rules! test_render {
        ($input:expr) => {
            let component = make_component($input);
            let mut out = vec![];
            render(&component, &mut out).unwrap();

            insta::assert_snapshot!(String::from_utf8(out).unwrap());
        };
    }

    #[test]
    fn basic_render_works() {
        test_render!("---js let x = 3; function remake_x() { x = 44; } --- #p {`${x}hello`} /p #button[@click={remake_x}]:Click me");
    }

    #[test]
    fn render_with_attrs_works() {
        test_render!("---js let x = 3; function remake_x() { x = 44; } --- #div[class={x}]/div");
    }

    #[test]
    fn render_with_event_listeners_works() {
        test_render!(
            "---js let x = 3; --- #p {x} /p #button[@click={() => {if (x == 3) { x = 44; }} }]Clickme /button"
        );
    }

    #[test]
    fn imports_are_hoisted_out_of_context_init() {
        test_render!("---js import data from \"data\"; let x = 3; --- #p {x} /p");
    }

    #[test]
    fn everything_in_script_tag_is_in_context_init() {
        test_render!("---js let x = 3; x = 4; ---");
    }

    #[test]
    fn can_write_basic_text_nodes() {
        test_render!("hello");
    }

    #[test]
    fn basic_elements_and_html_are_collapsed() {
        test_render!("#div text #div/div /div");
    }

    #[test]
    fn can_write_mustache_tags() {
        test_render!("---js let x = 0; --- {(x, x)}");
    }

    #[test]
    fn text_with_only_one_text_node_as_child_is_collapsed() {
        test_render!("#span:hello");
    }

    #[test]
    fn dirty_items_are_in_conditional() {
        test_render!("---js let hello = 0; let test = 1; --- {(hello, test)}");
    }

    #[test]
    fn can_render_else_blocks() {
        test_render!("---js let hello = 0; --- {#if hello == 0} wow {:else} woah {/if}");
    }
}
