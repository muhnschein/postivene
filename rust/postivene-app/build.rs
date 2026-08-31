//! Export `main()` as a dynamic symbol.
//!
//! Rust emits `main` as an ordinary global symbol, which lives only in
//! `.symtab` and is gone once rpmbuild strips the binary. The
//! `silica-qt5` booster looks it up in `.dynsym`, and Harbour's validator
//! rejects a Silica app where it is missing (`ci/harbour-check.sh`, check
//! 1.7.3).
//!
//! `--dynamic-list` rather than `--export-dynamic-symbol`, which needs
//! binutils 2.35, or `--export-dynamic`, which would export every symbol
//! in the binary.

fn main() {
    let list = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("main.dynlist");
    println!("cargo:rerun-if-changed={}", list.display());
    println!(
        "cargo:rustc-link-arg-bins=-Wl,--dynamic-list={}",
        list.display()
    );

    // qttypes links Qt5Widgets unconditionally, with no feature to turn it
    // off. Once the vendored qmetaobject stops using QApplication nothing
    // refers to it, and --as-needed drops the DT_NEEDED entry that would
    // otherwise fail Harbour's allowed-libraries check. It prunes only
    // libraries no symbol needs, so it is right for the others too.
    println!("cargo:rustc-link-arg-bins=-Wl,--as-needed");
}
