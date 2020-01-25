extern crate pkg_config;
use std::process::Command;
use std::path::Path;

fn main() {
    if pkg_config::find_library("opusfile").is_ok() { return }

    let out_dir = std::env::var("OUT_DIR").unwrap();
    let out_dir = Path::new(&out_dir);
    let static_lib_path = out_dir.join("lib/libopusfile.a");

    if std::fs::metadata(static_lib_path).is_err() {
        build(&out_dir);
    }

    println!("cargo:root={}", out_dir.display());
    inform_cargo(out_dir);
}

#[cfg(windows)]
fn build(out_dir: &Path) {
    std::env::set_current_dir("libopusfile").unwrap_or_else(|e| panic!("{}", e));

    success_or_panic(Command::new("sh")
        .args(&["./configure",
                "--disable-shared", "--enable-static",
                "--disable-doc",
                "--disable-extra-programs",
                "--with-pic",
                "--prefix", &out_dir.to_str().unwrap().replace("\\", "/")]));
    success_or_panic(&mut Command::new("make"));
    success_or_panic(&mut Command::new("make").arg("install"));

    std::env::set_current_dir("..").unwrap_or_else(|e| panic!("{}", e));
}

#[cfg(all(unix, target_os = "windows"))]
fn build(out_dir: &Path) {
    std::env::set_current_dir("libopusfile").unwrap_or_else(|e| panic!("{}", e));

    success_or_panic(Command::new("./configure")
        .args(&["--disable-shared", "--enable-static",
                "--disable-doc",
                "--disable-extra-programs",
                "--with-pic",
                "--prefix", out_dir.to_str().unwrap()]));
    success_or_panic(&mut Command::new("make"));
    success_or_panic(&mut Command::new("make").arg("install"));

    std::env::set_current_dir("..").unwrap_or_else(|e| panic!("{}", e));
}

#[cfg(all(unix, not(target_os = "windows")))]
fn build(out_dir: &Path) {
    std::env::set_current_dir("libopusfile").unwrap_or_else(|e| panic!("{}", e));

    success_or_panic(Command::new("./configure")
        .args(&["--disable-shared", "--enable-static",
                "--disable-doc",
                "--disable-extra-programs",
                "--with-pic",
                "--prefix", out_dir.to_str().unwrap()]));
    success_or_panic(&mut Command::new("make"));
    success_or_panic(&mut Command::new("make").arg("install"));

    std::env::set_current_dir("..").unwrap_or_else(|e| panic!("{}", e));
}

#[cfg(any(windows, all(unix, not(target_os = "linux"))))]
fn inform_cargo(out_dir: &Path) {
    let out_str = out_dir.to_str().unwrap();
    println!("cargo:rustc-flags=-L native={}/lib -l static=opusfile", out_str);
}

#[cfg(target_os = "linux")]
fn inform_cargo(out_dir: &Path) {
    let opusfile_pc = out_dir.join("lib/pkgconfig/opusfile.pc");
    let opusfile_pc = opusfile_pc.to_str().unwrap();
    pkg_config::Config::new().statik(true).find(opusfile_pc).unwrap();
}

fn success_or_panic(cmd: &mut Command) {
    match cmd.output() {
        Ok(output) => if !output.status.success() {
            panic!("command exited with failure\n=== Stdout ===\n{}\n=== Stderr ===\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr))
        },
        Err(e)     => panic!("{}", e),
    }
}
