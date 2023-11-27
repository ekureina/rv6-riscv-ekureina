use core::ptr;

use crate::c_bindings;

static mut FW_CFG_GET_NAME_PANIC: &[u8] = b"FWCfgFile get_name\0";
static mut FW_CFG_NOT_AVAILABLE: &[u8] = b"fw_cfg_not_available\0";
static mut FW_CFG_INVALID_FILENAME: &[u8] = b"fw_cfg_filename_invalid\0";
/// `fw_cfg` memory address base, as delared in the [QEMU code](https://github.com/qemu/qemu/blob/44f28df24767cf9dca1ddc9b23157737c4cbb645/hw/riscv/virt.c#L56)
pub const FW_CFG_BASE: usize = 0x1010_0000;
/// Address of the Control Register in MMIO space
const FW_CFG_CTL_ADDR: usize = FW_CFG_BASE + FW_CFG_DATA_SIZE;
/// Address of the data Register in MMIO space
const FW_CFG_DATA_ADDR: usize = FW_CFG_BASE;
/// Address of the DMA Register in MMIO space
const FW_CFG_DMA_ADDR: usize = FW_CFG_BASE + 16;
/// Size of the data register
const FW_CFG_DATA_SIZE: usize = 8;
/// Selection Key for the `fw_cfg` signature
const FW_CFG_SIGNATURE: u16 = 0;
/// Selection Key for the `fw_cfg` capabilities register
const FW_CFG_ID: u16 = 1;
/// Selection key for the listing API
const FW_CFG_FILE_DIR: u16 = 0x0019;
/// Expected Signature bits to verify we are running in QEMU
static SIGNATURE_DATA: &[u8; 5] = b"QEMU\0";

/// Struct containing `fw_cfg` file data
struct FWCfgFile {
    pub size: u32,
    pub selector_key: u16,
    reserved: u16,
    name: [u8; 56],
}

impl FWCfgFile {
    fn get_name(&self) -> &core::ffi::CStr {
        match core::ffi::CStr::from_bytes_until_nul(&self.name) {
            Ok(str) => str,
            Err(_) => unsafe { c_bindings::panic(ptr::addr_of_mut!(FW_CFG_GET_NAME_PANIC).cast()) },
        }
    }

    fn new(size: u32, selector_key: u16, name: &core::ffi::CStr) -> Self {
        let mut stored_name = [0u8; 56];
        name.to_bytes_with_nul()
            .iter()
            .zip(stored_name.iter_mut())
            .for_each(|(name_byte, store_byte)| *store_byte = *name_byte);
        FWCfgFile {
            size,
            selector_key,
            reserved: 0,
            name: stored_name,
        }
    }
}

/** Get up to `max_data_size` bytes of data into `data_pointer` from the `fw_cfg` file labeled `filename`

# Safety

Expects `filename` to point to a valid, null-terminated, c-style string.
*/
#[no_mangle]
#[allow(clippy::cast_possible_wrap)]
pub unsafe extern "C" fn get_fw_cfg(
    filename: *const i8,
    data_pointer: *mut u8,
    max_data_size: u32,
) -> u32 {
    let filename_str = core::ffi::CStr::from_ptr(filename);
    let file_descriptor = find_file(filename_str);
    set_ctl(file_descriptor.selector_key);
    let data_copy_size = if max_data_size > file_descriptor.size {
        file_descriptor.size
    } else {
        max_data_size
    };

    for i in 0..data_copy_size {
        ptr::copy_nonoverlapping(
            FW_CFG_DATA_ADDR as *const u8,
            data_pointer.offset(i as isize),
            1,
        );
    }
    data_copy_size
}

fn find_file(filename: &core::ffi::CStr) -> FWCfgFile {
    if !verify_fw_cfg() {
        unsafe { c_bindings::panic(ptr::addr_of_mut!(FW_CFG_NOT_AVAILABLE).cast()) }
    }

    set_ctl(FW_CFG_FILE_DIR);
    let fw_cfg_file_count = get_data_u32_be();

    for _ in 0..fw_cfg_file_count {
        let fw_cfg_file = get_data_fw_cfg_file();
        if fw_cfg_file.get_name() == filename {
            return fw_cfg_file;
        }
    }
    let invalid_name =
        unsafe { core::ffi::CStr::from_bytes_until_nul(FW_CFG_INVALID_FILENAME) }.unwrap();
    FWCfgFile::new(u32::MAX, u16::MAX, invalid_name)
}

fn verify_fw_cfg() -> bool {
    set_ctl(FW_CFG_SIGNATURE);

    let signature_data = get_data_bytes::<5>();

    signature_data
        .iter()
        .zip(SIGNATURE_DATA.iter())
        .all(|(read_data, expected_data)| *read_data == *expected_data)
}

fn set_ctl(data: u16) {
    unsafe {
        *core::mem::transmute::<usize, &'static mut u16>(FW_CFG_CTL_ADDR) = u16::to_be(data);
    }
}

fn get_data_bytes<const N: usize>() -> [u8; N] {
    let data_ptr: *const u8 = FW_CFG_DATA_ADDR as *const u8;
    let mut data_out = [0u8; N];
    for data_chunk in &mut data_out {
        *data_chunk = unsafe { core::ptr::read_volatile(data_ptr) };
    }
    data_out
}

fn get_data_u32_be() -> u32 {
    let data_bytes = get_data_bytes::<{ core::mem::size_of::<u32>() }>();
    u32::from_be_bytes(data_bytes)
}

fn get_data_u16_be() -> u16 {
    let data_bytes = get_data_bytes::<{ core::mem::size_of::<u16>() }>();
    u16::from_be_bytes(data_bytes)
}

fn get_data_fw_cfg_file() -> FWCfgFile {
    let size = get_data_u32_be();
    let selector_key = get_data_u16_be();
    get_data_bytes::<2>();
    let name = get_data_bytes::<56>();
    FWCfgFile::new(size, selector_key, unsafe {
        core::ffi::CStr::from_ptr(name.as_ptr().cast())
    })
}
