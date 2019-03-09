extern crate proc_macro;

use std::env;

use proc_macro::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream, Result};
use syn::{parse_macro_input, Ident};

use git2::Repository;

use chrono::prelude::{DateTime, FixedOffset, NaiveDateTime};

use log::warn;

struct TestamentOptions {
    name: Ident,
}

impl Parse for TestamentOptions {
    fn parse(input: ParseStream) -> Result<Self> {
        let name: Ident = input.parse()?;
        Ok(TestamentOptions { name })
    }
}

#[proc_macro]
pub fn git_testament(input: TokenStream) -> TokenStream {
    let TestamentOptions { name } = parse_macro_input!(input as TestamentOptions);

    let ceilings = env::var("GIT_CEILING_DIRECTORIES").unwrap_or_else(|_| "/".to_owned());

    let ceilings = ceilings.split(':');

    let repo = match Repository::open_ext(
        env::var("CARGO_MANIFEST_DIR").expect("Unable to find CARGO_MANIFEST_DIR"),
        git2::RepositoryOpenFlags::empty(),
        ceilings,
    ) {
        Ok(repo) => repo,
        Err(e) => {
            warn!(
                "Unable to open a repo at {}: {}",
                env::var("CARGO_MANIFEST_DIR").unwrap(),
                e
            );
            return (quote! {
                static #name: git_testament::GitTestament<'static> = git_testament::GitTestament {
                    commit: git_testament::CommitKind::NoRepository,
                    modifications: &[],
                };
            })
            .into();
        }
    };

    // Step one, determine the current commit ID and the date of that commit
    let (commit_id, commit_date) = {
        let spec = match repo.revparse_single("HEAD") {
            Ok(spec) => spec,
            Err(e) => {
                warn!("No commit at HEAD: {}", e);
                return (quote! {
                static #name: git_testament::GitTestament<'static> = git_testament::GitTestament {
                    commit: git_testament::CommitKind::NoCommit,
                    modifications: &[],
                };
            })
            .into();
            }
        };

        let commit = match spec.peel_to_commit() {
            Ok(commit) => commit,
            Err(e) => panic!(
                "Unable to continue, HEAD references something which isn't a commit: {}",
                e
            ),
        };

        // Acquire the commit info

        let commit_id = format!("{}", commit.id());
        let naive = NaiveDateTime::from_timestamp(commit.time().seconds(), 0);
        let offset = FixedOffset::east(commit.time().offset_minutes() * 60);
        let commit_time = DateTime::<FixedOffset>::from_utc(naive, offset);
        let commit_date = format!("{}", commit_time.format("%Y-%m-%d"));

        (commit_id, commit_date)
    };

    // Next determine if there was a tag, and if so, what our relationship
    // to that tag is...

    let (tag, steps) = match repo.describe(git2::DescribeOptions::new().describe_tags()) {
        Ok(desc) => {
            let res = desc
                .format(Some(
                    git2::DescribeFormatOptions::new().always_use_long_format(true),
                ))
                .expect("Unable to format tag information");

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
    let statuses: Vec<_> = repo
        .statuses(Some(
            git2::StatusOptions::new()
                .include_untracked(true)
                .exclude_submodules(true),
        ))
        .expect("Unable to generate status information for working tree!")
        .iter()
        .map(|status| {
            let path = status.path_bytes();
            use git2::Delta::*;
            let htoi = status
                .head_to_index()
                .map(|s| s.status())
                .unwrap_or(Unmodified);
            let itow = status
                .index_to_workdir()
                .map(|s| s.status())
                .unwrap_or(Unmodified);
            if htoi == Untracked || itow == Untracked {
                quote! {
                    git_testament::GitModification::Untracked(&[#(#path),*])
                }
            } else if htoi == Added || itow == Added {
                quote! {
                    git_testament::GitModification::Added(&[#(#path),*])
                }
            } else if htoi == Modified
                || htoi == Typechange
                || itow == Modified
                || itow == Typechange
            {
                quote! {
                    git_testament::GitModification::Modified(&[#(#path),*])
                }
            } else if htoi == Deleted || itow == Deleted {
                quote! {
                    git_testament::GitModification::Removed(&[#(#path),*])
                }
            } else {
                quote! {}
            }
        })
        .collect();

    (quote! {
        static #name: git_testament::GitTestament<'static> = git_testament::GitTestament {
            commit: #commit,
            modifications: &[#(#statuses),*],
        };
    })
    .into()
}
