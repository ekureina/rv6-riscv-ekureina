pub mod spinlock {
    use crate::{
        c_bindings,
        interrupts::{pop_off, push_off},
    };
    use core::{
        cell::Cell,
        ptr::NonNull,
        sync::atomic::{fence, AtomicBool},
    };

    /// cbindgen:no-export
    #[repr(C)]
    pub struct Spinlock<'a> {
        locked: AtomicBool,
        name: &'a str,
        cpu: Cell<Option<NonNull<c_bindings::cpu>>>,
    }

    impl<'a> Spinlock<'a> {
        pub(crate) const fn new_const(name: &'a str) -> Self {
            Self {
                locked: AtomicBool::new(false),
                name,
                cpu: Cell::new(None),
            }
        }

        pub(crate) fn acquire(&self) {
            push_off();
            if self.holding() {
                unsafe {
                    c_bindings::panic(b"acquire_rust\0".as_ptr().cast::<i8>().cast_mut());
                }
            }

            while self
                .locked
                .compare_exchange(
                    false,
                    true,
                    core::sync::atomic::Ordering::Relaxed,
                    core::sync::atomic::Ordering::Relaxed,
                )
                .is_err()
            {
                core::hint::spin_loop();
            }

            fence(core::sync::atomic::Ordering::SeqCst);

            self.cpu
                .set(unsafe { c_bindings::mycpu().as_mut() }.map(NonNull::from));
        }

        pub(crate) fn release(&self) {
            if !self.holding() {
                unsafe {
                    c_bindings::panic(b"release_rust\0".as_ptr().cast::<i8>().cast_mut());
                }
            }

            self.cpu.set(None);

            fence(core::sync::atomic::Ordering::SeqCst);

            self.locked
                .store(false, core::sync::atomic::Ordering::Release);

            pop_off();
        }

        pub(crate) fn holding(&self) -> bool {
            // We have to be running on a CPU, so if the lock is locked, we can guarantee that the CPU is set
            self.locked.load(core::sync::atomic::Ordering::Acquire)
                && self
                    .cpu
                    .get()
                    .is_some_and(|ptr| ptr.as_ptr() == unsafe { c_bindings::mycpu() })
        }
    }

    /// Initializes a Rust-held spinlock
    /// # Safety
    /// `name` should point to a valid c-string, and `lock` should point to enough space to store a spinlock
    /// # Panics
    /// Panics if `name` does not contain valid utf-8
    #[no_mangle]
    pub unsafe extern "C" fn initlock_rust(lock: *mut Spinlock, name: *mut i8) {
        let new_lock = Spinlock::new_const(
            core::ffi::CStr::from_ptr(name.cast_const())
                .to_str()
                .unwrap(),
        );
        *lock = new_lock;
    }

    /// Acquires a Rust-held spinlock
    #[no_mangle]
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub extern "C" fn acquire_rust(lock: *mut Spinlock) {
        if let Some(lock) = unsafe { lock.as_mut() } {
            lock.acquire();
        } else {
            unsafe {
                c_bindings::panic(b"acquire_rust\0".as_ptr().cast::<i8>().cast_mut());
            }
        }
    }

    /// Releases a Rust-held spinlock
    #[no_mangle]
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub extern "C" fn release_rust(lock: *mut Spinlock) {
        if let Some(lock) = unsafe { lock.as_mut() } {
            lock.release();
        } else {
            unsafe {
                c_bindings::panic(b"release_rust\0".as_ptr().cast::<i8>().cast_mut());
            }
        }
    }

    /// Checks whether the current cpu is holding a Rust-held spinlock
    #[no_mangle]
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub extern "C" fn holding_rust(lock: *mut Spinlock) -> bool {
        if let Some(lock) = unsafe { lock.as_mut() } {
            lock.holding()
        } else {
            unsafe { c_bindings::panic(b"release_rust\0".as_ptr().cast::<i8>().cast_mut()) }
        }
    }
}
