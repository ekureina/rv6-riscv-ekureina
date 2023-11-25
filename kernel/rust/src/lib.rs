#![no_std]

use core::panic::PanicInfo;

#[allow(non_upper_case_globals)]
#[allow(non_camel_case_types)]
#[allow(clippy::unreadable_literal)]
pub mod c_bindings {
    include!(concat!(env!("OUT_DIR"), "/kernel_bindings.rs"));
}

pub mod syscall;

#[panic_handler]
fn panic_handler(_info: &PanicInfo) -> ! {
    loop {}
}
