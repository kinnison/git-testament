#[cfg(feature = "alloc")]
use git_testament::{git_testament, render_testament};

#[cfg(feature = "alloc")]
git_testament!(TESTAMENT);

use git_testament::git_testament_macros;

git_testament_macros!(version, "trusted");

#[cfg(feature = "alloc")]
fn main() {
    assert_eq!(
        format!("{}", render_testament!(TESTAMENT, "trusted")),
        version_testament!()
    );
    println!("{}", render_testament!(TESTAMENT, "trusted"));
}

#[cfg(not(feature = "alloc"))]
fn main() {
    println!("{}", concat!("", version_testament!()));
}
