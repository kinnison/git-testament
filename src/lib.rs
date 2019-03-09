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
