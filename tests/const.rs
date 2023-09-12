use git_testament::{git_testament, git_testament_macros};

git_testament!(TESTAMENT);

git_testament_macros!(TESTAMENT);

const TESTAMENT_BRANCH_NAME_OR_DEFAULT: &str = {
    match TESTAMENT.branch_name {
        Some(branch_name) => branch_name,
        None => "main",
    }
};

const MACROS_BRANCH_NAME_OR_DEFAULT: &str = {
    match TESTAMENT_branch!() {
        Some(branch_name) => branch_name,
        None => "main",
    }
};

#[test]
fn it_works() {
    assert_eq!(
        TESTAMENT_BRANCH_NAME_OR_DEFAULT,
        MACROS_BRANCH_NAME_OR_DEFAULT
    );
}
