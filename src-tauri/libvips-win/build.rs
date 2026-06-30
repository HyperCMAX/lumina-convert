fn main() {
    println!("cargo:rustc-link-lib=vips");
    println!("cargo:rustc-link-lib=glib-2.0");
    println!("cargo:rustc-link-lib=gobject-2.0");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        generate_bindings();
    }
}

fn generate_bindings() {
    let vcpkg = std::env::var("VCPKG_ROOT")
        .or_else(|_| std::env::var("VCPKG_INSTALLATION_ROOT"))
        .unwrap_or_else(|_| "C:\\vcpkg".into());
    let inc = format!("{}\\installed\\x64-windows\\include", vcpkg);
    let glibconf = format!("{}\\installed\\x64-windows\\lib\\glib-2.0\\include", vcpkg);

    bindgen::Builder::default()
        .header("vips.h")
        .clang_arg(format!("-I{}", inc))
        .clang_arg(format!("-I{}\\glib-2.0", inc))
        .clang_arg(format!("-I{}", glibconf))
        .generate()
        .expect("bindgen failed to generate vips bindings")
        .write_to_file("src/bindings.rs")
        .expect("failed to write bindings");
}
