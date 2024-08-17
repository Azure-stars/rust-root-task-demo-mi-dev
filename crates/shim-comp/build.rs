fn main() {
    let linker_path = "crates/shim-comp/linker.ld";
    println!("cargo:rerun-if-changed={linker_path}");
    println!("cargo:rustc-link-arg=-T{linker_path}");
}
