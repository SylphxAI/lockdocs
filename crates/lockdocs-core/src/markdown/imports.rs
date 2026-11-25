pub(crate) fn import_base(import: &str) -> &str {
    let skip = usize::from(import.starts_with('@'));
    import.match_indices('/').nth(skip).map_or(import, |(i, _)| &import[..i])
}

/// Read module specifiers from actual JS/TS examples, not prose or MDX imports.
pub(super) fn collect_example_imports(body: &str, first_example: bool) -> Vec<String> {
    fn walk(n: tree_sitter::Node, src: &[u8], out: &mut Vec<String>) {
        let source = if matches!(n.kind(), "import_statement" | "export_statement") {
            n.child_by_field_name("source")
        } else if n.kind() == "call_expression"
            && n.child_by_field_name("function")
                .and_then(|f| f.utf8_text(src).ok())
                .is_some_and(|f| f == "require" || f == "import")
        {
            n.child_by_field_name("arguments")
                .and_then(|a| a.named_child(0))
                .filter(|s| s.kind() == "string")
        } else {
            None
        };
        if let Some(source) = source {
            if let Ok(s) = source.utf8_text(src) {
                out.push(s.trim_matches(['\'', '"']).to_string());
            }
        }
        for child in n.named_children(&mut n.walk()) {
            walk(child, src, out);
        }
    }
    let mut out = Vec::new();
    let mut fence: Option<String> = None;
    let mut code = String::new();
    let mut javascript = false;
    let mut parser = tree_sitter::Parser::new();
    let _ = parser.set_language(&tree_sitter_typescript::LANGUAGE_TSX.into());
    for line in body.lines() {
        let t = line.trim_start();
        if let Some(marker) = &fence {
            if t.starts_with(marker) {
                if javascript {
                    if let Some(tree) = parser.parse(&code, None) {
                        walk(tree.root_node(), code.as_bytes(), &mut out);
                    }
                    if first_example {
                        return out;
                    }
                }
                fence = None;
                code.clear();
            } else {
                code.push_str(line);
                code.push('\n');
            }
        } else if t.starts_with("```") || t.starts_with("~~~") {
            fence = Some(t[..3].to_string());
            let lang = t[3..].split_whitespace().next().unwrap_or("");
            javascript = ["", "js", "jsx", "ts", "tsx", "javascript", "typescript", "mjs", "cjs"].contains(&lang);
        }
    }
    if fence.is_some() && javascript {
        if let Some(tree) = parser.parse(&code, None) {
            walk(tree.root_node(), code.as_bytes(), &mut out);
        }
    }
    out
}

pub(super) fn example_imports(body: &str) -> Vec<String> {
    collect_example_imports(body, false)
}

/// A standalone declaration in the opening of a logical section. An
/// invalid or conflicting declaration clears inherited applicability.
pub(super) fn section_modes(text: &str) -> Option<Vec<String>> {
    let mut modes = None;
    for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let Some(rest) = line.strip_prefix("[MODES:") else { break };
        let Some(names) = rest.strip_suffix(']') else { return Some(Vec::new()) };
        let mut names: Vec<_> = names.split(',').map(str::trim).map(str::to_ascii_lowercase).collect();
        if names
            .iter()
            .any(|name| name.is_empty() || name.len() > 32 || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'))
        {
            return Some(Vec::new());
        }
        if names.len() > 8 {
            return Some(Vec::new());
        }
        names.sort_unstable();
        names.dedup();
        if modes.as_ref().is_some_and(|existing| existing != &names) {
            return Some(Vec::new());
        }
        modes = Some(names);
    }
    modes
}
