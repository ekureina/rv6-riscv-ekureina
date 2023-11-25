use std::env;

fn main() {
    let target = env::var("TARGET").unwrap();
    if target != "riscv64gc-unknown-none-elf" {
        panic!("Only able to compile this kernel for target 'riscv64gc-unknown-none-elf', attempted to build for '{target}'");
    }
}
