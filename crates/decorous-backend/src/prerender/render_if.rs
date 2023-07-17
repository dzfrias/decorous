use decorous_frontend::{
    ast::{Attribute, AttributeValue, IfBlock, Node, NodeType, SpecialBlock},
    utils, DeclaredVariables, FragmentMetadata,
};
use std::{borrow::Cow, fmt::Write};

use crate::codegen_utils;

pub fn render_if_block<'a>(
    if_block: &IfBlock<'a, FragmentMetadata>,
    id: u32,
    declared: &DeclaredVariables,
) -> (String, Option<String>) {
    let mut decls = String::new();
    let mut mounts = String::new();
    let mut updates = String::new();
    let mut detaches = String::new();
    for node in if_block.inner_recursive() {
        render_decl(&mut decls, node, declared);
        render_mount(&mut mounts, node, id, declared);
        render_update(&mut updates, node, declared);
        render_detach(&mut detaches, node, id);
    }

    let rendered = format!(
        include_str!("./templates/if_block.js"),
        id = id,
        decls = decls,
        mounts = mounts,
        update_body = updates,
        detach_body = detaches
    );

    (rendered, None)
}

fn render_decl<'a>(
    f: &mut String,
    node: &Node<'a, FragmentMetadata>,
    declared: &DeclaredVariables,
) {
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
                &mustache,
                &utils::get_unbound_refs(&mustache),
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

fn render_mount<'a>(
    f: &mut String,
    node: &Node<'a, FragmentMetadata>,
    if_block_id: u32,
    declared: &DeclaredVariables,
) {
    let id = node.metadata().id();

    // In the special case of an if block, create AND mount here
    if let NodeType::SpecialBlock(SpecialBlock::If(if_block)) = node.node_type() {
        // TODO: Else blocks
        let (block, _) = render_if_block(if_block, id, declared);
        writeln!(f, "{block}").expect("string format should not fail");
        let replacement = codegen_utils::replace_namerefs(
            if_block.expr(),
            &utils::get_unbound_refs(if_block.expr()),
            declared,
        );

        writeln!(f, "mount(target, e{id}_anchor, anchor);").expect("string format should not fail");
        writeln!(
            f,
            "let e{id} = {replacement} && create_{id}_block(e{id}_anchor.parentNode, e{id}_anchor);"
        )
        .expect("string format should not fail");
        return;
    }

    let Some(parent_id) = node.metadata().parent_id() else {
        panic!("BUG: parent id should be present in every element of an if block. Offending node: {node:?}")
    };
    if parent_id == if_block_id {
        writeln!(f, "mount(target, e{id}, anchor);").expect("string format should not fail");
        return;
    }

    writeln!(f, "e{parent_id}.appendChild(e{id});").expect("string format should not fail;")
}

fn render_update<'a>(
    f: &mut String,
    node: &Node<'a, FragmentMetadata>,
    declared: &DeclaredVariables,
) {
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
            writeln!(
                    f,
                    "if ({replaced}) {{ if (e{id}) {{ e{id}.u(dirty); }} else {{ e{id} = create_{id}_block(e{id}_anchor.parentNode, e{id}_anchor); }} }} else if (e{id}) {{ e{id}.d(); e{id} = null; }}"
                )
                .expect("string formatting should not fail");
        }
        _ => {}
    }
}

fn render_detach<'a>(f: &mut String, node: &Node<'a, FragmentMetadata>, if_block_id: u32) {
    if matches!(node.node_type(), NodeType::Comment(_))
        || node
            .metadata()
            .parent_id()
            .expect("BUG: all elements of if block should have parent")
            != if_block_id
    {
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
