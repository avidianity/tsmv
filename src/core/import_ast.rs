//! AST-based scanning and rewriting of ES module specifiers.
//!
//! This is the single source of truth for "what counts as a module reference".
//! It parses each file with tree-sitter's TypeScript/TSX grammar and matches the
//! *real* syntax nodes rather than text patterns, so it handles every static
//! string specifier form:
//!
//! * static `import …`/`export … from '…'` (incl. `import type`, multi-line)
//! * side-effect `import '…'`
//! * dynamic `import('…')`
//! * CommonJS `require('…')`
//! * mock/loader calls `jest.mock('…')` / `vi.mock('…')` (and siblings)
//!
//! Because matching is syntactic, specifiers that merely *look* like imports
//! inside comments or unrelated string literals are never touched. Specifiers
//! whose value is computed at runtime (a variable, or an interpolated template
//! literal) are intentionally left alone — they cannot be resolved statically by
//! any tool.

use std::sync::OnceLock;

use tree_sitter::{Node, Parser};

/// The TSX grammar parses both `.ts` and `.tsx` sources; tree-sitter recovers
/// locally from the rare constructs the two grammars disagree on, so import
/// detection stays correct either way.
fn language() -> &'static tree_sitter::Language {
    static LANG: OnceLock<tree_sitter::Language> = OnceLock::new();
    LANG.get_or_init(|| tree_sitter_typescript::LANGUAGE_TSX.into())
}

/// What kind of reference a specifier came from. Callers use this to decide
/// whether a rewrite is appropriate — e.g. a `new URL(..., import.meta.url)` path
/// is module-relative and must stay relative, so it is never aliased.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SpecKind {
    /// `import`/`export … from`, side-effect `import`, dynamic `import()`,
    /// `require()`, and `jest`/`vi` mock calls.
    Import,
    /// `new URL('…', import.meta.url)`.
    Url,
    /// `require.context('…', …)`.
    Context,
}

/// A module-specifier occurrence: the byte range *inside* the quotes, the
/// specifier text itself, and the kind of reference it came from.
struct SpecSpan {
    start: usize,
    end: usize,
    text: String,
    kind: SpecKind,
}

/// Parse `content` and collect every static-string module specifier in source
/// order.
fn collect_specs(content: &str) -> Vec<SpecSpan> {
    let mut parser = Parser::new();
    if parser.set_language(language()).is_err() {
        return Vec::new();
    }
    let tree = match parser.parse(content, None) {
        Some(t) => t,
        None => return Vec::new(),
    };

    let src = content.as_bytes();
    let mut out = Vec::new();
    let mut stack = vec![tree.root_node()];
    while let Some(node) = stack.pop() {
        if let Some((string_node, kind)) = specifier_string(node, src) {
            if let Some(span) = inner_span(string_node, content, kind) {
                out.push(span);
            }
        }
        let mut i = node.child_count();
        while i > 0 {
            i -= 1;
            if let Some(child) = node.child(i) {
                stack.push(child);
            }
        }
    }

    out.sort_by_key(|s| s.start);
    out
}

/// First direct `string` child of an `arguments` node.
fn first_string_arg<'a>(args: Node<'a>) -> Option<Node<'a>> {
    let mut i = 0;
    while i < args.child_count() {
        let child = args.child(i)?;
        if child.kind() == "string" {
            return Some(child);
        }
        i += 1;
    }
    None
}

/// If `node` carries a module specifier, return its `string` node and kind.
fn specifier_string<'a>(node: Node<'a>, src: &[u8]) -> Option<(Node<'a>, SpecKind)> {
    match node.kind() {
        // import … from '…' | export … from '…' | import '…'
        "import_statement" | "export_statement" => {
            let source = node.child_by_field_name("source")?;
            (source.kind() == "string").then_some((source, SpecKind::Import))
        }
        // import('…') | require('…') | jest.mock('…') | vi.mock('…') | require.context('…', …)
        "call_expression" => {
            let func = node.child_by_field_name("function")?;
            let args = node.child_by_field_name("arguments")?;
            let mut kind = SpecKind::Import;
            let is_module_call = match func.kind() {
                "import" => true,
                "identifier" => node_text(func, src) == "require",
                "member_expression" => {
                    let object = func.child_by_field_name("object")?;
                    match node_text(object, src) {
                        "jest" | "vi" => true,
                        "require" => {
                            let is_context = func
                                .child_by_field_name("property")
                                .map(|p| node_text(p, src) == "context")
                                .unwrap_or(false);
                            if is_context {
                                kind = SpecKind::Context;
                            }
                            is_context
                        }
                        _ => false,
                    }
                }
                _ => false,
            };
            if !is_module_call {
                return None;
            }
            Some((first_string_arg(args)?, kind))
        }
        // new URL('…', import.meta.url) — module-relative asset/worker URLs.
        "new_expression" => {
            let ctor = node.child_by_field_name("constructor")?;
            if node_text(ctor, src) != "URL" {
                return None;
            }
            let args = node.child_by_field_name("arguments")?;
            // Only when based on import.meta.url, so plain `new URL('http://…')`
            // and runtime bases are left alone.
            let mut has_meta = false;
            let mut i = 0;
            while i < args.child_count() {
                let child = args.child(i)?;
                if child.kind() != "string" && node_text(child, src) == "import.meta.url" {
                    has_meta = true;
                }
                i += 1;
            }
            if !has_meta {
                return None;
            }
            Some((first_string_arg(args)?, SpecKind::Url))
        }
        _ => None,
    }
}

/// Split a webpack-style inline-loader prefix off a specifier:
/// `"raw-loader!./a"` → `("raw-loader!", "./a")`; no `!` → `("", spec)`.
fn split_loader_prefix(spec: &str) -> (&str, &str) {
    match spec.rfind('!') {
        Some(i) => spec.split_at(i + 1),
        None => ("", spec),
    }
}

fn node_text<'a>(node: Node, src: &'a [u8]) -> &'a str {
    node.utf8_text(src).unwrap_or("")
}

/// Byte range and text *between* the quotes of a `string` node.
fn inner_span(string_node: Node, content: &str, kind: SpecKind) -> Option<SpecSpan> {
    let start = string_node.start_byte();
    let end = string_node.end_byte();
    // A string node always includes its two single-byte quote delimiters.
    if end < start + 2 {
        return None;
    }
    let (start, end) = (start + 1, end - 1);
    let text = content.get(start..end)?.to_string();
    Some(SpecSpan {
        start,
        end,
        text,
        kind,
    })
}

/// Rewrite every module specifier in `content` using `f`.
///
/// `f` receives the specifier path (without quotes, and without any inline-loader
/// prefix) and its [`SpecKind`], and returns `Some(replacement)` to rewrite it or
/// `None` to leave it untouched. Any loader prefix is preserved automatically.
/// Returns the new content and whether any change was made. Only the text inside
/// the quotes is replaced, so quote style and surrounding formatting are kept.
pub fn rewrite_imports<F>(content: &str, mut f: F) -> (String, bool)
where
    F: FnMut(&str, SpecKind) -> Option<String>,
{
    let specs = collect_specs(content);

    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    for spec in &specs {
        let (prefix, path) = split_loader_prefix(&spec.text);
        if let Some(new_path) = f(path, spec.kind) {
            if new_path != path {
                edits.push((spec.start, spec.end, format!("{prefix}{new_path}")));
            }
        }
    }

    if edits.is_empty() {
        return (content.to_string(), false);
    }

    // Apply from the end so earlier byte offsets stay valid.
    edits.sort_by_key(|e| std::cmp::Reverse(e.0));
    let mut result = content.to_string();
    for (start, end, new_spec) in edits {
        result.replace_range(start..end, &new_spec);
    }

    (result, true)
}

/// Collect every module specifier path (without quotes or loader prefix).
pub fn collect_import_specifiers(content: &str) -> Vec<String> {
    collect_specs(content)
        .into_iter()
        .map(|s| split_loader_prefix(&s.text).1.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repoint<'a>(old: &'a str, new: &'a str) -> impl Fn(&str, SpecKind) -> Option<String> + 'a {
        move |s: &str, _k: SpecKind| (s == old).then(|| new.to_string())
    }

    #[test]
    fn rewrites_single_line_import() {
        let (out, changed) =
            rewrite_imports("import { x } from './old';\n", repoint("./old", "./new"));
        assert!(changed);
        assert_eq!(out, "import { x } from './new';\n");
    }

    #[test]
    fn rewrites_multiline_named_import() {
        let src = "import {\n  a,\n  b,\n} from './old';\n";
        let (out, changed) = rewrite_imports(src, repoint("./old", "./new"));
        assert!(changed, "multi-line import should be rewritten");
        assert!(out.contains("from './new'"), "got:\n{out}");
    }

    #[test]
    fn rewrites_export_and_side_effect() {
        let src = "export { a } from './old';\nimport './old';\n";
        let (out, changed) = rewrite_imports(src, repoint("./old", "./new"));
        assert!(changed);
        assert!(out.contains("export { a } from './new'"), "got:\n{out}");
        assert!(out.contains("import './new'"), "got:\n{out}");
    }

    #[test]
    fn rewrites_dynamic_import() {
        let src = "const m = await import('./old');\n";
        let (out, changed) = rewrite_imports(src, repoint("./old", "./new"));
        assert!(changed, "dynamic import should be rewritten");
        assert_eq!(out, "const m = await import('./new');\n");
    }

    #[test]
    fn rewrites_require() {
        let src = "const r = require('./old');\n";
        let (out, changed) = rewrite_imports(src, repoint("./old", "./new"));
        assert!(changed, "require() should be rewritten");
        assert_eq!(out, "const r = require('./new');\n");
    }

    #[test]
    fn rewrites_jest_and_vi_mock() {
        let src = "jest.mock('./old');\nvi.mock('./old');\n";
        let (out, changed) = rewrite_imports(src, repoint("./old", "./new"));
        assert!(changed);
        assert_eq!(out, "jest.mock('./new');\nvi.mock('./new');\n");
    }

    #[test]
    fn ignores_specifiers_in_comments_and_strings() {
        // Only the real import must change — the look-alikes in a comment and in
        // a string literal must be left untouched. A regex cannot do this.
        let src = "// import { x } from './old';\n\
                   const s = \"import y from './old'\";\n\
                   import { z } from './old';\n";
        let (out, changed) = rewrite_imports(src, repoint("./old", "./new"));
        assert!(changed);
        assert!(out.contains("// import { x } from './old';"), "comment changed:\n{out}");
        assert!(out.contains("\"import y from './old'\""), "string changed:\n{out}");
        assert!(out.contains("import { z } from './new';"), "real import not changed:\n{out}");
    }

    #[test]
    fn leaves_bare_specifiers_alone() {
        let src = "import { x } from 'react';\n";
        let (out, changed) = rewrite_imports(src, |s, _k| {
            s.starts_with('.').then(|| "CHANGED".to_string())
        });
        assert!(!changed);
        assert_eq!(out, src);
    }

    #[test]
    fn rewrites_new_url_with_import_meta() {
        let src = "const w = new Worker(new URL('./old', import.meta.url));\n";
        let (out, changed) = rewrite_imports(src, repoint("./old", "./new"));
        assert!(changed, "new URL(import.meta.url) should be rewritten");
        assert!(out.contains("new URL('./new', import.meta.url)"), "got:\n{out}");
    }

    #[test]
    fn leaves_plain_new_url_alone() {
        // No import.meta.url base -> not a module-relative URL.
        let src = "const u = new URL('./old', base);\n";
        let (out, changed) = rewrite_imports(src, repoint("./old", "./new"));
        assert!(!changed, "new URL without import.meta.url must be untouched: {out}");
    }

    #[test]
    fn rewrites_require_context_dir() {
        let src = "const ctx = require.context('./old', true, /\\.js$/);\n";
        let (out, changed) = rewrite_imports(src, repoint("./old", "./new"));
        assert!(changed, "require.context dir should be rewritten");
        assert!(out.contains("require.context('./new'"), "got:\n{out}");
    }

    #[test]
    fn preserves_loader_prefix() {
        let src = "import css from '!!style-loader!css-loader!./old.css';\n";
        let (out, changed) = rewrite_imports(src, repoint("./old.css", "./new.css"));
        assert!(changed);
        assert_eq!(out, "import css from '!!style-loader!css-loader!./new.css';\n");
    }

    #[test]
    fn url_and_context_report_their_kind() {
        // The absolute-import pass relies on kind to skip URL/context specifiers.
        let src = "new URL('./a', import.meta.url);\nrequire.context('./b', true, /x/);\nimport './c';\n";
        let mut seen: Vec<(String, SpecKind)> = Vec::new();
        rewrite_imports(src, |s, k| {
            seen.push((s.to_string(), k));
            None
        });
        assert!(seen.contains(&("./a".to_string(), SpecKind::Url)));
        assert!(seen.contains(&("./b".to_string(), SpecKind::Context)));
        assert!(seen.contains(&("./c".to_string(), SpecKind::Import)));
    }

    #[test]
    fn collects_all_specifier_forms() {
        let src = "import a from './a';\n\
                   export * from './b';\n\
                   import './c';\n\
                   const d = import('./d');\n\
                   const e = require('./e');\n\
                   jest.mock('./f');\n\
                   const u = new URL('./g', import.meta.url);\n\
                   const c = require.context('./h', true, /x/);\n\
                   import s from 'raw-loader!./i';\n";
        let specs = collect_import_specifiers(src);
        for want in ["./a", "./b", "./c", "./d", "./e", "./f", "./g", "./h", "./i"] {
            assert!(specs.contains(&want.to_string()), "missing {want} in {specs:?}");
        }
    }
}
