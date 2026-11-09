//! yaaihh without extensions: the projects, their agents and views (lib.rs). A variant with extensions (an account
//! proxy's pages, say) is its own small binary calling [`yaaihh::run`] with them.

fn main() {
    yaaihh::run(Vec::new());
}
