use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct MoveOptions {
    pub recursive: bool,
    pub interactive: bool,
    pub force: bool,
    pub dry_run: bool,
    pub verbose: bool,
    pub extensions: Vec<String>,
    pub tsconfig: Option<PathBuf>,
    pub absolute_imports: bool,
    pub alias_prefix: String,
}

impl Default for MoveOptions {
    fn default() -> Self {
        Self {
            recursive: false,
            interactive: false,
            force: false,
            dry_run: false,
            verbose: false,
            extensions: vec![".ts".into(), ".tsx".into()],
            tsconfig: None,
            absolute_imports: true,
            alias_prefix: "@".into(),
        }
    }
}

impl MoveOptions {
    pub fn parse_extensions(ext_str: &str) -> Vec<String> {
        ext_str
            .split(',')
            .map(|s| s.trim())
            .map(|s| if s.starts_with('.') { s.to_string() } else { format!(".{s}") })
            .collect()
    }
}
