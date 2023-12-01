use crate::c_bindings;

#[no_mangle]
pub static mut PHYSICAL_ADDRESS_STOP: c_bindings::uint64 = 0;

/// Loads data from the FDT pointed to at `fdt_address`
/// # Safety
/// Assumes that the `fdt_address` points to a valid fdt and that the memory is mapped correctly.
/// # Panics
/// Panics  if the address or the data at the address is invalid
#[no_mangle]
pub unsafe extern "C" fn load_fdt(fdt_address: c_bindings::uint64) {
    let fdt = fdt::Fdt::from_ptr(fdt_address as *const u8).unwrap();
    PHYSICAL_ADDRESS_STOP = fdt
        .memory()
        .regions()
        .fold(c_bindings::KERNBASE as usize, |mem_size, region| {
            mem_size + region.size.unwrap()
        }) as c_bindings::uint64;
}
