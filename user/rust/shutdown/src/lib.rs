#![no_std]

use core::panic::PanicInfo;

/// cbindgen:no-export
/// Bindings generated to the C User APIs of xv6
#[allow(non_upper_case_globals)]
#[allow(non_camel_case_types)]
#[allow(clippy::unreadable_literal)]
pub mod c_bindings {
    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}

#[no_mangle]
pub extern "C" fn main() -> u32 {
    unsafe {
        c_bindings::shutdown();
        c_bindings::exit(0);
    };
}

#[panic_handler]
fn panic_handler(_info: &PanicInfo) -> ! {
    loop {}
}
