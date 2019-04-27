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

## Reproducible builds

In the case that your build is not being done from a Git repository, you still
want your testament to be useful to your users.  Reproducibility of the binary
is critical in that case.  The [Reproducible Builds][reprobuild] team have defined
a mechanism for this known as [`SOURCE_DATE_EPOCH`][sde] which is an environment
variable which can be set to ensure the build date is fixed for reproducibilty
reasons.  If you have no repo (or a repo but no commit) then `git_testament!()`
will use the [`SOURCE_DATE_EPOCH`][sde] environment variable (if present and parseable
as a number of seconds since the UNIX epoch) to override `now`.

[reprobuild]: https://reproducible-builds.org
[sde]: https://reproducible-builds.org/docs/source-date-epoch/
