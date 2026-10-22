fn main() {
    let src_dir = std::path::Path::new("src");

    // Upstream's own build.rs compiles scanner.c as C++; it's plain C, so
    // build both files as C.
    cc::Build::new()
        .std("c11")
        .include(src_dir)
        .flag_if_supported("-Wno-unused-parameter")
        .flag_if_supported("-Wno-unused-but-set-variable")
        .flag_if_supported("-Wno-trigraphs")
        .file(src_dir.join("parser.c"))
        .file(src_dir.join("scanner.c"))
        .compile("tree-sitter-talon");

    println!("cargo:rerun-if-changed=src");
}
