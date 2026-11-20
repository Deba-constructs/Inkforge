//! Build script: gives the Windows executable its INKFORGE icon and version
//! metadata.
//!
//! `packaging/windows/inkforge.rc.in` is filled in with the icon's path and the
//! crate version (so the version Explorer shows can never drift from
//! `Cargo.toml`) and compiled to a COFF object, which is then linked in.
//!
//! Whichever resource compiler the machine has is used: `rc.exe` from the
//! Windows SDK (MSVC builds on Windows), `windres` (MinGW), `llvm-rc`, or
//! `zig rc`, which is how `scripts/package-windows-cross.sh` builds from Linux.
//! Every other target skips this entirely.

use std::path::{Path, PathBuf};
use std::process::Command;

/// How a resource compiler wants its arguments.
#[derive(Clone, Copy, PartialEq)]
enum Style {
    /// `rc.exe` / `llvm-rc`: `/fo out.res in.rc`
    Msvc,
    /// `windres`: `-i in.rc -o out.o`
    Windres,
}

const RC_TEMPLATE: &str = "packaging/windows/inkforge.rc.in";
const ICON: &str = "packaging/windows/inkforge.ico";

fn main() {
    println!("cargo:rerun-if-changed={RC_TEMPLATE}");
    println!("cargo:rerun-if-changed={ICON}");
    println!("cargo:rerun-if-env-changed=RC");
    println!("cargo:rerun-if-env-changed=WINDRES");

    // Only the Windows executable carries Windows resources.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let version = std::env::var("CARGO_PKG_VERSION").expect("CARGO_PKG_VERSION");

    let icon = manifest_dir.join(ICON);
    if !icon.is_file() {
        panic!("{} is missing: run `python3 scripts/make-icon.py`", icon.display());
    }

    let rc_path = out_dir.join("inkforge.rc");
    let template = std::fs::read_to_string(manifest_dir.join(RC_TEMPLATE))
        .unwrap_or_else(|e| panic!("could not read {RC_TEMPLATE}: {e}"));
    let rc_text = template
        .replace("@ICON@", &icon.to_string_lossy().replace('\\', "/"))
        .replace("@VERSION_COMMA@", &version_as_quad(&version))
        .replace("@VERSION@", &version);
    std::fs::write(&rc_path, rc_text).expect("could not write the generated .rc");

    let object = out_dir.join("inkforge.res");
    compile(&rc_path, &object);
    // link.exe and ld both accept the compiled resource object as-is. Passed as
    // a plain argument so no `-l` name mangling is applied to the path.
    println!("cargo:rustc-link-arg={}", object.display());
}

/// `1.2.3` -> `1,2,3,0`, as the VERSIONINFO statement wants it.
fn version_as_quad(version: &str) -> String {
    let core = version.split(['-', '+']).next().unwrap_or(version);
    let mut parts: Vec<u32> = core
        .split('.')
        .map(|part| part.trim().parse::<u32>().unwrap_or(0))
        .collect();
    parts.resize(4, 0);
    parts[..4]
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

fn compile(rc: &Path, object: &Path) {
    let msvc_target = std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");
    let mut attempts: Vec<(String, Style)> = Vec::new();

    // An explicit override wins, so a build machine with an unusual layout can
    // point at its own compiler.
    if let Ok(rc_env) = std::env::var("RC") {
        if !rc_env.is_empty() {
            let style = if rc_env.to_lowercase().contains("windres") {
                Style::Windres
            } else {
                Style::Msvc
            };
            attempts.push((rc_env, style));
        }
    }
    if let Ok(windres) = std::env::var("WINDRES") {
        if !windres.is_empty() {
            attempts.push((windres, Style::Windres));
        }
    }

    if msvc_target {
        attempts.extend(sdk_rc_candidates());
        attempts.push(("rc".into(), Style::Msvc));
        attempts.push(("rc.exe".into(), Style::Msvc));
        attempts.push(("llvm-rc".into(), Style::Msvc));
    } else {
        attempts.push(("windres".into(), Style::Windres));
        attempts.push(("x86_64-w64-mingw32-windres".into(), Style::Windres));
        attempts.push(("llvm-rc".into(), Style::Msvc));
    }
    // `zig rc` is a drop-in rc.exe and is what the Linux cross-build uses.
    attempts.push(("zig rc".into(), Style::Msvc));

    // Run from the output directory and pass bare file names: a path that
    // starts with `/` looks like a switch to `zig rc`, and absolute paths are
    // not worth special-casing for every compiler.
    let work_dir = object.parent().filter(|dir| !dir.as_os_str().is_empty());
    let rc_name = rc.file_name().expect("resource script file name");
    let object_name = object.file_name().expect("resource object file name");

    let mut errors = Vec::new();
    for (program, style) in &attempts {
        let mut command = {
            let mut words = program.split_whitespace();
            let exe = words.next().unwrap_or_default();
            let mut cmd = Command::new(exe);
            cmd.args(words);
            cmd
        };
        if let Some(dir) = work_dir {
            command.current_dir(dir);
        }
        match style {
            Style::Msvc => {
                command.arg("/nologo").arg("/fo").arg(object_name).arg(rc_name);
            }
            Style::Windres => {
                command
                    .arg("--input")
                    .arg(rc_name)
                    .arg("--output")
                    .arg(object_name)
                    .arg("--output-format=coff");
            }
        }
        match command.output() {
            Ok(output) if output.status.success() => {
                if !object.is_file() {
                    errors.push(format!("{program}: reported success but wrote no object"));
                    continue;
                }
                return;
            }
            Ok(output) => errors.push(format!(
                "{program}: {}\n{}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            )),
            Err(error) => errors.push(format!("{program}: {error}")),
        }
    }

    panic!(
        "could not compile {}\nno working resource compiler was found; \
         tried:\n  {}\ninstall the Windows SDK (rc.exe), MinGW's windres, or zig, \
         or point RC/WINDRES at the compiler to use",
        rc.display(),
        errors.join("\n  ")
    );
}

/// `rc.exe` from the Windows 10/11 SDK, which is installed even when the
/// Developer Prompt (and so PATH) is not in use. Newest SDK wins.
fn sdk_rc_candidates() -> Vec<(String, Style)> {
    let mut found = Vec::new();
    for variable in ["ProgramFiles(x86)", "ProgramFiles"] {
        let Ok(root) = std::env::var(variable) else {
            continue;
        };
        let bin = Path::new(&root).join("Windows Kits/10/bin");
        let Ok(entries) = std::fs::read_dir(&bin) else {
            continue;
        };
        let mut versions: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
        versions.sort();
        versions.reverse();
        for version in versions {
            let rc = version.join("x64/rc.exe");
            if rc.is_file() {
                found.push((rc.to_string_lossy().into_owned(), Style::Msvc));
            }
        }
    }
    found
}
