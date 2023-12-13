use crate::c_bindings;
use crate::device_load::PHYSICAL_ADDRESS_STOP;
use crate::sync::spinlock::Spintex;
use alloc::alloc::{GlobalAlloc, Layout};
use core::cell::Cell;
use core::ptr::{self, null_mut, NonNull};

const MEM_LOCK_NAME: &str = "kmem";

extern "C" {
    static end: *const u8;
}

#[repr(C)]
struct Run {
    pub next: Cell<Option<NonNull<Run>>>,
}

pub(crate) struct KernelPageAllocator<'a> {
    freelist: Spintex<'a, Cell<Option<NonNull<Run>>>>,
}

#[global_allocator]
pub(crate) static ALLOCATOR: KernelPageAllocator = KernelPageAllocator {
    freelist: Spintex::new(Cell::new(None), MEM_LOCK_NAME),
};

impl<'a> KernelPageAllocator<'a> {
    pub(crate) unsafe fn pfree_count(&self) -> u64 {
        let mut free_memory = 0u64;
        let freelist = self.freelist.lock();
        let mut optional_run_ref = freelist.get();
        while optional_run_ref.is_some() {
            free_memory += u64::from(c_bindings::PGSIZE);
            optional_run_ref = optional_run_ref.and_then(|ptr| ptr.as_ref().next.get());
        }
        free_memory
    }
}

unsafe impl<'a> Sync for KernelPageAllocator<'a> {}
unsafe impl<'a> Send for KernelPageAllocator<'a> {}

unsafe impl<'a> GlobalAlloc for KernelPageAllocator<'a> {
    /// Allocates a page of physical memory
    /// Ignores `layout`, except to check that the request is for no more than a page of memory
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let size = layout.size();
        let align = layout.align();

        // Cannot allocate or align more than a page
        if size > c_bindings::PGSIZE as usize || align > c_bindings::PGSIZE as usize {
            return ptr::null_mut();
        }

        let freelist = self.freelist.lock();
        let return_cell = Cell::new(None);
        return_cell.swap(&freelist);
        if let Some(run_cell) = return_cell.get() {
            freelist.swap(&run_cell.as_ref().next);
        }

        Spintex::unlock(freelist);

        match return_cell.get().map(NonNull::cast::<u8>) {
            None => null_mut(),
            Some(ptr) => {
                let final_ptr = ptr.as_ptr();
                ptr::write_bytes(final_ptr, 5, c_bindings::PGSIZE as usize);
                final_ptr
            }
        }
    }

    /// Deallocate a page allocated by this allocator
    #[allow(clippy::cast_ptr_alignment)]
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let size = layout.size();
        let align = layout.align();
        let ptr_int = ptr as usize;
        if ptr_int % c_bindings::PGSIZE as usize != 0
            || ptr < end.cast_mut()
            || ptr_int >= usize::try_from(PHYSICAL_ADDRESS_STOP).unwrap()
            || size > c_bindings::PGSIZE as usize
            || align > c_bindings::PGSIZE as usize
        {
            c_bindings::panic(b"KPA_dealloc\0".as_ptr().cast::<i8>().cast_mut());
        }

        ptr::write_bytes(ptr, 1, c_bindings::PGSIZE as usize);
        let run_ref_option: Option<&'static mut Run> = ptr.cast::<Run>().as_mut();
        if let Some(run_ref) = run_ref_option {
            let freelist = self.freelist.lock();
            run_ref.next = Cell::new(None);
            run_ref.next.swap(&freelist);
            let run_cell = Cell::new(Some(NonNull::new_unchecked(run_ref as *mut Run)));
            freelist.swap(&run_cell);
        }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let size = layout.size();
        let align = layout.align();

        // Cannot allocate or align more than a page, otherwise, this realloc is valid in place
        if size > c_bindings::PGSIZE as usize
            || align > c_bindings::PGSIZE as usize
            || new_size > c_bindings::PGSIZE as usize
        {
            return ptr::null_mut();
        }

        ptr
    }
}

/// C Entry point for Kernel Page Alloc
#[no_mangle]
#[allow(clippy::missing_safety_doc)]
pub unsafe extern "C" fn kalloc() -> *mut core::ffi::c_void {
    let layout =
        Layout::from_size_align_unchecked(c_bindings::PGSIZE as usize, c_bindings::PGSIZE as usize);
    alloc::alloc::alloc(layout).cast()
}

/// C Entry point for Kernel Page Frees
/// # Safety
/// `ptr` should be allocated from `kalloc` and should be page-aligned
#[no_mangle]
pub unsafe extern "C" fn kfree(ptr: *mut core::ffi::c_void) {
    let layout =
        Layout::from_size_align_unchecked(c_bindings::PGSIZE as usize, c_bindings::PGSIZE as usize);
    alloc::alloc::dealloc(ptr.cast(), layout);
}
