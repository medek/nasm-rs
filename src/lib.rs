#![feature(if_let)]

use std::os;
use std::io::Command;
use std::io::process::InheritFd;

/// # Example
///
/// ```no_run
/// nasm::compile_library("libfoo.a", &["foo.s", "bar.s"]);
/// ```
pub fn compile_library(output: &str, files: &[&str]) {
    assert!(output.starts_with("lib"));

    assert!(output.ends_with(".a"));

    let target = os::getenv("TARGET").unwrap();

    let opt_level = os::getenv("OPT_LEVEL").unwrap();

    let mut cmd = Command::new("nasm");

    cmd.arg("-felf64");

    let src = Path::new(os::getenv("CARGO_MANIFEST_DIR").unwrap());

    let dst = Path::new(os::getenv("OUT_DIR").unwrap());

    let mut objects = Vec::new();

    for file in files.iter() {
        let obj = dst.join(*file).with_extension("o");

        std::io::fs::mkdir_recursive(&obj.dir_path(), std::io::USER_RWX).unwrap();

        run(cmd.clone().arg(src.join(*file)).arg("-o").arg(&obj));

        objects.push(obj);
    }

    run(Command::new(ar()).arg("crus").arg(dst.join(output)).args(objects.as_slice()));

    println!("cargo:rustc-flags=-L {} -l {}:static",
             dst.display(),
             output.slice(3, output.len() - 2));
}

fn run(cmd: &mut Command) {
    println!("running: {}", cmd);

    let status = match cmd.stdout(InheritFd(1)).stderr(InheritFd(2)).status() {
        Ok(status) => status,

        Err(e) => panic!("failed to spawn process: {}", e),
    };

    if !status.success() {
        panic!("nonzero exit status: {}", status);
    }
}

fn ar() -> String {
    os::getenv("AR").unwrap_or("ar".to_string())
}
