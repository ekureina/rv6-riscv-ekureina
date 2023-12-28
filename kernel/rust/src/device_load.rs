use crate::c_bindings;

#[no_mangle]
pub static mut PHYSICAL_ADDRESS_STOP: c_bindings::uint64 = 0;
#[no_mangle]
pub static mut CPU_COUNT: c_bindings::uint64 = 0;

/// Loads data from the FDT pointed to at `fdt_address`
/// # Safety
/// Assumes that the `fdt_address` points to a valid fdt and that the memory is mapped correctly.
/// # Panics
/// Panics  if the address or the data at the address is invalid
#[no_mangle]
pub unsafe extern "C" fn load_fdt(fdt_address: c_bindings::uint64) {
    let fdt = fdt::Fdt::from_ptr(fdt_address as *const u8).unwrap();
    // The true size of physical memory, calculated from the FDT
    let true_physical_stop = fdt
        .memory()
        .regions()
        .fold(c_bindings::KERNBASE as usize, |mem_size, region| {
            mem_size + region.size.unwrap()
        }) as c_bindings::uint64;
    // Get the CPU Count from the FDT. The max for this value for qemu's `virt` architecture is 8, but we allow for more memory to
    // be used if less CPUs are allocated.
    CPU_COUNT = fdt.cpus().count() as u64;
    // Reserved pages for the Trampoline and Kernel stacks (2 for trampoline, and 2 per CPU (stack + guard page))
    let reserved_pages =
        c_bindings::PGSIZE as usize * (2 * (usize::try_from(CPU_COUNT).unwrap() + 1));
    // Set the `PHYSICAL_ADDRESS_STOP` to the minimum of the true amount of system RAM, and the maxiumum amount of
    // physical RAM before Xv6 breaks this is about 256GiB, so this is probably unecessary, but just covering all
    // the bases here
    PHYSICAL_ADDRESS_STOP = core::cmp::min(
        true_physical_stop,
        c_bindings::MAXVA - reserved_pages as u64,
    );
}
