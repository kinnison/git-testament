//! # Generate a testament of the git working tree state for a build
//!
use std::fmt::{self, Display, Formatter};

pub use git_testament_derive::git_testament;

#[derive(Debug)]
pub enum GitModification<'a> {
    Added(&'a [u8]),
    Removed(&'a [u8]),
    Modified(&'a [u8]),
    Untracked(&'a [u8]),
}

#[derive(Debug)]
pub enum CommitKind<'a> {
    NoRepository,
    NoCommit,
    NoTags(&'a str, &'a str),
    FromTag(&'a str, &'a str, &'a str, usize),
}

/// A testament to the state of a git repository when a crate is built.
///
/// This is the type returned by the [`git_testament_derive::git_testament`]
/// macro when used to record the state of a git tree when a crate is built.
///
/// The structure contains information about the commit from which the crate
/// was built, along with information about any modifications to the working
/// tree which could be considered "dirty" as a result.
///
/// By default, the `Display` implementation for this structure attempts to
/// produce something pleasant but useful to humans.  For example it might
/// produce a string along the lines of `"1.0.0 (763aa159d 2019-04-02)"` for
/// a clean build from a 1.0.0 tag.  Alternatively if the working tree is dirty
/// and there have been some commits since the last tag, you might get something
/// more like `"1.0.0+14 (651af89ed 2019-04-02) dirty 4 modifications"`
///
/// If your program wishes to go into more detail, then the `commit` and the
/// `modifications` members are available for rendering as the program author
/// sees fit.
///
/// In general this is only of use for binaries, since libraries will generally
/// be built from `crates.io` provided tarballs and as such won't carry the
/// information needed.
#[derive(Debug)]
pub struct GitTestament<'a> {
    pub commit: CommitKind<'a>,
    pub modifications: &'a [GitModification<'a>],
}

impl<'a> Display for CommitKind<'a> {
    fn fmt(&self, fmt: &mut Formatter) -> fmt::Result {
        match self {
            CommitKind::NoRepository => fmt.write_str("not_in_git"),
            CommitKind::NoCommit => fmt.write_str("uncommitted"),
            CommitKind::NoTags(commit, when) => {
                fmt.write_fmt(format_args!("unknown ({} {})", &commit[..9], when))
            }
            CommitKind::FromTag(tag, commit, when, depth) => {
                if *depth > 0 {
                    fmt.write_fmt(format_args!(
                        "{}+{} ({} {})",
                        tag,
                        depth,
                        &commit[..9],
                        when
                    ))
                } else {
                    fmt.write_fmt(format_args!("{} ({} {})", tag, &commit[..9], when))
                }
            }
        }
    }
}

impl<'a> Display for GitTestament<'a> {
    fn fmt(&self, fmt: &mut Formatter) -> fmt::Result {
        self.commit.fmt(fmt)?;
        if !self.modifications.is_empty() {
            fmt.write_fmt(format_args!(
                " dirty {} modification{}",
                self.modifications.len(),
                if self.modifications.len() > 1 {
                    "s"
                } else {
                    ""
                }
            ))?;
        }
        Ok(())
    }
}
