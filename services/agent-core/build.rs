//! Rebuilds the crate when the git SHA it embeds changes, so a local build never keeps a stale one.

fn main() {
    println!("cargo:rerun-if-env-changed=ARIA_GIT_SHA");
}
