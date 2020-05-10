//! # Generate a testament of the git working tree state for a build
//!
use std::fmt::{self, Display, Formatter};

pub use git_testament_derive::{git_testament, git_testament_macros};

/// A modification to a working tree, recorded when the testament was created.
#[derive(Debug)]
pub enum GitModification<'a> {
    /// A file or directory was added but not committed
    Added(&'a [u8]),
    /// A file or directory was removed but not committed
    Removed(&'a [u8]),
    /// A file was modified in some way, either content or permissions
    Modified(&'a [u8]),
    /// A file or directory was present but untracked
    Untracked(&'a [u8]),
}

/// The kind of commit available at the point that the testament was created.
#[derive(Debug)]
pub enum CommitKind<'a> {
    /// No repository was present.  Instead the crate's version and the
    /// build date are recorded.
    NoRepository(&'a str, &'a str),
    /// No commit was present, though it was a repository.  Instead the crate's
    /// version and the build date are recorded.
    NoCommit(&'a str, &'a str),
    /// There are no tags in the repository in the history of the commit.
    /// The commit hash and commit date are recorded.
    NoTags(&'a str, &'a str),
    /// There were tags in the history of the commit.
    /// The tag name, commit hash, commit date, and distance from the tag to
    /// the commit are recorded.
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
    pub branch_name: Option<&'a str>,
}

/// An empty testament.
///
/// This is used by the derive macro to fill in defaults
/// in the case that an older derive macro is used with a newer version
/// of git_testament.
///
/// Typically this will not be used directly by a user.
pub const EMPTY_TESTAMENT: GitTestament = GitTestament {
    commit: CommitKind::NoRepository("unknown", "unknown"),
    modifications: &[],
    branch_name: None,
};

impl<'a> GitTestament<'a> {
    #[doc(hidden)]
    pub fn _render_with_version(
        &self,
        pkg_version: &str,
        trusted_branch: Option<&'static str>,
    ) -> String {
        match self.commit {
            CommitKind::FromTag(tag, hash, date, _) => {
                let trusted = match trusted_branch {
                    Some(_) => {
                        if self.branch_name == trusted_branch {
                            self.modifications.is_empty()
                        } else {
                            false
                        }
                    }
                    None => false,
                };
                if trusted {
                    // We trust our branch, so construct an equivalent
                    // testament to render
                    format!(
                        "{}",
                        GitTestament {
                            commit: CommitKind::FromTag(pkg_version, hash, date, 0),
                            ..*self
                        }
                    )
                } else if tag.find(&pkg_version).is_some() {
                    format!("{}", self)
                } else {
                    format!("{} :: {}", pkg_version, self)
                }
            }
            _ => format!("{}", self),
        }
    }
}

/// Render a testament
///
/// This macro can be used to render a testament created with the `git_testament`
/// macro.  It renders a testament with the added benefit of indicating if the
/// tag does not match the version (by substring) then the crate's version and
/// the tag will be displayed in the form: "crate-ver :: testament..."
///
/// For situations where the crate version MUST override the tag, for example
/// if you have a release process where you do not make the tag unless the CI
/// constructing the release artifacts passes, then you can pass a second
/// argument to this macro stating a branch name to trust.  If the working
/// tree is clean and the branch name matches then the testament is rendered
/// as though the tag had been pushed at the built commit.  Since this overrides
/// a fundamental part of the behaviour of `git_testament` it is recommended that
/// this *ONLY* be used if you have a trusted CI release branch process.
///
/// ```
/// use git_testament::{git_testament, render_testament};
///
/// git_testament!(TESTAMENT);
///
/// # fn main() {
/// println!("The testament is: {}", render_testament!(TESTAMENT));
/// println!("The fiddled testament is: {}", render_testament!(TESTAMENT, "trusted-branch"));
/// # }
#[macro_export]
macro_rules! render_testament {
    ( $testament:expr ) => {
        $testament._render_with_version(env!("CARGO_PKG_VERSION"), None)
    };
    ( $testament:expr, $trusted_branch:expr ) => {
        $testament._render_with_version(env!("CARGO_PKG_VERSION"), Some($trusted_branch))
    };
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
