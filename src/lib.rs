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
    target: Option<String>,
    archiver: Option<PathBuf>,
    debug: bool,
}

impl Build {
    pub fn new() -> Self {
        Self {
            files: Vec::new(),
            flags: Vec::new(),
            archiver: None,
            target: None,
            debug: env::var("DEBUG").ok().map_or(false, |d| d != "false"),
        }
    }

    /// Add a file which will be compiled
    ///
    /// e.g. `"foo.s"`
    pub fn file<P: AsRef<Path>>(&mut self, p: P) -> &mut Self {
        self.files.push(p.as_ref().to_owned());
        self
    }

    /// Add a directory to the `-I` include path
    pub fn include<P: AsRef<Path>>(&mut self, dir: P) -> &mut Self {
        self.flags.push(format!("-I{}", dir.as_ref().display()));
        self
    }

    /// Pre-define a macro with an optional value
    pub fn define<'a, V: Into<Option<&'a str>>>(&mut self, var: &str, val: V) -> &mut Self {
        let val = val.into();
        let flag = if let Some(val) = val {
            format!("{}={}", var, val)
        } else {
            var.to_owned()
        };
        self.flags.push(flag);
        self
    }

    /// Configures whether the assembler will generate debug information.
    ///
    /// This option is automatically scraped from the `DEBUG` environment
    /// variable by build scripts (only enabled when the profile is "debug"), so
    /// it's not required to call this function.
    pub fn debug(&mut self, enable: bool) -> &mut Self {
        self.debug = enable;
        self
    }

    /// Add an arbitrary flag to the invocation of the assembler
    ///
    /// e.g. `"-Fdwarf"`
    pub fn flag(&mut self, flag: &str) -> &mut Self {
        self.flags.push(flag.to_owned());
        self
    }

    /// Configures the target this configuration will be compiling for.
    ///
    /// This option is automatically scraped from the `TARGET` environment
    /// variable by build scripts, so it's not required to call this function.
    pub fn target(&mut self, target: &str) -> &mut Self {
        self.target = Some(target.to_owned());
        self
    }

    /// Configures the tool used to assemble archives.
    ///
    /// This option is automatically determined from the target platform or a
    /// number of environment variables, so it's not required to call this
    /// function.
    pub fn archiver<P: AsRef<Path>>(&mut self, archiver: P) -> &mut Self {
        self.archiver = Some(archiver.as_ref().to_owned());
        self
    }

    /// Run the compiler, generating the file output
    ///
    /// The name output should be the name of the library
    /// including platform-specific prefix and file extension,
    /// e.g. `"libfoo.a"`
    pub fn compile(&mut self, output: &str) {
        #[cfg(not(target_env = "msvc"))]
        assert!(output.starts_with("lib"));

        #[cfg(not(target_env = "msvc"))]
        assert!(output.ends_with(".a"));

        #[cfg(target_env = "msvc")]
        assert!(output.ends_with(".lib"));

        let target = self.target.clone()
            .unwrap_or_else(|| env::var("TARGET").expect("TARGET must be set"));

        let mut new_args: Vec<&str> = vec![];
        new_args.push(parse_triple(&target));

        if self.debug {
            new_args.push("-g");
        }

        for arg in &self.flags {
            new_args.push(arg);
        }

        let src = &PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR must be set"));
        let dst = &PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR must be set"));


        let objects = self.make_iter().map(|file| {
            self.compile_file(file.as_ref(), &new_args, src, dst)
        }).collect::<Vec<_>>();

        run(Command::new(self.ar()).arg("crus").arg(dst.join(output)).args(&objects[..]));

        println!("cargo:rustc-flags=-L {}",
                 dst.display());
    }

    #[cfg(feature = "parallel")]
    fn make_iter(&self) -> rayon::slice::Iter<PathBuf> {
        self.files.par_iter()
    }

    #[cfg(not(feature = "parallel"))]
    fn make_iter(&self) -> std::slice::Iter<PathBuf> {
        self.files.iter()
    }

    fn compile_file(&self, file: &Path, new_args: &[&str], src: &Path, dst: &Path) -> PathBuf {
        let obj = dst.join(file).with_extension("o");
        let mut cmd = Command::new("nasm");
        cmd.args(&new_args[..]);
        std::fs::create_dir_all(&obj.parent().unwrap()).unwrap();

        run(cmd.arg(src.join(file)).arg("-o").arg(&obj));
        obj
    }

    fn ar(&self) -> PathBuf {
        self.archiver.clone()
            .or_else(|| env::var_os("AR").map(|a| a.into()))
            .unwrap_or_else(|| "ar".into())
    }
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

#[test]
fn test_build() {
    let mut build = Build::new();
    build.file("test");
    build.archiver("ar");
    build.include("./");
    build.define("foo", Some("1"));
    build.flag("-test");
}
