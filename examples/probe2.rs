use tree_sitter::{Parser, TreeCursor};

fn dump(c: &mut TreeCursor, src: &[u8], depth: usize) {
    let n = c.node();
    let f = c.field_name().map(|x| format!(" field={x}")).unwrap_or_default();
    let t = if n.child_count() == 0 { format!(" {:?}", n.utf8_text(src).unwrap_or("")) } else { String::new() };
    println!("{}{}{}{}", "  ".repeat(depth), n.kind(), f, t);
    if c.goto_first_child() { loop { dump(c, src, depth+1); if !c.goto_next_sibling() { break; } } c.goto_parent(); }
}
fn main() {
    let lang: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TSX.into();
    let mut p = Parser::new(); p.set_language(&lang).unwrap();
    for s in [
        "const w = new URL('./worker.js', import.meta.url);",
        "require.context('./dir', true, /\\.js$/);",
        "import x from 'raw-loader!./a';",
    ] {
        println!("\n===== {s}");
        let t = p.parse(s, None).unwrap();
        let mut c = t.walk(); dump(&mut c, s.as_bytes(), 0);
    }
}
