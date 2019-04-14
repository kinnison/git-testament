//! Derive macro for `git_testament`
//!
extern crate proc_macro;

use std::env;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use proc_macro::TokenStream;
use quote::quote;
use syn::parse;
use syn::parse::{Parse, ParseStream};
use syn::{parse_macro_input, Ident};

use chrono::prelude::{DateTime, FixedOffset, NaiveDateTime, Utc};

use log::warn;

struct TestamentOptions {
    name: Ident,
}

impl Parse for TestamentOptions {
    fn parse(input: ParseStream) -> parse::Result<Self> {
        let name: Ident = input.parse()?;
        Ok(TestamentOptions { name })
    }
}

fn run_git<GD>(dir: GD, args: &[&str]) -> Result<Vec<u8>, Box<Error>>
where
    GD: AsRef<Path>,
{
    let output = Command::new("git")
        .args(args)
        .stdin(Stdio::null())
        .current_dir(dir)
        .output()?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err(String::from_utf8(output.stderr)?)?
    }
}

fn find_git_dir() -> Result<PathBuf, Box<Error>> {
    // run git rev-parse --show-toplevel in the MANIFEST DIR
    let dir = run_git(
        env::var("CARGO_MANIFEST_DIR").unwrap(),
        &["rev-parse", "--show-toplevel"],
    )?;
    // TODO: Find a way to go from the stdout to a pathbuf cleanly
    // without relying on utf8ness
    Ok(String::from_utf8(dir)?.trim_end().into())
}

fn revparse_single(git_dir: &Path, refname: &str) -> Result<(String, i64, i32), Box<Error>> {
    // TODO: Again, try and remove UTF8 assumptions somehow
    let sha = String::from_utf8(run_git(git_dir, &["rev-parse", refname])?)?
        .trim_end()
        .to_owned();
    let show = String::from_utf8(run_git(git_dir, &["cat-file", "-p", &sha])?)?;

    for line in show.lines() {
        if line.starts_with("committer ") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 2 {
                Err(format!("Insufficient committer data in {}", line))?
            }
            let time: i64 = parts[parts.len() - 2].parse()?;
            let offset: &str = parts[parts.len() - 1];
            if offset.len() != 5 {
                Err(format!(
                    "Insufficient/Incorrect data in timezone offset: {}",
                    offset
                ))?
            }
            let offset: i32 = if offset.starts_with('-') {
                // Negative...
                let hours: i32 = offset[1..=2].parse()?;
                let mins: i32 = offset[3..=4].parse()?;
                -(mins + (hours * 60))
            } else {
                // Positive...
                let hours: i32 = offset[1..=2].parse()?;
                let mins: i32 = offset[3..=4].parse()?;
                (mins + (hours * 60))
            };
            return Ok((sha, time, offset));
        } else if line.is_empty() {
            // Ran out of input, without finding committer
            Err(format!(
                "Unable to find committer information in {}",
                refname
            ))?
        }
    }

    Err(format!("Somehow fell off the end of the commit data"))?
}

fn branch_name(dir: &Path) -> Result<Option<String>, Box<Error>> {
    let symref = match run_git(dir, &["symbolic-ref", "-q", "HEAD"]) {
        Ok(s) => s,
        Err(_) => run_git(dir, &["name-rev", "--name-only", "HEAD"])?,
    };
    let mut name = String::from_utf8(symref)?.trim().to_owned();
    if name.starts_with("refs/heads/") {
        name = name[11..].to_owned();
    }
    if name.is_empty() {
        Ok(None)
    } else {
        Ok(Some(name))
    }
}

fn describe(dir: &Path, sha: &str) -> Result<String, Box<Error>> {
    // TODO: Work out a way to not use UTF8?
    Ok(
        String::from_utf8(run_git(dir, &["describe", "--tags", "--long", sha])?)?
            .trim_end()
            .to_owned(),
    )
}

enum StatusFlag {
    Added,
    Deleted,
    Modified,
    Untracked,
}
use StatusFlag::*;

struct StatusEntry {
    path: String,
    status: StatusFlag,
}

fn status(dir: &Path) -> Result<Vec<StatusEntry>, Box<Error>> {
    // TODO: Work out a way to not use UTF8?
    let info = String::from_utf8(run_git(
        dir,
        &[
            "status",
            "--porcelain",
            "--untracked-files=all",
            "--ignore-submodules=all",
        ],
    )?)?;

    let mut ret = Vec::new();

    for line in info.lines() {
        let index_change = line.chars().next().unwrap();
        let worktree_change = line.chars().skip(1).next().unwrap();
        match (index_change, worktree_change) {
            ('?', _) | (_, '?') => ret.push(StatusEntry {
                path: line[3..].to_owned(),
                status: Untracked,
            }),
            ('A', _) | (_, 'A') => ret.push(StatusEntry {
                path: line[3..].to_owned(),
                status: Added,
            }),
            ('M', _) | (_, 'M') => ret.push(StatusEntry {
                path: line[3..].to_owned(),
                status: Modified,
            }),
            ('D', _) | (_, 'D') => ret.push(StatusEntry {
                path: line[3..].to_owned(),
                status: Deleted,
            }),
            _ => {}
        }
    }

    Ok(ret)
}

/// Generate a testament for the working tree.
///
/// This macro declares a static data structure which represents a testament
/// to the state of a git repository at the point that a crate was built.
///
/// The intention is that the macro should be used at the top level of a binary
/// crate to provide information about the state of the codebase that the output
/// program was built from.  This includes a number of things such as the commit
/// SHA, any related tag, how many commits since the tag, the date of the commit,
/// and if there are any "dirty" parts to the working tree such as modified files,
/// uncommitted files, etc.
///
/// ```
/// // Bring the procedural macro into scope
/// use git_testament::git_testament;
///
/// // Declare a testament, it'll end up as a static, so give it a capital
/// // letters name or it'll result in a warning.
/// git_testament!(TESTAMENT);
/// # fn main() {
///
/// // ... later, you can display the testament.
/// println!("app version {}", TESTAMENT);
/// # }
/// ```
///
/// See [`git_testament::GitTestament`] for the type of the defined `TESTAMENT`.
#[proc_macro]
pub fn git_testament(input: TokenStream) -> TokenStream {
    let TestamentOptions { name } = parse_macro_input!(input as TestamentOptions);

    let pkgver = env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "?.?.?".to_owned());
    let now = Utc::now();
    let now = format!("{}", now.format("%Y-%m-%d"));

    let git_dir = match find_git_dir() {
        Ok(dir) => dir,
        Err(e) => {
            warn!(
                "Unable to open a repo at {}: {}",
                env::var("CARGO_MANIFEST_DIR").unwrap(),
                e
            );
            return (quote! {
                static #name: git_testament::GitTestament<'static> = git_testament::GitTestament {
                    commit: git_testament::CommitKind::NoRepository(#pkgver, #now),
                    .. git_testament::EMPTY_TESTAMENT
                };
            })
            .into();
        }
    };

    // Second simple preliminary step: attempt to get a branch name to report
    let branch_name = match branch_name(&git_dir) {
        Ok(Some(name)) => quote! {Some(#name)},
        Ok(None) => quote! {None},
        Err(e) => {
            warn!("Unable to determine branch name: {}", e);
            quote! {None}
        }
    };

    // Step one, determine the current commit ID and the date of that commit
    let (commit_id, commit_date) = {
        let (commit, commit_time, commit_offset) = match revparse_single(&git_dir, "HEAD") {
            Ok(commit_data) => commit_data,
            Err(e) => {
                warn!("No commit at HEAD: {}", e);
                return (quote! {
                static #name: git_testament::GitTestament<'static> = git_testament::GitTestament {
                    commit: git_testament::CommitKind::NoCommit(#pkgver, #now),
                    branch_name: #branch_name,
                    .. git_testament::EMPTY_TESTAMENT
                };
            })
            .into();
            }
        };

        // Acquire the commit info

        let commit_id = format!("{}", commit);
        let naive = NaiveDateTime::from_timestamp(commit_time, 0);
        let offset = FixedOffset::east(commit_offset * 60);
        let commit_time = DateTime::<FixedOffset>::from_utc(naive, offset);
        let commit_date = format!("{}", commit_time.format("%Y-%m-%d"));

        (commit_id, commit_date)
    };

    // Next determine if there was a tag, and if so, what our relationship
    // to that tag is...

    let (tag, steps) = match describe(&git_dir, &commit_id) {
        Ok(res) => {
            let res = &res[..res.rfind('-').expect("No commit info in describe!")];
            let tag_name = &res[..res.rfind('-').expect("No commit count in describe!")];
            let commit_count = res[tag_name.len() + 1..]
                .parse::<usize>()
                .expect("Unable to parse commit count in describe!");

            (tag_name.to_owned(), commit_count)
        }
        Err(_) => {
            warn!("No tag info found!");
            ("".to_owned(), 0)
        }
    };

    let commit = if tag.len() > 0 {
        // We've a tag
        quote! {
            git_testament::CommitKind::FromTag(#tag, #commit_id, #commit_date, #steps)
        }
    } else {
        quote! {
            git_testament::CommitKind::NoTags(#commit_id, #commit_date)
        }
    };

    // Finally, we need to gather the modifications to the tree...
    let statuses: Vec<_> = status(&git_dir)
        .expect("Unable to generate status information for working tree!")
        .into_iter()
        .map(|status| {
            let path = status.path.into_bytes();
            match status.status {
                Untracked => quote! {
                    git_testament::GitModification::Untracked(&[#(#path),*])
                },
                Added => quote! {
                    git_testament::GitModification::Added(&[#(#path),*])
                },
                Modified => quote! {
                    git_testament::GitModification::Modified(&[#(#path),*])
                },
                Deleted => quote! {
                    git_testament::GitModification::Removed(&[#(#path),*])
                },
            }
        })
        .collect();

    (quote! {
        static #name: git_testament::GitTestament<'static> = git_testament::GitTestament {
            commit: #commit,
            modifications: &[#(#statuses),*],
            branch_name: #branch_name,
            .. git_testament::EMPTY_TESTAMENT
        };
    })
    .into()
}
