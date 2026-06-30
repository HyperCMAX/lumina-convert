fn main() {
    if let Ok(lib_dir) = std::env::var("VIPS_LIB_DIR") {
        println!("cargo:rustc-link-search=native={}", lib_dir);
    } else {
        for dir in discover_lib_dirs() {
            println!("cargo:rustc-link-search=native={}", dir);
        }
    }

    generate_bindings();

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rustc-link-lib=libvips");
        println!("cargo:rustc-link-lib=libglib-2.0");
        println!("cargo:rustc-link-lib=libgobject-2.0");
    } else {
        println!("cargo:rustc-link-lib=vips");
        println!("cargo:rustc-link-lib=glib-2.0");
        println!("cargo:rustc-link-lib=gobject-2.0");
    }
}

fn discover_lib_dirs() -> Vec<String> {
    let mut dirs = Vec::new();

    for lib_dir in &[
        "/opt/homebrew/lib",
        "/usr/local/lib",
    ] {
        let vips_lib = format!("{}/libvips.dylib", lib_dir);
        if std::path::Path::new(&vips_lib).exists() {
            dirs.push(lib_dir.to_string());
            return dirs;
        }
    }

    dirs
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

    // macOS Homebrew paths
    for base in &["/opt/homebrew/include", "/usr/local/include"] {
        let vips_h = format!("{}/vips/vips.h", base);
        if std::path::Path::new(&vips_h).exists() {
            dirs.push(base.to_string());
            dirs.push(format!("{}/glib-2.0", base));
            let glib_config = format!("{}/../lib/glib-2.0/include", base);
            if std::path::Path::new(&glib_config).exists() {
                dirs.push(glib_config);
            }
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
