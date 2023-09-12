#![no_implicit_prelude]

use ::git_testament::{git_testament, git_testament_macros};

git_testament!(TESTAMENT);

git_testament_macros!(TESTAMENT);

#[test]
fn it_works() {
    ::core::assert_eq!(TESTAMENT_branch!(), TESTAMENT.branch_name);
}
