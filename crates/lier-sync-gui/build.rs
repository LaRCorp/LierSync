// LierSync — Copyright (C) 2026 LaRevole (LaRCorp) — GPL-3.0-only

fn main() {
    println!("cargo:rerun-if-changed=assets/app.rc");
    println!("cargo:rerun-if-changed=assets/icon.ico");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let Some(rc_exe) = find_rc_exe() else {
        println!("cargo:warning=rc.exe not found; skipping Windows exe icon resource");
        return;
    };

    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let res_file = out_dir.join("liersync_app.res");
    let rc_file = std::path::PathBuf::from("assets/app.rc");

    let status = std::process::Command::new(&rc_exe)
        .args(["/nologo", "/fo"])
        .arg(&res_file)
        .arg(&rc_file)
        .status();

    match status {
        Ok(s) if s.success() => {
            println!("cargo:rustc-link-arg-bin=lier-sync-gui={}", res_file.display());
        }
        other => {
            println!("cargo:warning=rc.exe failed ({other:?}); continuing without exe resource");
        }
    }
}

fn find_rc_exe() -> Option<std::path::PathBuf> {
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let cand = dir.join("rc.exe");
            if cand.exists() {
                return Some(cand);
            }
        }
    }

    let kits_bin = std::path::Path::new(r"C:\Program Files (x86)\Windows Kits\10\bin");
    if let Ok(entries) = std::fs::read_dir(kits_bin) {
        let mut versions: Vec<_> = entries
            .filter_map(|e| e.ok())
            .map(|e| e.path().join("x64").join("rc.exe"))
            .filter(|p| p.exists())
            .collect();
        versions.sort();
        if let Some(latest) = versions.pop() {
            return Some(latest);
        }
    }
    None
}
