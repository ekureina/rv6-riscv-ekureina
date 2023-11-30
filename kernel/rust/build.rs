use std::{
    env,
    path::{Path, PathBuf},
};
static CBINDGEN_AFTER_INCLUDES: &str = "typedef _Bool bool;
typedef unsigned int uint32_t;
typedef unsigned short uint16_t;
typedef unsigned char uint8_t;
typedef char int8_t;
typedef void* uintptr_t;";

fn main() {
    let target = env::var("TARGET").unwrap();
    assert!(target == "riscv64gc-unknown-none-elf",
        "Only able to compile this kernel for target 'riscv64gc-unknown-none-elf', attempted to build for '{target}'");

    let current_path = env::current_dir().unwrap();
    let kernel_path = current_path
        .ancestors()
        .find(|path| {
            let potential_kernel_path = path.join("kernel");
            potential_kernel_path.exists() && potential_kernel_path.is_dir()
        })
        .unwrap()
        .join("kernel");
    println!(
        "cargo:rerun-if-changed={}",
        kernel_path.join("rust").to_string_lossy()
    );
    bindgen(&kernel_path);
    cbindgen(&kernel_path);
}

fn bindgen(kernel_path: &Path) {
    let mut kernel_headers = indexmap::IndexSet::new();
    kernel_headers.insert(kernel_path.join("types.h").to_string_lossy().into_owned());
    kernel_headers.insert(
        kernel_path
            .join("spinlock.h")
            .to_string_lossy()
            .into_owned(),
    );
    kernel_headers.insert(
        kernel_path
            .join("sleeplock.h")
            .to_string_lossy()
            .into_owned(),
    );
    kernel_headers.insert(kernel_path.join("fs.h").to_string_lossy().into_owned());
    kernel_headers.insert(kernel_path.join("riscv.h").to_string_lossy().into_owned());
    for kernel_file in kernel_path
        .read_dir()
        .unwrap_or_else(|_| panic!("unable to view kernel directory: {kernel_path:?}"))
        .flatten()
    {
        let kernel_file_path = kernel_file.path().to_string_lossy().into_owned();
        if kernel_file_path.contains(".h") && kernel_file_path != "rust.h" {
            kernel_headers.insert(kernel_file_path.clone());
        }
    }

    let bindings = kernel_headers
        .iter()
        .fold(bindgen::Builder::default(), |builder, kernel_header| {
            builder.header(kernel_header)
        })
        .use_core()
        .generate_cstr(true)
        .default_enum_style(bindgen::EnumVariation::Rust {
            non_exhaustive: false,
        })
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .expect("unable to generate kernel bindings");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("kernel_bindings.rs"))
        .expect("Couldn't write kernel bindings!");
}

fn cbindgen(kernel_path: &Path) {
    let crate_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    cbindgen::Builder::new()
        .with_crate(crate_dir)
        .with_documentation(true)
        .with_language(cbindgen::Language::C)
        .with_no_includes()
        .with_after_include(CBINDGEN_AFTER_INCLUDES)
        .with_include_guard("RUST_H")
        .generate()
        .expect("Unable to generate")
        .write_to_file(kernel_path.join("rust.h"));
}
