use std::{env, path::PathBuf};

use bindgen::callbacks::{ParseCallbacks, TypeKind};

fn main() {
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
    println!("cargo:rerun-if-changed={}", kernel_path.to_string_lossy());

    let mut kernel_headers = std::collections::HashSet::new();
    for kernel_file in kernel_path
        .read_dir()
        .unwrap_or_else(|_| panic!("unable to view kernel directory: {kernel_path:?}"))
        .flatten()
    {
        let kernel_file_path = kernel_file.path().to_string_lossy().into_owned();
        if kernel_file_path.contains(".h") && !kernel_file_path.ends_with("rust.h") {
            kernel_headers.insert(kernel_file_path.clone());
        }
    }
    eprintln!("{kernel_headers:?}");

    let bindings = kernel_headers
        .iter()
        .fold(bindgen::Builder::default(), |builder, kernel_header| {
            builder.header(kernel_header)
        })
        .generate_cstr(true)
        .default_enum_style(bindgen::EnumVariation::Rust {
            non_exhaustive: false,
        })
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .parse_callbacks(Box::new(PodDeriver))
        .generate()
        .expect("unable to generate kernel bindings");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("kernel_bindings.rs"))
        .expect("Couldn't write kernel bindings!");
}

#[derive(Debug)]
struct PodDeriver;

impl ParseCallbacks for PodDeriver {
    fn add_derives(&self, info: &bindgen::callbacks::DeriveInfo<'_>) -> Vec<String> {
        if info.kind == TypeKind::Struct
            && (info.name == "dinode" || info.name == "dirent" || info.name == "superblock")
        {
            vec![
                String::from("bytemuck::Pod"),
                String::from("bytemuck::Zeroable"),
            ]
        } else {
            vec![]
        }
    }
}
