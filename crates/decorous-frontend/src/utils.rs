use rslint_parser::{
    ast::{
        ArrowExpr, ArrowExprParams, BlockStmt, Decl, Expr, ExprOrBlock, ExprStmt, NameRef,
        ObjectPatternProp, Pattern, Stmt,
    },
    AstNode, SmolStr, SyntaxNode, SyntaxNodeExt,
};

/// Get unbound variable references from a syntax element of the `rslint_parser` tree. This function
/// also takes scoped variables into account, so it is always correct.
pub fn get_unbound_refs(syntax_node: &SyntaxNode) -> Vec<NameRef> {
    if syntax_node.is::<ExprStmt>() {
        let mut declared = vec![];
        let mut all = vec![];

        let expr_stmt = syntax_node.to::<ExprStmt>();

        match expr_stmt.expr() {
            Some(expr) => get_unbound_refs_from_expr(expr, &mut declared, &mut all),
            None => return vec![],
        }

        return all;
    } else if syntax_node.is::<ArrowExpr>() {
        let mut declared = vec![];
        let mut all = vec![];
        get_unbound_refs_from_arrow_expr(syntax_node.to::<ArrowExpr>(), &mut declared, &mut all);
        return all;
    }

    let mut declared = vec![];
    let mut all = vec![];
    if syntax_node.is::<BlockStmt>() {
        get_unbound_refs_from_block(syntax_node.to(), &mut declared, &mut all);
    } else {
        // Special case when the syntax node is something not covered in the previous conditions.
        // Just find all the descendent blocks of the node and get their unbound refs.
        syntax_node.descendants_with(&mut |descendent| {
            if descendent.is::<BlockStmt>() {
                let old_len = declared.len();
                get_unbound_refs_from_block(descendent.to(), &mut declared, &mut all);
                declared.truncate(old_len);
                false
            } else {
                true
            }
        });
    }
    all
}

fn get_unbound_refs_from_arrow_expr(
    arrow_expr: ArrowExpr,
    already_declared: &mut Vec<SmolStr>,
    all: &mut Vec<NameRef>,
) {
    let old_len = already_declared.len();
    if let Some(ArrowExprParams::ParameterList(params)) = arrow_expr.params() {
        // Extend the scope to include the parameters of the function
        for param in params.parameters() {
            let idents = get_idents_from_pattern(param);
            already_declared.extend(idents);
        }
    };

    match arrow_expr.body() {
        Some(ExprOrBlock::Block(block)) => {
            get_unbound_refs_from_block(block, already_declared, all);
            already_declared.truncate(old_len);
        }
        Some(ExprOrBlock::Expr(expr)) => {
            get_unbound_refs_from_expr(expr, already_declared, all);
            already_declared.truncate(old_len);
        }
        _ => {
            // We still have to do this bookkeeping if the arrow function has no body.
            already_declared.truncate(old_len);
        }
    }
}

fn get_unbound_refs_from_expr(
    expr: Expr,
    already_declared: &mut Vec<SmolStr>,
    all: &mut Vec<NameRef>,
) {
    // For an arrow expression, we can make the assertion that there will be no special
    // scoping stuff that we have to handle, unless an arrow expression is found. In this case, we
    // can recursively find every single NameRef and call it unbound, unless we encounter an
    // ArrowExpr. In that case, our assertion doesn't hold true for anything inside that function,
    // and we must treat it as a normal scope.
    match expr {
        // Check if the expression itself is an ArrowExpr
        Expr::ArrowExpr(arrow_expr) => {
            get_unbound_refs_from_arrow_expr(arrow_expr, already_declared, all);
        }
        Expr::NameRef(name_ref) => all.push(name_ref),
        expr => {
            expr.syntax().descendants_with(&mut |node| {
                // Our first case. We know that any NameRef not in an ArrowExpr must be
                // unbound with the scope of the current expression.
                if node.is::<NameRef>() && !all.contains(&node.to::<NameRef>()) {
                    let name_ref = node.to::<NameRef>();
                    if name_ref
                        .ident_token()
                        .is_some_and(|ident| !already_declared.contains(ident.text()))
                    {
                        all.push(node.to());
                    }
                }
                // If arrow expr, there will be a scope created, so handle that case.
                if node.is::<ArrowExpr>() {
                    get_unbound_refs_from_arrow_expr(node.to(), already_declared, all);
                    // Return false to not visit descendants of the ArrowExpr
                    return false;
                }

                true
            });
        }
    }
}

fn get_unbound_refs_from_block(
    block: BlockStmt,
    already_declared: &mut Vec<SmolStr>,
    all: &mut Vec<NameRef>,
) {
    for stmt in block.stmts() {
        match stmt {
            // If a function declaration, add to list of declared items
            Stmt::Decl(Decl::FnDecl(fn_decl)) => {
                let Some(ident) = fn_decl.name().and_then(|name| name.ident_token()) else {
                    continue;
                };

                already_declared.push(ident.text().clone());
            }
            // If a variable declaration, add to list of declared items
            Stmt::Decl(Decl::VarDecl(var_decl)) => {
                for decl in var_decl.declared() {
                    let Some(pat) = decl.pattern() else {
                        continue;
                    };

                    let idents = get_idents_from_pattern(pat);
                    already_declared.extend(idents);
                }
            }
            Stmt::ExprStmt(expr) if expr.expr().is_some() => {
                get_unbound_refs_from_expr(expr.expr().unwrap(), already_declared, all);
            }
            stmt => stmt.syntax().descendants_with(&mut |node| {
                if node.is::<BlockStmt>() {
                    // old_len is kept to pop the declared variables that are added to
                    // `already_declared` in `get_unbound_refs_from_block`. This emulates a stack
                    // frame getting popped
                    let old_len = already_declared.len();
                    get_unbound_refs_from_block(node.to(), already_declared, all);
                    already_declared.truncate(old_len);
                    return false;
                }
                if node.is::<ArrowExpr>() {
                    let arrow_expr = node.to::<ArrowExpr>();
                    if arrow_expr.params().is_none() {
                        // Keep recursing until block is reached
                        return true;
                    }

                    get_unbound_refs_from_arrow_expr(arrow_expr, already_declared, all);
                    return false;
                }
                // In the case of a NameRef, check if it's been declared. If not, push to the
                // undeclared variables vector.
                if node.is::<NameRef>() {
                    let name_ref = node.to::<NameRef>();
                    if let Some(ident) = name_ref.ident_token() {
                        let s = ident.text();
                        if !already_declared.contains(s) {
                            all.push(name_ref);
                        }
                    }
                }
                true
            }),
        }
    }
}

/// Gets the identifiers from a pattern. This is useful for complex assignments.
pub fn get_idents_from_pattern(pat: Pattern) -> Vec<SmolStr> {
    let mut idents = vec![];

    match pat {
        Pattern::SinglePattern(single) => {
            if let Some(ident) = single
                .name()
                .and_then(|name| name.ident_token())
                .map(|ident| ident.text().clone())
            {
                idents.push(ident);
            }
        }
        Pattern::ArrayPattern(array) => {
            idents.extend(array.elements().flat_map(get_idents_from_pattern));
        }
        Pattern::RestPattern(rest) if rest.pat().is_some() => {
            idents.extend(get_idents_from_pattern(rest.pat().unwrap()));
        }
        Pattern::AssignPattern(assign) if assign.key().is_some() => {
            idents.extend(get_idents_from_pattern(assign.key().unwrap()));
        }
        Pattern::ObjectPattern(obj) => {
            for elem in obj.elements() {
                let elem_idents = match elem {
                    ObjectPatternProp::AssignPattern(assign) => {
                        get_idents_from_pattern(assign.into())
                    }
                    ObjectPatternProp::KeyValuePattern(key_value) => {
                        if let Some(value) = key_value.value() {
                            get_idents_from_pattern(value)
                        } else {
                            continue;
                        }
                    }
                    ObjectPatternProp::RestPattern(rest) => get_idents_from_pattern(rest.into()),
                    ObjectPatternProp::SinglePattern(single) => {
                        get_idents_from_pattern(single.into())
                    }
                };
                idents.extend(elem_idents);
            }
        }
        _ => {}
    }

    idents
}

#[cfg(test)]
mod tests {
    use rslint_parser::{ast::VarDecl, parse_text};

    use super::*;

    macro_rules! test_unbound {
        ($input:expr, $expected:expr, $func:ident) => {
            let tree = parse_text($input, 0).syntax();
            let got = $func(&tree.first_child().unwrap())
                .into_iter()
                .map(|name_ref| name_ref.to_string())
                .collect::<Vec<_>>();
            assert_eq!($expected.as_slice(), got.as_slice()); //
        };
    }

    macro_rules! test_all {
        ($inputs_and_expecteds:expr, $func:ident) => {
            for (input, expected) in $inputs_and_expecteds {
                test_unbound!(input, expected, $func);
            }
        };
    }

    #[test]
    fn can_get_unbound_refs_from_arrow_expr() {
        let input = "(x) => { console.log(x); y = 3; }";
        let expected = ["console", "y"];

        test_unbound!(input, expected, get_unbound_refs);
    }

    #[test]
    fn can_get_unbound_refs_from_blocks() {
        let input =
            "{ console.log(x); console.log(y); if (3 === 3) { let z = 1; console.log(z); } }";
        let expected = ["console", "x", "console", "y", "console"];

        test_unbound!(input, expected, get_unbound_refs);
    }

    #[test]
    fn can_get_unbound_refs_from_expr() {
        test_all!(
            [
                ("console.log(y)", vec!["console", "y"]),
                ("y = 3;", vec!["y"]),
                ("(param) => console.log(param);", vec!["console"]),
                ("x", vec!["x"])
            ],
            get_unbound_refs
        );
    }

    #[test]
    fn can_get_idents_from_assignment_with_complex_patterns() {
        let input = "let { size, items: [item1, item2], stuff: [...other] } = big_object;";
        let expected = ["size", "item1", "item2", "other"];
        let tree = parse_text(input, 0).syntax();
        let pat = tree
            .first_child()
            .unwrap()
            .to::<VarDecl>()
            .declared()
            .next()
            .unwrap()
            .pattern()
            .unwrap();
        assert_eq!(expected, get_idents_from_pattern(pat).as_slice());
    }
}
