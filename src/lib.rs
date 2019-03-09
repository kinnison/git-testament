use std::path::PathBuf;

enum GitModification {
    Added(PathBuf),
    Removed(PathBuf),
    Modified(PathBuf),
}

struct GitTestament {
    commit: Option<String>,
    describe: Option<(String, usize)>,
    modifications: Vec<GitModification>,
}
