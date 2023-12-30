#![no_std]

use rv6_user::c_bindings;

#[no_mangle]
pub extern "C" fn main() -> u32 {
    unsafe {
        c_bindings::shutdown();
        c_bindings::exit(0);
    };
}
