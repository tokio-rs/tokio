use std::{env, ffi::OsString, process::Command};

fn main() {
    match rustc_minor_version() {
        // If rustc >= 1.47.0, we can enable `track_caller`.
        Ok(minor) if minor >= 47 => println!("cargo:rustc-cfg=tokio_track_caller"),
        Err(e) => println!("cargo:warning=could not parse rustc version: {}", e),
        _ => {}
    }
}

fn rustc_minor_version() -> Result<usize, Box<dyn std::error::Error>> {
    let rustc = env::var_os("RUSTC").unwrap_or_else(|| OsString::from("rustc"));
    let out = Command::new(rustc).arg("-V").output()?;
    let version_str = std::str::from_utf8(&out.stdout)?;
    let mut parts = version_str.split(' ');
    if parts.next() != Some("rustc") {
        return Err(format!("weird rustc version: {:?} (missing 'rustc') ", version_str).into());
    }
    if let Some(part) = parts.next() {
        let mut parts = part.split('.');
        if parts.next() != Some("1") {
            Err(format!(
                "weird rustc version: {:?} (does not start with 1)",
                version_str
            ))?;
        }
        if let Some(middle) = parts.next() {
            Ok(middle.parse()?)
        } else {
            Err(format!("weird rustc version: {:?} (no minor version)", version_str).into())
        }
    } else {
        Err(format!(
            "weird rustc version: {:?} (missing version entirely?) ",
            version_str,
        )
        .into())
    }
}
