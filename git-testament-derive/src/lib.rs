//! Derive macro for `git_testament`
//!
extern crate proc_macro;

use std::env;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use syn::parse;
use syn::parse::{Parse, ParseStream};
use syn::token::Comma;
use syn::{parse_macro_input, Ident, LitStr, Token};

use chrono::prelude::{DateTime, FixedOffset, NaiveDateTime, TimeZone, Utc};

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

struct StaticTestamentOptions {
    name: Ident,
    trusted: Option<String>,
}

impl Parse for StaticTestamentOptions {
    fn parse(input: ParseStream) -> parse::Result<Self> {
        let name: Ident = input.parse()?;
        let trusted = if input.peek(Token![,]) {
            input.parse::<Comma>()?;
            let t: LitStr = input.parse()?;
            Some(t.value())
        } else {
            None
        };
        Ok(StaticTestamentOptions { name, trusted })
    }
}

fn run_git<GD>(dir: GD, args: &[&str]) -> Result<Vec<u8>, Box<dyn Error>>
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
        Err(String::from_utf8(output.stderr)?.into())
    }
}

fn find_git_dir() -> Result<PathBuf, Box<dyn Error>> {
    // run git rev-parse --show-toplevel in the MANIFEST DIR
    let dir = run_git(
        env::var("CARGO_MANIFEST_DIR").unwrap(),
        &["rev-parse", "--show-toplevel"],
    )?;
    // TODO: Find a way to go from the stdout to a pathbuf cleanly
    // without relying on utf8ness
    Ok(String::from_utf8(dir)?.trim_end().into())
}

fn revparse_single(git_dir: &Path, refname: &str) -> Result<(String, i64, i32), Box<dyn Error>> {
    // TODO: Again, try and remove UTF8 assumptions somehow
    let sha = String::from_utf8(run_git(git_dir, &["rev-parse", refname])?)?
        .trim_end()
        .to_owned();
    let show = String::from_utf8(run_git(git_dir, &["cat-file", "-p", &sha])?)?;

    for line in show.lines() {
        if line.starts_with("committer ") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 2 {
                return Err(format!("Insufficient committer data in {}", line).into());
            }
            let time: i64 = parts[parts.len() - 2].parse()?;
            let offset: &str = parts[parts.len() - 1];
            if offset.len() != 5 {
                return Err(
                    format!("Insufficient/Incorrect data in timezone offset: {}", offset).into(),
                );
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
                mins + (hours * 60)
            };
            return Ok((sha, time, offset));
        } else if line.is_empty() {
            // Ran out of input, without finding committer
            return Err(format!("Unable to find committer information in {}", refname).into());
        }
    }

    Err("Somehow fell off the end of the commit data".into())
}

fn branch_name(dir: &Path) -> Result<Option<String>, Box<dyn Error>> {
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

fn describe(dir: &Path, sha: &str) -> Result<String, Box<dyn Error>> {
    // TODO: Work out a way to not use UTF8?
    Ok(
        String::from_utf8(run_git(dir, &["describe", "--tags", "--long", sha])?)?
            .trim_end()
            .to_owned(),
    )
}

#[derive(Clone, Copy)]
enum StatusFlag {
    Added,
    Deleted,
    Modified,
    Untracked,
}
use StatusFlag::*;

#[derive(Clone)]
struct StatusEntry {
    path: String,
    status: StatusFlag,
}

fn status(dir: &Path) -> Result<Vec<StatusEntry>, Box<dyn Error>> {
    // TODO: Work out a way to not use UTF8?
    let info = String::from_utf8(run_git(
        dir,
        &[
            "status",
            "--porcelain",
            "--untracked-files=normal",
            "--ignore-submodules=all",
        ],
    )?)?;

    let mut ret = Vec::new();

    for line in info.lines() {
        let index_change = line.chars().next().unwrap();
        let worktree_change = line.chars().nth(1).unwrap();
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

struct InvocationInformation {
    pkgver: String,
    now: String,
}

impl InvocationInformation {
    fn acquire() -> Self {
        let pkgver = env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "?.?.?".to_owned());
        let now = Utc::now();
        let now = format!("{}", now.format("%Y-%m-%d"));
        let sde = match env::var("SOURCE_DATE_EPOCH") {
            Ok(sde) => match sde.parse::<i64>() {
                Ok(sde) => Some(format!("{}", Utc.timestamp(sde, 0).format("%Y-%m-%d"))),
                Err(_) => None,
            },
            Err(_) => None,
        };
        let now = sde.unwrap_or(now);

        Self { pkgver, now }
    }
}

#[derive(Clone)]
struct CommitInfo {
    id: String,
    date: String,
    tag: String,
    distance: usize,
}

#[derive(Clone)]
struct GitInformation {
    branch: Option<String>,
    commitinfo: Option<CommitInfo>,
    status: Vec<StatusEntry>,
}

impl GitInformation {
    fn acquire() -> Result<Self, Box<dyn std::error::Error>> {
        let git_dir = find_git_dir()?;
        let branch = match branch_name(&git_dir) {
            Ok(b) => b,
            Err(e) => {
                warn!("Unable to determine branch name: {}", e);
                None
            }
        };

        let commitinfo = (|| {
            let (commit, commit_time, commit_offset) = match revparse_single(&git_dir, "HEAD") {
                Ok(commit_data) => commit_data,
                Err(e) => {
                    warn!("No commit at HEAD: {}", e);
                    return None;
                }
            };
            // Acquire the commit info
            let commit_id = commit;
            let naive = NaiveDateTime::from_timestamp(commit_time, 0);
            let offset = FixedOffset::east(commit_offset * 60);
            let commit_time = DateTime::<FixedOffset>::from_utc(naive, offset);
            let commit_date = format!("{}", commit_time.format("%Y-%m-%d"));

            let (tag, distance) = match describe(&git_dir, &commit_id) {
                Ok(res) => {
                    let res = &res[..res.rfind('-').expect("No commit info in describe!")];
                    let tag_name = &res[..res.rfind('-').expect("No commit count in describe!")];
                    let commit_count = res[tag_name.len() + 1..]
                        .parse::<usize>()
                        .expect("Unable to parse commit count in describe!");
                    (tag_name.to_owned(), commit_count)
                }
                Err(e) => {
                    warn!("No tag info found!\n{:?}", e);
                    ("".to_owned(), 0)
                }
            };

            Some(CommitInfo {
                id: commit_id,
                date: commit_date,
                tag,
                distance,
            })
        })();

        let status = if commitinfo.is_some() {
            status(&git_dir).expect("Unable to generate status information")
        } else {
            vec![]
        };

        Ok(Self {
            branch,
            commitinfo,
            status,
        })
    }
}

// Clippy thinks our fn main() is needless, but it is needed because otherwise
// we cannot have the invocation of the procedural macro (yet)
#[allow(clippy::needless_doctest_main)]
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

    let InvocationInformation { pkgver, now } = InvocationInformation::acquire();
    let gitinfo = match GitInformation::acquire() {
        Ok(gi) => gi,
        Err(e) => {
            warn!(
                "Unable to open a repo at {}: {}",
                env::var("CARGO_MANIFEST_DIR").unwrap(),
                e
            );
            return (quote! {
                #[allow(clippy::needless_update)]
                static #name: git_testament::GitTestament<'static> = git_testament::GitTestament {
                    commit: git_testament::CommitKind::NoRepository(#pkgver, #now),
                    .. git_testament::EMPTY_TESTAMENT
                };
            })
            .into();
        }
    };

    // Second simple preliminary step: attempt to get a branch name to report
    let branch_name = {
        if let Some(branch) = gitinfo.branch {
            quote! {Some(#branch)}
        } else {
            quote! {None}
        }
    };

    // Step one, determine the current commit ID and the date of that commit
    if gitinfo.commitinfo.is_none() {
        return (quote! {
            #[allow(clippy::needless_update)]
            static #name: git_testament::GitTestament<'static> = git_testament::GitTestament {
                commit: git_testament::CommitKind::NoCommit(#pkgver, #now),
                branch_name: #branch_name,
                .. git_testament::EMPTY_TESTAMENT
            };
        })
        .into();
    }

    let commitinfo = gitinfo.commitinfo.as_ref().unwrap();

    let commit = if !commitinfo.tag.is_empty() {
        // We've a tag
        let (tag, id, date, distance) = (
            &commitinfo.tag,
            &commitinfo.id,
            &commitinfo.date,
            commitinfo.distance,
        );
        quote! {
            git_testament::CommitKind::FromTag(#tag, #id, #date, #distance)
        }
    } else {
        let (id, date) = (&commitinfo.id, &commitinfo.date);
        quote! {
            git_testament::CommitKind::NoTags(#id, #date)
        }
    };

    // Finally, we need to gather the modifications to the tree...
    let statuses: Vec<_> = gitinfo
        .status
        .iter()
        .map(|status| {
            let path = status.path.clone().into_bytes();
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
        #[allow(clippy::needless_update)]
        static #name: git_testament::GitTestament<'static> = git_testament::GitTestament {
            commit: #commit,
            modifications: &[#(#statuses),*],
            branch_name: #branch_name,
            .. git_testament::EMPTY_TESTAMENT
        };
    })
    .into()
}

// Clippy thinks our fn main() is needless, but it is needed because otherwise
// we cannot have the invocation of the procedural macro (yet)
#[allow(clippy::needless_doctest_main)]
/// Generate a testament for the working tree as a set of static string macros.
///
/// This macro declares a set of macros which provide you with your testament
/// as static strings.
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
/// use git_testament::git_testament_macros;
///
/// // Declare a testament, it'll end up as pile of macros, so you can
/// // give it whatever ident-like name you want.  The name will prefix the
/// // macro names.  Also you can optionally specify
/// // a branch name which will be considered the "trusted" branch like in
/// // `git_testament::render_testament!()`
/// git_testament_macros!(version);
/// # fn main() {
///
/// // ... later, you can display the testament.
/// println!("app version {}", version_testament!());
/// # }
/// ```
///
/// The macros all resolve to string literals, boolean literals, or in the case
/// of `NAME_tag_distance!()` a number.  This is most valuable when you are
/// wanting to include the information into a compile-time-constructed string
///
/// ```
/// // Bring the procedural macro into scope
/// use git_testament::git_testament_macros;
///
/// // Declare a testament, it'll end up as pile of macros, so you can
/// // give it whatever ident-like name you want.  The name will prefix the
/// // macro names.  Also you can optionally specify
/// // a branch name which will be considered the "trusted" branch like in
/// // `git_testament::render_testament!()`
/// git_testament_macros!(version);
///
/// const APP_VERSION: &str = concat!("app version ", version_testament!());
/// # fn main() {
///
/// // ... later, you can display the testament.
/// println!("{}", APP_VERSION);
/// # }
/// ```
///
/// The set of macros defined is:
///
/// * `NAME_testament!()` -> produces a string similar but not guaranteed to be
///   identical to the result of `Display` formatting a normal testament.
/// * `NAME_branch!()` -> An Option<&str> of the current branch name
/// * `NAME_repo_present!()` -> A boolean indicating if there is a repo at all
/// * `NAME_commit_present!()` -> A boolean indicating if there is a commit present at all
/// * `NAME_tag_present!()` -> A boolean indicating if there is a tag present
/// * `NAME_commit_hash!()` -> A string of the commit hash (or crate version if commit not present)
/// * `NAME_commit_date!()` -> A string of the commit date (or build date if no commit present)
/// * `NAME_tag_name!()` -> The tag name if present (or crate version if commit not present)
/// * `NAME_tag_distance!()` -> The number of commits since the tag if present (zero otherwise)
#[proc_macro]
pub fn git_testament_macros(input: TokenStream) -> TokenStream {
    let StaticTestamentOptions { name, trusted } =
        parse_macro_input!(input as StaticTestamentOptions);
    let sname = format!("{}", name);
    let (pkgver, now, gitinfo, macros) = macro_content(&sname);

    // Render the testament string
    let testament = if let Some(gitinfo) = gitinfo {
        let commitstr = if let Some(ref commitinfo) = gitinfo.commitinfo {
            if commitinfo.tag.is_empty() {
                // No tag
                format!("unknown ({} {})", &commitinfo.id[..9], commitinfo.date)
            } else {
                let trusted = if gitinfo.branch == trusted {
                    gitinfo.status.is_empty()
                } else {
                    false
                };
                // Full behaviour
                if trusted {
                    format!("{} ({} {})", pkgver, &commitinfo.id[..9], commitinfo.date)
                } else {
                    let basis = if commitinfo.distance > 0 {
                        format!(
                            "{}+{} ({} {})",
                            commitinfo.tag,
                            commitinfo.distance,
                            &commitinfo.id[..9],
                            commitinfo.date
                        )
                    } else {
                        // Not dirty
                        format!(
                            "{} ({} {})",
                            commitinfo.tag,
                            &commitinfo.id[..9],
                            commitinfo.date
                        )
                    };
                    if commitinfo.tag.find(&pkgver).is_some() {
                        basis
                    } else {
                        format!("{} :: {}", pkgver, basis)
                    }
                }
            }
        } else {
            // We're in a repo, but with no commit
            format!("{} (uncommitted {})", pkgver, now)
        };
        if gitinfo.status.is_empty() {
            commitstr
        } else {
            format!(
                "{} dirty {} modification{}",
                commitstr,
                gitinfo.status.len(),
                if gitinfo.status.len() == 1 { "" } else { "s" }
            )
        }
    } else {
        // No git information whatsoever
        format!("{} ({})", pkgver, now)
    };

    let mac_testament = concat_ident(&sname, "testament");

    (quote! {
            #macros
            macro_rules! #mac_testament { () => {#testament}}
    })
    .into()
}

fn macro_content(prefix: &str) -> (String, String, Option<GitInformation>, impl quote::ToTokens) {
    let InvocationInformation { pkgver, now } = InvocationInformation::acquire();
    let mac_branch = concat_ident(prefix, "branch");
    let mac_repo_present = concat_ident(prefix, "repo_present");
    let mac_commit_present = concat_ident(prefix, "commit_present");
    let mac_tag_present = concat_ident(prefix, "tag_present");
    let mac_commit_hash = concat_ident(prefix, "commit_hash");
    let mac_commit_date = concat_ident(prefix, "commit_date");
    let mac_tag_name = concat_ident(prefix, "tag_name");
    let mac_tag_distance = concat_ident(prefix, "tag_distance");
    let gitinfo = match GitInformation::acquire() {
        Ok(gi) => gi,
        Err(e) => {
            warn!(
                "Unable to open a repo at {}: {}",
                env::var("CARGO_MANIFEST_DIR").unwrap(),
                e
            );
            return (
                pkgver.clone(),
                now.clone(),
                None,
                quote! {
                    macro_rules! #mac_branch { () => {None}}
                    macro_rules! #mac_repo_present { () => {false}}
                    macro_rules! #mac_commit_present { () => {false}}
                    macro_rules! #mac_tag_present { () => {false}}
                    macro_rules! #mac_commit_hash { () => {#pkgver}}
                    macro_rules! #mac_commit_date { () => {#now}}
                    macro_rules! #mac_tag_name { () => {#pkgver}}
                    macro_rules! #mac_tag_distance { () => {0}}
                },
            );
        }
    };

    let branch_name = {
        if let Some(ref branch) = gitinfo.branch {
            quote! {Some(#branch)}
        } else {
            quote! {None}
        }
    };

    let basics = quote! {
        macro_rules! #mac_repo_present { () => {true}}
        macro_rules! #mac_branch { () => {#branch_name}}
    };

    // Step one, determine the current commit ID and the date of that commit
    if gitinfo.commitinfo.is_none() {
        return (
            pkgver.clone(),
            now.clone(),
            Some(gitinfo),
            quote! {
                #basics
                macro_rules! #mac_commit_present { () => {false}}
                macro_rules! #mac_tag_present { () => {false}}
                macro_rules! #mac_commit_hash { () => {#pkgver}}
                macro_rules! #mac_commit_date { () => {#now}}
                macro_rules! #mac_tag_name { () => {#pkgver}}
                macro_rules! #mac_tag_distance { () => {0}}
            },
        );
    }

    let commitinfo = gitinfo.commitinfo.as_ref().unwrap();
    let (commit_hash, commit_date) = (&commitinfo.id, &commitinfo.date);
    let (tag, distance) = (&commitinfo.tag, commitinfo.distance);

    let basics = quote! {
        #basics
        macro_rules! #mac_commit_present { () => {true}}
        macro_rules! #mac_commit_hash { () => {#commit_hash}}
        macro_rules! #mac_commit_date { () => {#commit_date}}
    };

    (
        pkgver.clone(),
        now,
        Some(gitinfo.clone()),
        if commitinfo.tag.is_empty() {
            quote! {
                #basics
                macro_rules! #mac_tag_present { () => {false}}
                macro_rules! #mac_tag_name { () => {#pkgver}}
                macro_rules! #mac_tag_distance { () => {0}}
            }
        } else {
            quote! {
                #basics
                macro_rules! #mac_tag_present { () => {true}}
                macro_rules! #mac_tag_name { () => {#tag}}
                macro_rules! #mac_tag_distance { () => {#distance}}
            }
        },
    )
}

fn concat_ident(prefix: &str, suffix: &str) -> Ident {
    Ident::new(&format!("{}_{}", prefix, suffix), Span::call_site())
}
