use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=assets/brand/yujie-logo.ico");
    println!("cargo:rerun-if-changed=build.rs");

    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let sdk_arch = match target_arch.as_str() {
        "x86_64" => "x64",
        "x86" => "x86",
        "aarch64" => "arm64",
        other => panic!("unsupported Windows target architecture: {other}"),
    };
    let rc_exe = find_resource_compiler(sdk_arch)
        .unwrap_or_else(|| panic!("Windows SDK resource compiler (rc.exe) was not found"));

    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let source_icon = manifest_dir.join("assets/brand/yujie-logo.ico");
    let build_icon = out_dir.join("yujie-logo.ico");
    let rc_path = out_dir.join("yujie-reader.rc");
    let res_path = out_dir.join("yujie-reader.res");

    fs::copy(&source_icon, &build_icon).expect("failed to copy the application icon");
    fs::write(&rc_path, "1 ICON \"yujie-logo.ico\"\r\n")
        .expect("failed to write the Windows resource script");

    let output = Command::new(&rc_exe)
        .current_dir(&out_dir)
        .arg("/nologo")
        .arg("/fo")
        .arg(&res_path)
        .arg(&rc_path)
        .output()
        .expect("failed to run the Windows resource compiler");
    if !output.status.success() {
        panic!(
            "Windows resource compilation failed:\n{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    println!(
        "cargo:rustc-link-arg-bin=yujie-reader={}",
        res_path.display()
    );
}

fn find_resource_compiler(arch: &str) -> Option<PathBuf> {
    if let Some(path) = env::var_os("RC") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Some(path);
        }
    }

    let mut roots = Vec::new();
    if let Some(program_files_x86) = env::var_os("ProgramFiles(x86)") {
        roots.push(PathBuf::from(program_files_x86).join("Windows Kits/10/bin"));
    }
    roots.push(PathBuf::from(r"C:\Program Files (x86)\Windows Kits\10\bin"));

    let mut candidates = Vec::new();
    for root in roots {
        collect_rc_candidates(&root, arch, &mut candidates);
    }
    candidates.sort();
    candidates.pop()
}

fn collect_rc_candidates(root: &Path, arch: &str, candidates: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let candidate = entry.path().join(arch).join("rc.exe");
        if candidate.is_file() {
            candidates.push(candidate);
        }
    }
}
