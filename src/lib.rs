#[cfg(feature = "parallel")]
extern crate rayon;
#[cfg(feature = "parallel")]
use rayon::prelude::*;

use std::env;
use std::process::Command;
use std::process::Stdio;
use std::path::{Path, PathBuf};

fn x86_triple(os: &str) -> &'static str {
    match os {
        "linux" => "-felf32",
        "darwin" => "-fmacho32",
        "windows" => "-fwin32",
        _ => ""
    }
}

fn x86_64_triple(os: &str) -> &'static str {
    match os {
        "linux" => "-felf64",
        "darwin" => "-fmacho64",
        "windows" => "-fwin64",
        _ => ""
    }
}

fn parse_triple(trip: &str) -> &'static str {
    let parts = trip.split('-').collect::<Vec<_>>();
    // ARCH-VENDOR-OS-ENVIRONMENT
    // or ARCH-VENDOR-OS
    // we don't care about environ so doesn't matter if triple doesn't have it
    if parts.len() < 3 {
        return ""
    }

    match parts[0] {
        "x86_64" => x86_64_triple(parts[2]),
        "x86" | "i386" | "i586" | "i686" => x86_triple(parts[2]),
        _ => ""
    }
}

/// # Example
///
/// ```no_run
/// nasm_rs::compile_library("libfoo.a", &["foo.s", "bar.s"]);
/// ```
pub fn compile_library(output: &str, files: &[&str]) {
    compile_library_args(output, files, &[]);
}

/// # Example
///
/// ```no_run
/// nasm_rs::compile_library_args("libfoo.a", &["foo.s", "bar.s"], &["-Fdwarf"]);
/// ```
pub fn compile_library_args<P: AsRef<Path>>(output: &str, files: &[P], args: &[&str]) {
    let mut b = Build::new();
    for file in files {
        b.file(file);
    }
    for arg in args {
        b.flag(arg);
    }
    b.compile(output);
}

pub struct Build {
    files: Vec<PathBuf>,
    flags: Vec<String>,
}

impl Build {
    pub fn new() -> Self {
        Self {
            files: Vec::new(),
            flags: Vec::new(),
        }
    }

    pub fn file<P: AsRef<Path>>(&mut self, p: P) -> &mut Self {
        self.files.push(p.as_ref().to_owned());
        self
    }

    pub fn flag(&mut self, flag: &str) -> &mut Self {
        self.flags.push(flag.to_owned());
        self
    }

    pub fn compile(&mut self, output: &str) {
        #[cfg(not(target_env = "msvc"))]
        assert!(output.starts_with("lib"));

        #[cfg(not(target_env = "msvc"))]
        assert!(output.ends_with(".a"));

        #[cfg(target_env = "msvc")]
        assert!(output.ends_with(".lib"));

        let target = env::var("TARGET").unwrap();

        let cargo_manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
        let out_dir = env::var("OUT_DIR").unwrap();

        let mut new_args: Vec<&str> = vec![];
        new_args.push(parse_triple(&target));

        if env::var_os("DEBUG").is_some() {
            new_args.push("-g");
        }

        for arg in &self.flags {
            new_args.push(arg);
        }

        let src = Path::new(&cargo_manifest_dir);

        let dst = Path::new(&out_dir);

        let objects = make_iter(&self.files).map(|file| {
            compile_file(file.as_ref(), &new_args, src, dst)
        }).collect::<Vec<_>>();

        run(Command::new(ar()).arg("crus").arg(dst.join(output)).args(&objects[..]));

        println!("cargo:rustc-flags=-L {}",
                 dst.display());
    }
}


#[cfg(feature = "parallel")]
fn make_iter<'a, S: AsRef<Path>>(files: &'a [S]) -> rayon::slice::Iter<'a, S> {
    files.par_iter()
}

#[cfg(not(feature = "parallel"))]
fn make_iter<'a, S: AsRef<Path>>(files: &'a [S]) -> std::slice::Iter<'a, S> {
    files.iter()
}

fn compile_file(file: &Path, new_args: &[&str], src: &Path, dst: &Path) -> PathBuf {
    let obj = dst.join(file).with_extension("o");
    let mut cmd = Command::new("nasm");
    cmd.args(&new_args[..]);
    std::fs::create_dir_all(&obj.parent().unwrap()).unwrap();

    run(cmd.arg(src.join(file)).arg("-o").arg(&obj));
    obj
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


#[test]
fn test_build() {
    let mut build = Build::new();
    build.file("test");
    build.flag("-test");
}
