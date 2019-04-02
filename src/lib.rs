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
    NoRepository(&'a str, &'a str),
    NoCommit(&'a str, &'a str),
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
/// information needed.  In such a fallback position the string will be something
/// along the lines of `"x.y (somedate)"` where `x.y` is the crate's version and
/// `somedate` is the date of the build.  You'll get similar information if the
/// crate is built in a git repository on a branch with no commits yet (e.g.
/// when you first have run `cargo init`) though that will include the string
/// `uncommitted` to indicate that once commits are made the information will be
/// of more use.
#[derive(Debug)]
pub struct GitTestament<'a> {
    pub commit: CommitKind<'a>,
    pub modifications: &'a [GitModification<'a>],
}

impl<'a> Display for CommitKind<'a> {
    fn fmt(&self, fmt: &mut Formatter) -> fmt::Result {
        match self {
            CommitKind::NoRepository(crate_ver, build_date) => {
                fmt.write_fmt(format_args!("{} ({})", crate_ver, build_date))
            }
            CommitKind::NoCommit(crate_ver, build_date) => {
                fmt.write_fmt(format_args!("{} (uncommitted {})", crate_ver, build_date))
            }
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
