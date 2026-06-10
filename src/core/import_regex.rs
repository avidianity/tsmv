//! Shared regular expressions for matching ES module import/export specifiers.
//!
//! All import-rewriting and import-scanning logic in the crate goes through this
//! module so there is a single source of truth for what counts as an import.
//! The patterns intentionally tolerate multi-line statements (e.g. named imports
//! split across several lines) by allowing whitespace inside the import clause.

use std::sync::OnceLock;

use regex::Regex;

/// Compiled `(regex, capture_group_of_specifier)` pairs, computed once.
fn import_patterns() -> &'static [(Regex, usize)] {
    static PATTERNS: OnceLock<Vec<(Regex, usize)>> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        let raw: &[(&str, usize)] = &[
            // import x from '...' | import { a, b } from '...' (multi-line ok)
            // import * as x from '...' | import type { x } from '...'
            (
                r#"(import\s+(?:type\s+)?(?:[\w\s{},*]|(?:[^{}]*\{[^{}]*\}))+?\s+from\s+)['"]([^'"]+)['"]"#,
                2,
            ),
            // export { a, b } from '...' | export * from '...' | export * as ns from '...'
            // export type { x } from '...'
            (
                r#"(export\s+(?:type\s+)?(?:\{[^{}]*\}|\*(?:\s+as\s+[\w$]+)?\s+)\s*from\s+)['"]([^'"]+)['"]"#,
                2,
            ),
            // import '...' (side-effect import)
            (r#"(import\s+)['"]([^'"]+)['"]"#, 2),
        ];
        raw.iter()
            .map(|(p, g)| (Regex::new(p).expect("valid import regex"), *g))
            .collect()
    })
}

/// Rewrite every module specifier in `content` using `f`.
///
/// `f` receives each specifier (without quotes) and returns `Some(replacement)`
/// to rewrite it, or `None` to leave it untouched. Returns the new content and
/// whether any change was made.
pub fn rewrite_imports<F>(content: &str, mut f: F) -> (String, bool)
where
    F: FnMut(&str) -> Option<String>,
{
    let mut changed = false;
    let mut current = content.to_string();

    for (re, group) in import_patterns() {
        current = re
            .replace_all(&current, |caps: &regex::Captures| {
                let spec = caps[*group].to_string();
                if let Some(new_spec) = f(&spec) {
                    if new_spec != spec {
                        changed = true;
                        return caps[0].replace(&spec, &new_spec);
                    }
                }
                caps[0].to_string()
            })
            .into_owned();
    }

    (current, changed)
}

/// Collect every module specifier (without quotes) found in `content`.
pub fn collect_import_specifiers(content: &str) -> Vec<String> {
    let mut specs = Vec::new();
    for (re, group) in import_patterns() {
        for caps in re.captures_iter(content) {
            specs.push(caps[*group].to_string());
        }
    }
    specs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrites_single_line_import() {
        let (out, changed) = rewrite_imports("import { x } from './old';\n", |s| {
            (s == "./old").then(|| "./new".to_string())
        });
        assert!(changed);
        assert_eq!(out, "import { x } from './new';\n");
    }

    #[test]
    fn rewrites_multiline_named_import() {
        let src = "import {\n  a,\n  b,\n} from './old';\n";
        let (out, changed) = rewrite_imports(src, |s| {
            (s == "./old").then(|| "./new".to_string())
        });
        assert!(changed, "multi-line import should be rewritten");
        assert!(out.contains("from './new'"), "got:\n{out}");
    }

    #[test]
    fn rewrites_export_and_side_effect() {
        let src = "export { a } from './old';\nimport './old';\n";
        let (out, changed) = rewrite_imports(src, |s| {
            (s == "./old").then(|| "./new".to_string())
        });
        assert!(changed);
        assert!(out.contains("export { a } from './new'"), "got:\n{out}");
        assert!(out.contains("import './new'"), "got:\n{out}");
    }

    #[test]
    fn leaves_bare_specifiers_alone() {
        let src = "import { x } from 'react';\n";
        let (out, changed) = rewrite_imports(src, |s| {
            s.starts_with('.').then(|| "CHANGED".to_string())
        });
        assert!(!changed);
        assert_eq!(out, src);
    }

    #[test]
    fn collects_all_specifiers() {
        let src = "import a from './a';\nexport * from './b';\nimport './c';\n";
        let specs = collect_import_specifiers(src);
        assert!(specs.contains(&"./a".to_string()));
        assert!(specs.contains(&"./b".to_string()));
        assert!(specs.contains(&"./c".to_string()));
    }
}
