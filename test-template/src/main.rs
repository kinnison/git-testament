#[cfg(not(feature = "no-std"))]
use git_testament::{git_testament, render_testament};

#[cfg(not(feature = "no-std"))]
git_testament!(TESTAMENT);

use git_testament::git_testament_macros;

git_testament_macros!(version, "trusted");

#[cfg(not(feature = "no-std"))]
fn main() {
    assert_eq!(
        format!("{}", render_testament!(TESTAMENT, "trusted")),
        version_testament!()
    );
    println!("{}", render_testament!(TESTAMENT, "trusted"));
}

#[cfg(feature = "no-std")]
fn main() {
    println!("{}", concat!("", version_testament!()));
}
