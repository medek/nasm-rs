use std::env;
use std::process::Command;
use std::process::Stdio;
use std::path::Path;

fn x86_triple(os: &str) -> &'static str {
    match os {
        "linux" => "-felf32",
        "macos" => "-fmachos32",
        "windows" => "-fwin32",
        _ => ""
    }
}

fn x86_64_triple(os: &str) -> &'static str {
    match os {
        "linux" => "-felf64",
        "macos" => "-felf64",
        "windows" => "-fwin64",
        _ => ""
    }
}

fn parse_triple(trip: &str) -> &'static str {
    let parts = trip.split("-").collect::<Vec<&str>>();
    // ARCH-VENDOR-OS-ENVIRONMENT
    // or ARCH-VENDOR-OS
    // we don't care about environ so doesn't matter if triple doesn't have it
    if parts.len() < 3 {
        return ""
    }

    match parts[0] {
        "x86_64" => x86_64_triple(&parts[2]),
        "x86" => x86_triple(&parts[2]),
        _ => ""
    }
}

/// # Example
///
/// ```no_run
/// nasm::compile_library("libfoo.a", &["foo.s", "bar.s"]);
/// ```
pub fn compile_library(output: &str, files: &[&str]) {
    assert!(output.starts_with("lib"));

    assert!(output.ends_with(".a"));

    let target = env::var("TARGET").unwrap();

    let cargo_manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let out_dir = env::var("OUT_DIR").unwrap();
    let mut args:Vec<&str> = Vec::new();
    args.push(parse_triple(&target));

    if env::var_os("DEBUG").is_some() {
        args.push("-g");
    }

    let src = Path::new(&cargo_manifest_dir);

    let dst = Path::new(&out_dir);

    let mut objects = Vec::new();

    for file in files.iter() {
        let obj = dst.join(*file).with_extension("o");
        let mut cmd = Command::new("nasm");
        cmd.args(&args[..]);
        std::fs::create_dir_all(&obj.parent().unwrap()).unwrap();

        run(cmd.arg(src.join(*file)).arg("-o").arg(&obj));
        objects.push(obj);
    }

    run(Command::new(ar()).arg("crus").arg(dst.join(output)).args(&objects[..]));

    println!("cargo:rustc-flags=-L {} -l {}:static",
             dst.display(),
             &output[3..output.len() - 2]);
}

fn run(cmd: &mut Command) {
    println!("running: {:?}", cmd);

    let status = match cmd.stdout(Stdio::inherit()).stderr(Stdio::inherit()).status() {
        Ok(status) => status,

        Err(e) => panic!("failed to spawn process: {}", e),
    };

    if !status.success() {
        panic!("nonzero exit status: {}", status);
    }
}

fn ar() -> String {
    env::var("AR").unwrap_or("ar".to_string())
}
