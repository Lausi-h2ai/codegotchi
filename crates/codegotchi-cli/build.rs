use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let web_dist = manifest_dir.join("web-dist");
    println!("cargo:rerun-if-changed={}", web_dist.display());

    let mut files = Vec::new();
    collect_files(&web_dist, &mut files).expect("walk web-dist");
    files.sort();

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("build output dir"));
    let generated = out_dir.join("web_assets.rs");
    let mut source = String::from("pub static EMBEDDED_ASSETS: &[EmbeddedAsset] = &[\n");
    for path in files {
        let relative = path
            .strip_prefix(&web_dist)
            .expect("asset is under web-dist")
            .to_string_lossy()
            .replace(std::path::MAIN_SEPARATOR, "/");
        let route = format!("/{relative}");
        let mime = mime_type(&path);
        let absolute = path.to_string_lossy();
        writeln!(
            source,
            "    EmbeddedAsset {{ path: {route:?}, content_type: {mime:?}, bytes: include_bytes!({absolute:?}) }},"
        )
        .expect("generated asset table writes");
        println!("cargo:rerun-if-changed={}", path.display());
    }
    source.push_str("];\n");
    fs::write(generated, source).expect("write generated asset table");
}

fn collect_files(directory: &Path, files: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, files)?;
        } else if path.is_file() {
            files.push(path);
        }
    }
    Ok(())
}

fn mime_type(path: &Path) -> &'static str {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("js") | Some("mjs") => "application/javascript; charset=utf-8",
        Some("json") => "application/json; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("ico") => "image/x-icon",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        _ => "application/octet-stream",
    }
}
