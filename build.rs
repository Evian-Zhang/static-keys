fn main() {
    // See https://lld.llvm.org/ELF/start-stop-gc.html and https://doc.rust-lang.org/cargo/reference/build-scripts.html#rustc-link-arg
    println!("cargo::rustc-link-arg-examples=-z");
    println!("cargo::rustc-link-arg-examples=nostart-stop-gc");
}
