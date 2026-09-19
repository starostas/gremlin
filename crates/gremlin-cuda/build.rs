use std::{env, path::PathBuf, process::Command};
fn main() {
    println!("cargo:rerun-if-changed=src/evaluator.cu");
    println!("cargo:rerun-if-env-changed=CUDA_HOME");
    if env::var_os("CARGO_FEATURE_CUDA").is_none() {
        return;
    }
    let home = PathBuf::from(env::var_os("CUDA_HOME").unwrap_or_else(|| "/usr/local/cuda".into()));
    let nvcc = home.join("bin/nvcc");
    let version = Command::new(&nvcc)
        .arg("--version")
        .output()
        .expect("CUDA feature requires NVCC under CUDA_HOME");
    assert!(version.status.success(), "nvcc --version failed");
    println!(
        "cargo:rustc-env=GREMLIN_NVCC_VERSION={}",
        String::from_utf8_lossy(&version.stdout).replace('\n', " ")
    );
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let object = out.join("evaluator.o");
    assert!(
        Command::new(nvcc)
            .args([
                "-std=c++17",
                "-O3",
                "-Xcompiler",
                "-fPIC",
                "-gencode",
                "arch=compute_75,code=compute_75",
                "-c",
                "src/evaluator.cu",
                "-o"
            ])
            .arg(&object)
            .status()
            .expect("run NVCC")
            .success(),
        "CUDA compilation failed"
    );
    assert!(Command::new("ar")
        .arg("crs")
        .arg(out.join("libgremlin_cuda_kernel.a"))
        .arg(object)
        .status()
        .expect("run ar")
        .success());
    println!("cargo:rustc-link-search=native={}", out.display());
    println!(
        "cargo:rustc-link-search=native={}",
        home.join("lib64").display()
    );
    for lib in [
        "static=gremlin_cuda_kernel",
        "static=cudart_static",
        "stdc++",
        "dl",
        "rt",
        "pthread",
    ] {
        println!("cargo:rustc-link-lib={lib}");
    }
}
