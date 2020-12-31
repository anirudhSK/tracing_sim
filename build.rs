use std::process::Command;

// Example custom build script.
fn main() {
    // Note: needs to be run with cargo -vv to generate output from these commands.
    eprintln!("{:?}",
              Command::new("rustc").args(&["plugins/sample.rs", "--crate-type=dylib"]).output().unwrap());
    eprintln!("{:?}",
              Command::new("rustc").args(&["plugins/sample2.rs", "--crate-type=dylib"]).output().unwrap());
    eprintln!("{:?}",
              Command::new("rustc").args(&["plugins/compiled_sample.rs", "--crate-type=dylib"]).output().unwrap());
    eprintln!("{:?}",
              Command::new("rustc").args(&["plugins/sink.rs", "--crate-type=dylib"]).output().unwrap());

    // Don't go any further if these commands failed.
    assert!(Command::new("rustc").args(&["plugins/sample.rs",  "--crate-type=dylib"]).status().unwrap().success());
    assert!(Command::new("rustc").args(&["plugins/sample2.rs", "--crate-type=dylib"]).status().unwrap().success());
    assert!(Command::new("rustc").args(&["plugins/compiled_sample.rs", "--crate-type=dylib"]).status().unwrap().success());
    assert!(Command::new("rustc").args(&["plugins/sink.rs"  ,  "--crate-type=dylib"]).status().unwrap().success());

    // Tell Cargo that if the given file changes, to rerun this build script.
    println!("cargo:rerun-if-changed=./plugins");
    println!("cargo:rerun-if-changed=./plugins/sample.rs");
    println!("cargo:rerun-if-changed=./plugins/sample2.rs");
    println!("cargo:rerun-if-changed=./plugins/compiled_sample.rs");
    println!("cargo:rerun-if-changed=./plugins/sink.rs");
}
