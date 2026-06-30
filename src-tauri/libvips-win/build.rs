fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        if let Ok(lib_dir) = std::env::var("VIPS_LIB_DIR") {
            println!("cargo:rustc-link-search=native={}", lib_dir);
        }
        generate_bindings();
    }
    println!("cargo:rustc-link-lib=vips");
    println!("cargo:rustc-link-lib=glib-2.0");
    println!("cargo:rustc-link-lib=gobject-2.0");
}

fn generate_bindings() {
    let include_dirs = discover_include_dirs();

    eprintln!("libvips-win build.rs: include_dirs = {:?}", include_dirs);

    let mut builder = bindgen::Builder::default()
        .header("vips.h");

    for dir in &include_dirs {
        builder = builder.clang_arg(format!("-I{}", dir));
    }

    builder
        .generate()
        .expect("bindgen failed to generate vips bindings")
        .write_to_file("src/bindings.rs")
        .expect("failed to write bindings");
}

fn discover_include_dirs() -> Vec<String> {
    let mut dirs = Vec::new();

    if let Ok(prefix) = std::env::var("VIPS_INCLUDE_DIR") {
        if std::path::Path::new(&prefix).exists() {
            dirs.push(prefix.clone());
            dirs.push(format!("{}/glib-2.0", prefix));
            dirs.push(format!("{}/../lib/glib-2.0/include", prefix));
            return dirs;
        }
    }

    if let Ok(vcpkg) = std::env::var("VCPKG_ROOT")
        .or_else(|_| std::env::var("VCPKG_INSTALLATION_ROOT"))
    {
        let base = format!("{}\\installed\\x64-windows\\include", vcpkg);
        if std::path::Path::new(&base).exists() {
            dirs.push(base.clone());
            dirs.push(format!("{}\\glib-2.0", base));
            dirs.push(format!("{}\\..\\lib\\glib-2.0\\include", base));
            return dirs;
        }
    }

    for base in &[
        "C:\\Program Files\\libvips\\include",
        "C:\\vips\\include",
        "C:\\vcpkg\\installed\\x64-windows\\include",
    ] {
        if std::path::Path::new(base).exists() {
            dirs.push(base.to_string());
            dirs.push(format!("{}\\glib-2.0", base));
            dirs.push(format!("{}\\..\\lib\\glib-2.0\\include", base));
            return dirs;
        }
    }

    dirs
}
