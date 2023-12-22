#![no_std]

extern crate alloc;

use core::panic::PanicInfo;

/// cbindgen:no-export
/// Bindings generated to the C Kernel APIs of xv6
#[allow(non_upper_case_globals)]
#[allow(non_camel_case_types)]
#[allow(clippy::unreadable_literal)]
pub mod c_bindings {
    include!(concat!(env!("OUT_DIR"), "/kernel_bindings.rs"));
}

/// Loads devices from the flattened device tree
pub mod device_load;
/// Exec syscall implementation details
pub mod exec;
/// Interface to qemu's `fw_config` files
pub mod fw_config;
/// Interrupt handling
pub mod interrupts;
/// Kernel page allocations
pub mod kalloc;
/// Functions around printing to the screen
pub mod printf;
/// Macros for interfacing with riscv assembly
pub mod riscv_asm;
/// Kernel Sycronization primatives
pub mod sync;
/// rv6 syscall implementations
pub mod syscall;
/// rv6 trap handlers
pub mod trap;
/// rv6 Virtual Memory routines
pub mod vm;

/// Panic handler that calls xv6's `panic` method.
#[panic_handler]
fn panic_handler(info: &PanicInfo) -> ! {
    let mut bytes = [0i8; c_bindings::MAXPATH as usize];
    unsafe {
        let payload = info.payload().downcast_ref::<&str>().unwrap();
        #[allow(clippy::cast_possible_wrap)]
        (*payload)
            .as_bytes()
            .iter()
            .zip(bytes.iter_mut())
            .for_each(|(payload, byte)| {
                *byte = *payload as i8;
            });

        bytes[payload.len()] = 0;
        c_bindings::panic(bytes.as_mut_ptr());
    }
}
