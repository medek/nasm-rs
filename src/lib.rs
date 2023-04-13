#[cfg(feature = "parallel")]
use rayon::prelude::*;

use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::process::Stdio;

fn x86_triple(os: &str) -> (&'static str, &'static str) {
    match os {
        "darwin" | "ios" => ("-fmacho32", "-g"),
        "windows" | "uefi" => ("-fwin32", "-g"),
        _ => ("-felf32", "-gdwarf"),
    }
}

fn x86_64_triple(os: &str) -> (&'static str, &'static str) {
    match os {
        "darwin" | "ios" => ("-fmacho64", "-g"),
        "windows" | "uefi" => ("-fwin64", "-g"),
        _ => ("-felf64", "-gdwarf"),
    }
}

fn parse_triple(trip: &str) -> (&'static str, &'static str) {
    let parts = trip.split('-').collect::<Vec<_>>();
    // ARCH-VENDOR-OS-ENVIRONMENT
    // or ARCH-VENDOR-OS
    // we don't care about environ (yes, we do... gnux32) so doesn't matter if triple doesn't have it
    if parts.len() < 3 {
        return ("", "-g");
    }

    match parts[0] {
        "x86_64" => {
            if parts.len() >= 4 && parts[3] == "gnux32" {
                ("-felfx32", "-gdwarf")
            } else {
                x86_64_triple(parts[2])
            }
        },
        "x86" | "i386" | "i586" | "i686" => x86_triple(parts[2]),
        _ => ("", "-g"),
    }
}

/// # Example
///
/// ```no_run
/// nasm_rs::compile_library("libfoo.a", &["foo.s", "bar.s"]).unwrap();
/// ```
pub fn compile_library(output: &str, files: &[&str]) -> Result<(), String> {
    compile_library_args(output, files, &[])
}

/// # Example
///
/// ```no_run
/// nasm_rs::compile_library_args("libfoo.a", &["foo.s", "bar.s"], &["-Fdwarf"]).unwrap();
/// ```
pub fn compile_library_args<P: AsRef<Path>>(
    output: &str,
    files: &[P],
    args: &[&str],
) -> Result<(), String> {
    let mut b = Build::new();
    for file in files {
        b.file(file);
    }
    for arg in args {
        b.flag(arg);
    }
    b.compile(output)
}

pub struct Build {
    files: Vec<PathBuf>,
    flags: Vec<String>,
    target: Option<String>,
    out_dir: Option<PathBuf>,
    archiver: Option<PathBuf>,
    archiver_is_msvc: Option<bool>,
    nasm: Option<PathBuf>,
    debug: bool,
    min_version: (usize, usize, usize),
}

impl Build {
    pub fn new() -> Self {
        Self {
            files: Vec::new(),
            flags: Vec::new(),
            archiver: None,
            archiver_is_msvc: None,
            out_dir: None,
            nasm: None,
            target: None,
            min_version: (1, 0, 0),
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

    /// Set multiple files
    pub fn files<P: AsRef<Path>, I: IntoIterator<Item = P>>(&mut self, files: I) -> &mut Self {
        for file in files {
            self.file(file);
        }
        self
    }

    /// Add a directory to the `-I` include path
    pub fn include<P: AsRef<Path>>(&mut self, dir: P) -> &mut Self {
        let mut flag = format!("-I{}", dir.as_ref().display());
        // nasm requires trailing slash, but `Path` may omit it.
        if !flag.ends_with('/') {
            flag += "/";
        }
        self.flags.push(flag);
        self
    }

    /// Pre-define a macro with an optional value
    pub fn define<'a, V: Into<Option<&'a str>>>(&mut self, var: &str, val: V) -> &mut Self {
        let val = val.into();
        let flag = if let Some(val) = val {
            format!("-D{}={}", var, val)
        } else {
            format!("-D{}", var)
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

    /// Configures the output directory where all object files and static libraries will be located.
    ///
    /// This option is automatically scraped from the OUT_DIR environment variable by build scripts,
    /// so it's not required to call this function.
    pub fn out_dir<P: AsRef<Path>>(&mut self, out_dir: P) -> &mut Self {
        self.out_dir = Some(out_dir.as_ref().to_owned());
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

    /// Configures the default archiver tool as well as the command syntax.
    ///
    /// This option is automatically determined from `cfg!(target_env = "msvc")`,
    /// so it's not required to call this function.
    pub fn archiver_is_msvc(&mut self, is_msvc: bool) -> &mut Self {
        self.archiver_is_msvc = Some(is_msvc);
        self
    }

    /// Configures path to `nasm` command
    pub fn nasm<P: AsRef<Path>>(&mut self, nasm: P) -> &mut Self {
        self.nasm = Some(nasm.as_ref().to_owned());
        self
    }

    /// Set the minimum version required
    pub fn min_version(&mut self, major: usize, minor: usize, micro: usize) -> &mut Self {
        self.min_version = (major, minor, micro);
        self
    }

    /// Run the compiler, generating the file output
    ///
    /// The name output should be the base name of the library,
    /// without file extension, and without "lib" prefix.
    ///
    /// The output file will have target-specific name,
    /// such as `lib*.a` (non-MSVC) or `*.lib` (MSVC).
    pub fn compile(&mut self, lib_name: &str) -> Result<(), String> {
        // Trim name for backwards comatibility
        let lib_name = if lib_name.starts_with("lib") && lib_name.ends_with(".a") {
            &lib_name[3..lib_name.len() - 2]
        } else {
            lib_name.trim_end_matches(".lib")
        };

        let target = self.get_target();
        let output = if target.ends_with("-msvc") {
            format!("{}.lib", lib_name)
        } else {
            format!("lib{}.a", lib_name)
        };

        let dst = &self.get_out_dir();
        let objects = self.compile_objects()?;
        self.archive(&dst, &output, &objects[..])?;

        println!("cargo:rustc-link-search={}", dst.display());
        Ok(())
    }

    /// Run the compiler, generating .o files
    ///
    /// The files can be linked in a separate step, e.g. passed to `cc`
    pub fn compile_objects(&mut self) -> Result<Vec<PathBuf>, String> {
        let target = self.get_target();

        let nasm = self.find_nasm()?;
        let args = self.get_args(&target);

        let src = &PathBuf::from(
            env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR must be set"),
        );
        let dst = &self.get_out_dir();

        self.make_iter()
            .map(|file| self.compile_file(&nasm, file.as_ref(), &args, src, dst))
            .collect::<Result<Vec<_>, String>>()
    }

    fn get_args(&self, target: &str) -> Vec<&str> {
        let (arch_flag, debug_flag) = parse_triple(&target);
        let mut args = vec![arch_flag];

        if self.debug {
            args.push(debug_flag);
        }

        for arg in &self.flags {
            args.push(arg);
        }

        args
    }

    #[cfg(feature = "parallel")]
    fn make_iter(&self) -> rayon::slice::Iter<PathBuf> {
        self.files.par_iter()
    }

    #[cfg(not(feature = "parallel"))]
    fn make_iter(&self) -> std::slice::Iter<'_, PathBuf> {
        self.files.iter()
    }

    fn compile_file(
        &self,
        nasm: &Path,
        file: &Path,
        new_args: &[&str],
        src: &Path,
        dst: &Path,
    ) -> Result<PathBuf, String> {
        let obj = dst.join(file.file_name().unwrap()).with_extension("o");
        let mut cmd = Command::new(nasm);
        cmd.args(&new_args[..]);
        std::fs::create_dir_all(&obj.parent().unwrap()).unwrap();

        run(cmd.arg(src.join(file)).arg("-o").arg(&obj))?;
        Ok(obj)
    }

    fn archive(&self, out_dir: &Path, lib: &str, objs: &[PathBuf]) -> Result<(), String> {
        let ar_is_msvc = self.archiver_is_msvc.unwrap_or(cfg!(target_env = "msvc"));

        let ar = if ar_is_msvc {
            self.archiver.clone().unwrap_or_else(|| "lib".into())
        } else {
            self.archiver
                .clone()
                .or_else(|| env::var_os("AR").map(|a| a.into()))
                .unwrap_or_else(|| "ar".into())
        };
        if ar_is_msvc {
            let mut out_param = OsString::new();
            out_param.push("/OUT:");
            out_param.push(out_dir.join(lib).as_os_str());
            run(Command::new(ar).arg(out_param).args(objs))
        } else {
            run(Command::new(ar)
                .arg("crus")
                .arg(out_dir.join(lib))
                .args(objs))
        }
    }

    fn get_out_dir(&self) -> PathBuf {
        self.out_dir
            .clone()
            .unwrap_or_else(|| PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR must be set")))
    }

    fn get_target(&self) -> String {
        self.target
            .clone()
            .unwrap_or_else(|| env::var("TARGET").expect("TARGET must be set"))
    }

    /// Returns version string if nasm is too old,
    /// or error message string if it's unusable.
    fn is_nasm_found_and_new_enough(&self, nasm_path: &Path) -> Result<(), String> {
        let version = get_output(Command::new(nasm_path).arg("-v"))
            .map_err(|e| format!("Unable to run {}: {}", nasm_path.display(), e))?;
        let (major, minor, micro) = self.min_version;
        let ver = parse_nasm_version(&version)?;
        if major > ver.0
            || (major == ver.0 && minor > ver.1)
            || (major == ver.0 && minor == ver.1 && micro > ver.2)
        {
            Err(format!(
                "This version of NASM is too old: {}. Required >= {}.{}.{}",
                version, major, minor, micro
            ))
        } else {
            Ok(())
        }
    }

    fn find_nasm(&mut self) -> Result<PathBuf, String> {
        let paths = match &self.nasm {
            Some(p) => vec![p.to_owned()],
            None => {
                // Xcode has an outdated verison of nasm,
                // and puts its own SDK first in the PATH.
                // The proper Homebrew nasm is later in the PATH.
                let path = env::var_os("PATH").unwrap_or_default();
                std::iter::once(PathBuf::from("nasm"))
                    .chain(env::split_paths(&path).map(|p| p.join("nasm")))
                    .collect()
            }
        };

        let mut first_error = None;
        for nasm_path in paths {
            match self.is_nasm_found_and_new_enough(&nasm_path) {
                Ok(_) => return Ok(nasm_path),
                Err(err) => {
                    let _ = first_error.get_or_insert(err);
                }
            }
        }
        Err(first_error.unwrap())
    }
}

fn parse_nasm_version(version: &str) -> Result<(usize, usize, usize), String> {
    let mut ver = version
        .split(' ')
        .nth(2)
        .ok_or_else(|| format!("Invalid nasm version '{}'", version))?;

    //this will probably break at some point...
    if let Some(ver_rc) = ver.find("rc") {
        ver = &ver[0..ver_rc];
    }
    let ver: Vec<_> = ver
        .split('.')
        .map(|v| v.parse())
        .take_while(Result::is_ok)
        .map(Result::unwrap)
        .collect();

    Ok((
        ver[0],
        ver.get(1).copied().unwrap_or(0),
        ver.get(2).copied().unwrap_or(0),
    ))
}

fn get_output(cmd: &mut Command) -> Result<String, String> {
    let out = cmd.output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).to_string())
    }
}

fn run(cmd: &mut Command) -> Result<(), String> {
    println!("running: {:?}", cmd);

    let status = match cmd
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
    {
        Ok(status) => status,

        Err(e) => return Err(format!("failed to spawn process: {}", e)),
    };

    if !status.success() {
        return Err(format!("nonzero exit status: {}", status));
    }
    Ok(())
}

#[test]
fn test_build() {
    let mut build = Build::new();
    build.file("test");
    build.archiver("ar");
    build.include("./");
    build.include("dir");
    build.define("foo", Some("1"));
    build.define("bar", None);
    build.flag("-test");
    build.target("i686-unknown-linux-musl");
    build.out_dir("/tmp");
    build.min_version(0, 0, 0);

    assert_eq!(
        build.get_args("i686-unknown-linux-musl"),
        &["-felf32", "-I./", "-Idir/", "-Dfoo=1", "-Dbar", "-test"]
    );
}

#[test]
fn test_parse_nasm_version() {
    let ver_str = "NASM version 2.14.02 compiled on Jan 22 2019";
    assert_eq!((2, 14, 2), parse_nasm_version(ver_str).unwrap());
    let ver_str = "NASM version 2.14.02";
    assert_eq!((2, 14, 2), parse_nasm_version(ver_str).unwrap());
    let ver_str = "NASM version 2.14 compiled on Jan 22 2019";
    assert_eq!((2, 14, 0), parse_nasm_version(ver_str).unwrap());
    let ver_str = "NASM version 2.14";
    assert_eq!((2, 14, 0), parse_nasm_version(ver_str).unwrap());
    let ver_str = "NASM version 2.14rc2";
    assert_eq!((2, 14, 0), parse_nasm_version(ver_str).unwrap());
}

#[test]
fn test_parse_triple() {
    let triple = "x86_64-unknown-linux-gnux32";
    assert_eq!(parse_triple(&triple), ("-felfx32", "-gdwarf"));
    
    let triple = "x86_64-unknown-linux";
    assert_eq!(parse_triple(&triple), ("-felf64", "-gdwarf"));
}

