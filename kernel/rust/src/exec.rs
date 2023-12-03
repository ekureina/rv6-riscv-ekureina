use core::ptr;

use crate::c_bindings;

/// Reads an inode, determines if the file starts with a shebang line
/// # Safety
/// `inode_pointer` must point to a valid inode
#[no_mangle]
pub unsafe extern "C" fn is_shebang(inode_pointer: *mut c_bindings::inode) -> bool {
    let mut opening_chars = [0i8; 2];
    if c_bindings::readi(
        inode_pointer,
        0,
        ptr::addr_of_mut!(opening_chars) as c_bindings::uint64,
        0,
        core::mem::size_of_val(&opening_chars) as u32,
    ) != 2
    {
        return false;
    }

    opening_chars[0] == 35 && opening_chars[1] == 33
}

/// Reads an inode with a shebang line, and reads the shebang args into `argv`
/// # Safety
/// `inode_pointer` must point to a valid inode, and that inode must have a shebang line
#[no_mangle]
pub unsafe extern "C" fn read_shebang(
    inode_pointer: *mut c_bindings::inode,
    arg: *mut i8,
    max_size: i32,
) -> i32 {
    let mut offset = 0isize;
    while c_bindings::readi(
        inode_pointer,
        0,
        arg.offset(offset) as c_bindings::uint64,
        (offset + 2) as u32,
        1,
    ) == 1
        && offset < max_size as isize
    {
        if *arg.offset(offset) == 10 {
            *arg.offset(offset) = 0;
            return 0;
        }
        offset += 1;
    }
    -1
}
