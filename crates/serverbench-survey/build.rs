fn main() {
    let rustc = std::env::var("RUSTC").expect("Cargo provides RUSTC");
    let output = std::process::Command::new(rustc)
        .arg("--version")
        .output()
        .expect("rustc version");
    println!(
        "cargo:rustc-env=SERVERBENCH_COMPILER={}",
        String::from_utf8_lossy(&output.stdout).trim()
    );
    println!(
        "cargo:rustc-env=SERVERBENCH_TARGET={}",
        std::env::var("TARGET").expect("Cargo target")
    );
    println!("cargo:rerun-if-changed=build.rs");
}
