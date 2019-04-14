use git_testament::{git_testament, render_testament};

git_testament!(TESTAMENT);

fn main() {
    println!("{}", render_testament!(TESTAMENT, "trusted"));
}
