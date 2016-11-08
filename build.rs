//! Get the version string using `git describe --dirty` or, if it
//! fails, using the `CARGO_PKG_VERSION`.
//!
//! The `GIT` environment variable can be used to set an alternative
//! path to the git executable.

use std::env;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::process::Command;

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("version.rs");
    let mut f = File::create(&dest_path).unwrap();

    let git =
        env::var("GIT").unwrap_or("git".into());

    let description =
        Command::new(git)
        .arg("describe")
        .arg("--dirty")
        .output();

    let cargo_version = env!("CARGO_PKG_VERSION").to_owned();

    let mut version =
        match description {
            Ok(output) => {
                if output.status.success() {
                    format!("git-{}",
                            String::from_utf8(output.stdout).unwrap())
                } else {
                    cargo_version
                }
            }
            _ => cargo_version,
        };

    // Make sure version is on a single line
    if let Some(l) = version.find('\n') {
        version.truncate(l);
    }

    writeln!(f, "pub const VERSION: &'static str = \
                 \"{}\";", version).unwrap();
    writeln!(f, "pub const VERSION_CSTR: &'static str = \
                 \"{}\\0\";", version).unwrap();
}
