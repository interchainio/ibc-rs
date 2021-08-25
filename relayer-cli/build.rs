use std::env;

use git::Handle as GitHandle;

fn main() {
    // Overwrites the package `version` to include metadata, and package `name` to be 'hermes'
    // (the binary name), instead of 'ibc-relayer-cli' (which is the crate name), so that abscissa
    // outputs consistent usage and help messages.
    // https://github.com/informalsystems/ibc-rs/issues/590
    // Note: This can potentially break the normal cargo (or crates.io) workflow.
    println!("cargo:rustc-env=CARGO_PKG_NAME=hermes");
    println!("cargo:rustc-env=CARGO_PKG_VERSION={}", version());
}

// returns a valid semver string optionally suffixed with the current git branch and last commit
// hash. e.g. 0.4.0 or 0.4.0+cli-git-hash-eb0e94fc-dirty
fn version() -> String {
    let mut vers = env::var("CARGO_PKG_VERSION").unwrap();
    if !is_ci_release() {
        if let Some(git) = GitHandle::new() {
            println!("cargo:rustc-rerun-if-changed=.git/HEAD");
            vers.push('+');
            vers.push_str(&git.last_commit_hash());
            if git.is_dirty() {
                vers.push_str("-dirty");
            }
            vers = vers.replace("\n", "");
        }
    }
    vers
}

fn is_ci_release() -> bool {
    matches!(env::var("GITHUB_JOB"), Ok(job) if job == "create-release")
}

mod git {
    use core::marker::PhantomData;
    use std::ffi::OsStr;
    use std::process::{Command, Output};

    // A wrapper over a git shell command that is only constructable if git is available & the
    // current directory is a git repository
    pub struct Handle(PhantomData<Command>);

    impl Handle {
        pub fn new() -> Option<Self> {
            if Self::is_git_repo() {
                Some(Handle(PhantomData))
            } else {
                None
            }
        }

        // Returns the short hash of the last git commit
        pub fn last_commit_hash(&self) -> String {
            let commit = Self::command(&["rev-parse", "--short", "HEAD"]);
            String::from_utf8_lossy(&commit.stdout).into_owned()
        }

        // Checks if the git repo is dirty
        pub fn is_dirty(&self) -> bool {
            !Self::command(&["diff-index", "--quiet", "HEAD", "--"])
                .status
                .success()
        }

        #[inline]
        fn command(args: impl IntoIterator<Item = impl AsRef<OsStr>>) -> Output {
            Command::new("git").args(args).output().unwrap()
        }

        // returns true iff git is installed and current directory is a git repo
        fn is_git_repo() -> bool {
            Command::new("git")
                .args(&["rev-parse", "--git-dir"])
                .output()
                .map_or(false, |o| o.status.success())
        }
    }
}
