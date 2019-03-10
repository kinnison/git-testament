use git_testament::git_testament;

git_testament!(TESTAMENT);

#[test]
fn it_works() {
    println!("Testament: {}", TESTAMENT);
}

use testutils;

#[test]
fn verify_builds_ok() {
    let test = testutils::prep_test("no-git");
    assert!(test.run_cmd("cargo", &["build"]));
    test.assert_manifest_exact("not_in_git");
}

#[test]
fn verify_no_commit() {
    let test = testutils::prep_test("no-commit");
    assert!(test.run_cmd("git", &["init"]));
    assert!(test.run_cmd("cargo", &["build"]));
    test.assert_manifest_exact("uncommitted");
}

#[test]
fn verify_no_changes_no_tags() {
    let test = testutils::prep_test("no-changes");
    assert!(test.run_cmd("git", &["init"]));
    assert!(test.run_cmd("cargo", &["check"]));
    assert!(test.run_cmd("git", &["add", "."]));
    assert!(test.run_cmd("git", &["commit", "-m", "first"]));
    assert!(test.run_cmd("cargo", &["build"]));
    test.assert_manifest_parts("unknown", 0, "TODO", None);
}

#[test]
fn verify_no_changes_with_a_tag() {
    let test = testutils::prep_test("no-changes");
    assert!(test.run_cmd("git", &["init"]));
    assert!(test.run_cmd("cargo", &["check"]));
    assert!(test.run_cmd("git", &["add", "."]));
    assert!(test.run_cmd("git", &["commit", "-m", "first"]));
    assert!(test.run_cmd("git", &["tag", "1.0"]));
    assert!(test.run_cmd("cargo", &["build"]));
    test.assert_manifest_parts("1.0", 0, "TODO", None);
}

#[test]
fn verify_dirty_changes_with_a_tag() {
    let test = testutils::prep_test("no-changes");
    assert!(test.run_cmd("git", &["init"]));
    assert!(test.run_cmd("cargo", &["check"]));
    assert!(test.run_cmd("git", &["add", "."]));
    assert!(test.run_cmd("git", &["commit", "-m", "first"]));
    assert!(test.run_cmd("git", &["tag", "1.0"]));
    test.dirty_code();
    assert!(test.run_cmd("cargo", &["build"]));
    test.assert_manifest_parts("1.0", 0, "TODO", Some(1));
}

#[test]
fn verify_another_commit_with_a_tag() {
    let test = testutils::prep_test("no-changes");
    assert!(test.run_cmd("git", &["init"]));
    assert!(test.run_cmd("cargo", &["check"]));
    assert!(test.run_cmd("git", &["add", "."]));
    assert!(test.run_cmd("git", &["commit", "-m", "first"]));
    assert!(test.run_cmd("git", &["tag", "1.0"]));
    test.dirty_code();
    assert!(test.run_cmd("git", &["add", "."]));
    assert!(test.run_cmd("git", &["commit", "-m", "second"]));
    assert!(test.run_cmd("cargo", &["build"]));
    test.assert_manifest_parts("1.0", 1, "TODO", None);
}
