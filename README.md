# Git Testament

![BSD 3 Clause](https://img.shields.io/github/license/kinnison/git-testament.svg)
![Master branch build status](https://api.travis-ci.com/kinnison/git-testament.svg?branch=master)
![Latest docs](https://docs.rs/git-testament/badge.svg)
![Crates.IO](https://img.shields.io/crates/v/git-testament.svg)

`git-testament` is a library to embed a testament as to the state of a git
working tree during the build of a Rust program. It uses the power of procedural
macros to embed commit, tag, and working-tree-state information into your program
when it is built. This can then be used to report version information.

```rust
use git_testament::{git_testament, render_testament};

git_testament!(TESTAMENT);

fn main() {
    println!("My version information: {}", render_testament!(TESTAMENT));
}
```
