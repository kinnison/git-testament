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
use syn::parse::{Parse, ParseStream};
use syn::{parse, Visibility};
use syn::{parse_macro_input, Ident, LitStr};

use log::warn;

use time::{format_description::FormatItem, macros::format_description, OffsetDateTime, UtcOffset};

const DATE_FORMAT: &[FormatItem<'_>] = format_description!("[year]-[month]-[day]");

struct TestamentOptions {
    crate_: Ident,
    name: Ident,
    vis: Option<Visibility>,
}

impl Parse for TestamentOptions {
    fn parse(input: ParseStream) -> parse::Result<Self> {
        let crate_ = input.parse()?;
        let name = input.parse()?;
        let vis = if input.is_empty() {
            None
        } else {
            Some(input.parse()?)
        };
        Ok(TestamentOptions { crate_, name, vis })
    }
}

struct StaticTestamentOptions {
    crate_: Ident,
    name: Ident,
    trusted: Option<LitStr>,
}

impl Parse for StaticTestamentOptions {
    fn parse(input: ParseStream) -> parse::Result<Self> {
        Ok(StaticTestamentOptions {
            crate_: input.parse()?,
            name: input.parse()?,
            trusted: input.parse()?,
        })
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
        env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR env variable not set"),
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
                return Err(format!("Insufficient committer data in {line}").into());
            }
            let time: i64 = parts[parts.len() - 2].parse()?;
            let offset: &str = parts[parts.len() - 1];
            if offset.len() != 5 {
                return Err(
                    format!("Insufficient/Incorrect data in timezone offset: {offset}").into(),
                );
            }
            let hours: i32 = offset[1..=2].parse()?;
            let mins: i32 = offset[3..=4].parse()?;
            let absoffset: i32 = mins + (hours * 60);
            let offset: i32 = if offset.starts_with('-') {
                // Negative...
                -absoffset
            } else {
                // Positive...
                absoffset
            };
            return Ok((sha, time, offset));
        } else if line.is_empty() {
            // Ran out of input, without finding committer
            return Err(format!("Unable to find committer information in {refname}").into());
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
        let now = OffsetDateTime::now_utc();
        let now = now.format(DATE_FORMAT).expect("unable to format now");
        let sde = match env::var("SOURCE_DATE_EPOCH") {
            Ok(sde) => match sde.parse::<i64>() {
                Ok(sde) => Some(
                    OffsetDateTime::from_unix_timestamp(sde)
                        .expect("couldn't contruct datetime from source date epoch")
                        .format(DATE_FORMAT)
                        .expect("couldn't format source date epoch datetime"),
                ),
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
                warn!("Unable to determine branch name: {e}");
                None
            }
        };

        let commitinfo = (|| {
            let (commit, commit_time, commit_offset) = match revparse_single(&git_dir, "HEAD") {
                Ok(commit_data) => commit_data,
                Err(e) => {
                    warn!("No commit at HEAD: {e}");
                    return None;
                }
            };
            // Acquire the commit info
            let commit_id = commit;
            let naive =
                OffsetDateTime::from_unix_timestamp(commit_time).expect("Invalid commit time");
            let offset = UtcOffset::from_whole_seconds(commit_offset * 60)
                .expect("Invalid UTC offset (seconds)");
            let commit_time = naive.replace_offset(offset);
            let commit_date = commit_time
                .format(DATE_FORMAT)
                .expect("unable to format commit date");

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

#[proc_macro]
pub fn git_testament(input: TokenStream) -> TokenStream {
    let TestamentOptions { crate_, name, vis } = parse_macro_input!(input);

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
                #vis const #name: #crate_::GitTestament<'static> = #crate_::GitTestament {
                    commit: #crate_::CommitKind::NoRepository(#pkgver, #now),
                    .. #crate_::EMPTY_TESTAMENT
                };
            })
            .into();
        }
    };

    // Second simple preliminary step: attempt to get a branch name to report
    let branch_name = {
        if let Some(branch) = gitinfo.branch {
            quote! {#crate_::__core::option::Option::Some(#branch)}
        } else {
            quote! {#crate_::__core::option::Option::None}
        }
    };

    // Step one, determine the current commit ID and the date of that commit
    if gitinfo.commitinfo.is_none() {
        return (quote! {
            #[allow(clippy::needless_update)]
            #vis const #name: #crate_::GitTestament<'static> = #crate_::GitTestament {
                commit: #crate_::CommitKind::NoCommit(#pkgver, #now),
                branch_name: #branch_name,
                .. #crate_::EMPTY_TESTAMENT
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
            #crate_::CommitKind::FromTag(#tag, #id, #date, #distance)
        }
    } else {
        let (id, date) = (&commitinfo.id, &commitinfo.date);
        quote! {
            #crate_::CommitKind::NoTags(#id, #date)
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
                    #crate_::GitModification::Untracked(&[#(#path),*])
                },
                Added => quote! {
                    #crate_::GitModification::Added(&[#(#path),*])
                },
                Modified => quote! {
                    #crate_::GitModification::Modified(&[#(#path),*])
                },
                Deleted => quote! {
                    #crate_::GitModification::Removed(&[#(#path),*])
                },
            }
        })
        .collect();

    (quote! {
        #[allow(clippy::needless_update)]
        #vis const #name: #crate_::GitTestament<'static> = #crate_::GitTestament {
            commit: #commit,
            modifications: &[#(#statuses),*],
            branch_name: #branch_name,
            .. #crate_::EMPTY_TESTAMENT
        };
    })
    .into()
}

#[proc_macro]
pub fn git_testament_macros(input: TokenStream) -> TokenStream {
    let StaticTestamentOptions {
        crate_,
        name,
        trusted,
    } = parse_macro_input!(input);
    let sname = name.to_string();
    let (pkgver, now, gitinfo, macros) = macro_content(&crate_, &sname);

    // Render the testament string
    let testament = if let Some(gitinfo) = gitinfo {
        let commitstr = if let Some(ref commitinfo) = gitinfo.commitinfo {
            if commitinfo.tag.is_empty() {
                // No tag
                format!("unknown ({} {})", &commitinfo.id[..9], commitinfo.date)
            } else {
                let trusted = if gitinfo.branch == trusted.map(|v| v.value()) {
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
                    if commitinfo.tag.contains(&pkgver) {
                        basis
                    } else {
                        format!("{pkgver} :: {basis}")
                    }
                }
            }
        } else {
            // We're in a repo, but with no commit
            format!("{pkgver} (uncommitted {now})")
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
        format!("{pkgver} ({now})")
    };

    let mac_testament = concat_ident(&sname, "testament");

    (quote! {
            #macros
            #[allow(unused_macros)]
            macro_rules! #mac_testament { () => {#testament}}
    })
    .into()
}

fn macro_content(
    crate_: &Ident,
    prefix: &str,
) -> (String, String, Option<GitInformation>, impl quote::ToTokens) {
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
                    #[allow(unused_macros)]
                    macro_rules! #mac_branch { () => {None}}
                    #[allow(unused_macros)]
                    macro_rules! #mac_repo_present { () => {false}}
                    #[allow(unused_macros)]
                    macro_rules! #mac_commit_present { () => {false}}
                    #[allow(unused_macros)]
                    macro_rules! #mac_tag_present { () => {false}}
                    #[allow(unused_macros)]
                    macro_rules! #mac_commit_hash { () => {#pkgver}}
                    #[allow(unused_macros)]
                    macro_rules! #mac_commit_date { () => {#now}}
                    #[allow(unused_macros)]
                    macro_rules! #mac_tag_name { () => {#pkgver}}
                    #[allow(unused_macros)]
                    macro_rules! #mac_tag_distance { () => {0}}
                },
            );
        }
    };

    let branch_name = {
        if let Some(ref branch) = gitinfo.branch {
            quote! {#crate_::__core::option::Option::Some(#branch)}
        } else {
            quote! {#crate_::__core::option::Option::None}
        }
    };

    let basics = quote! {
        #[allow(unused_macros)]
        macro_rules! #mac_repo_present { () => {true}}
        #[allow(unused_macros)]
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
                #[allow(unused_macros)]
                macro_rules! #mac_commit_present { () => {false}}
                #[allow(unused_macros)]
                macro_rules! #mac_tag_present { () => {false}}
                #[allow(unused_macros)]
                macro_rules! #mac_commit_hash { () => {#pkgver}}
                #[allow(unused_macros)]
                macro_rules! #mac_commit_date { () => {#now}}
                #[allow(unused_macros)]
                macro_rules! #mac_tag_name { () => {#pkgver}}
                #[allow(unused_macros)]
                macro_rules! #mac_tag_distance { () => {0}}
            },
        );
    }

    let commitinfo = gitinfo.commitinfo.as_ref().unwrap();
    let (commit_hash, commit_date) = (&commitinfo.id, &commitinfo.date);
    let (tag, distance) = (&commitinfo.tag, commitinfo.distance);

    let basics = quote! {
        #basics
        #[allow(unused_macros)]
        macro_rules! #mac_commit_present { () => {true}}
        #[allow(unused_macros)]
        macro_rules! #mac_commit_hash { () => {#commit_hash}}
        #[allow(unused_macros)]
        macro_rules! #mac_commit_date { () => {#commit_date}}
    };

    (
        pkgver.clone(),
        now,
        Some(gitinfo.clone()),
        if commitinfo.tag.is_empty() {
            quote! {
                #basics
                #[allow(unused_macros)]
                macro_rules! #mac_tag_present { () => {false}}
                #[allow(unused_macros)]
                macro_rules! #mac_tag_name { () => {#pkgver}}
                #[allow(unused_macros)]
                macro_rules! #mac_tag_distance { () => {0}}
            }
        } else {
            quote! {
                #basics
                #[allow(unused_macros)]
                macro_rules! #mac_tag_present { () => {true}}
                #[allow(unused_macros)]
                macro_rules! #mac_tag_name { () => {#tag}}
                #[allow(unused_macros)]
                macro_rules! #mac_tag_distance { () => {#distance}}
            }
        },
    )
}

fn concat_ident(prefix: &str, suffix: &str) -> Ident {
    Ident::new(&format!("{prefix}_{suffix}"), Span::call_site())
}
