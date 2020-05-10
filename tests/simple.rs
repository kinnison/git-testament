use git_testament::git_testament;

git_testament!(TESTAMENT);

#[test]
fn it_works() {
    println!("Testament: {}", TESTAMENT);
}

mod testutils;

#[test]
fn verify_builds_ok() {
    let test = testutils::prep_test("no-git");
    assert!(test.run_cmd("cargo", &["build"]));
    test.assert_manifest_contains("1.0.0");
}

#[test]
fn verify_no_commit() {
    let test = testutils::prep_test("no-commit");
    assert!(test.basic_git_init());
    assert!(test.run_cmd("cargo", &["build"]));
    test.assert_manifest_contains("uncommitted");
}

#[test]
fn verify_no_changes_no_tags() {
    let test = testutils::prep_test("no-changes");
    assert!(test.basic_git_init());
    assert!(test.run_cmd("cargo", &["check"]));
    assert!(test.run_cmd("git", &["add", "."]));
    assert!(test.run_cmd("git", &["commit", "-m", "first"]));
    assert!(test.run_cmd("cargo", &["build"]));
    test.assert_manifest_parts("unknown", 0, "TODO", None);
}

#[test]
fn verify_no_changes_with_a_tag() {
    let test = testutils::prep_test("no-changes-with-tag");
    assert!(test.basic_git_init());
    assert!(test.run_cmd("cargo", &["check"]));
    assert!(test.run_cmd("git", &["add", "."]));
    assert!(test.run_cmd("git", &["commit", "-m", "first"]));
    assert!(test.run_cmd("git", &["tag", "1.0.0"]));
    assert!(test.run_cmd("cargo", &["build"]));
    test.assert_manifest_parts("1.0.0", 0, "TODO", None);
}

#[test]
fn verify_dirty_changes_with_a_tag() {
    let test = testutils::prep_test("dirty-with-tag");
    assert!(test.basic_git_init());
    assert!(test.run_cmd("cargo", &["check"]));
    assert!(test.run_cmd("git", &["add", "."]));
    assert!(test.run_cmd("git", &["commit", "-m", "first"]));
    assert!(test.run_cmd("git", &["tag", "1.0.0"]));
    test.dirty_code();
    assert!(test.run_cmd("cargo", &["build"]));
    test.assert_manifest_parts("1.0.0", 0, "TODO", Some(1));
}

#[test]
fn verify_another_commit_with_a_tag() {
    let test = testutils::prep_test("tag-plus-commit");
    assert!(test.basic_git_init());
    assert!(test.run_cmd("cargo", &["check"]));
    assert!(test.run_cmd("git", &["add", "."]));
    assert!(test.run_cmd("git", &["commit", "-m", "first"]));
    assert!(test.run_cmd("git", &["tag", "1.0.0"]));
    test.dirty_code();
    assert!(test.run_cmd("git", &["add", "."]));
    assert!(test.run_cmd("git", &["commit", "-m", "second"]));
    assert!(test.run_cmd("cargo", &["build"]));
    test.assert_manifest_parts("1.0.0", 1, "TODO", None);
}

#[test]
fn verify_trusted_branch() {
    let test = testutils::prep_test("trusted-branch");
    assert!(test.basic_git_init());
    assert!(test.run_cmd("cargo", &["check"]));
    assert!(test.run_cmd("git", &["add", "."]));
    assert!(test.run_cmd("git", &["commit", "-m", "first"]));
    assert!(test.run_cmd("git", &["tag", "0.1.0"]));
    assert!(test.run_cmd("git", &["checkout", "-b", "aaaa"]));
    test.dirty_code();
    assert!(test.run_cmd("git", &["add", "."]));
    assert!(test.run_cmd("git", &["commit", "-m", "second"]));
    assert!(test.run_cmd("git", &["checkout", "-b", "trusted"]));
    assert!(test.run_cmd("cargo", &["build"]));
    test.assert_manifest_parts("1.0.0", 0, "TODO", None);
}

#[test]
fn verify_source_date_epoch_no_repo() {
    let mut test = testutils::prep_test("source-date-epoch-norepo");
    test.setenv("SOURCE_DATE_EPOCH", "324086400");
    assert!(test.run_cmd("cargo", &["build"]));
    test.assert_manifest_contains("1.0.0");
    test.assert_manifest_contains("1980-04-09");
}

#[test]
fn verify_source_date_epoch_no_commit() {
    let mut test = testutils::prep_test("source-date-epoch-nocommit");
    assert!(test.basic_git_init());
    test.setenv("SOURCE_DATE_EPOCH", "324086400");
    assert!(test.run_cmd("cargo", &["build"]));
    test.assert_manifest_contains("1.0.0");
    test.assert_manifest_contains("1980-04-09");
}
