use std::env;
use std::path::PathBuf;

fn main() {
    let target = env::var("TARGET").unwrap();
    assert!(target == "riscv64gc-unknown-none-elf",
        "Only able to compile this kernel for target 'riscv64gc-unknown-none-elf', attempted to build for '{target}'");
    let current_path = env::current_dir().unwrap();

    let base_path = current_path
        .ancestors()
        .find(|path| {
            let potential_kernel_path = path.join("kernel");
            potential_kernel_path.exists() && potential_kernel_path.is_dir()
        })
        .unwrap();
    let headers = vec![
        base_path
            .join("kernel")
            .join("types.h")
            .to_string_lossy()
            .into_owned(),
        base_path
            .join("kernel")
            .join("param.h")
            .to_string_lossy()
            .into_owned(),
        base_path
            .join("user")
            .join("user.h")
            .to_string_lossy()
            .into_owned(),
    ];

    let bindings = headers
        .iter()
        .fold(bindgen::Builder::default(), |builder, header| {
            builder.header(header)
        })
        .use_core()
        .generate_cstr(true)
        .default_enum_style(bindgen::EnumVariation::Rust {
            non_exhaustive: false,
        })
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .expect("Unable to generate bindings");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");
}
