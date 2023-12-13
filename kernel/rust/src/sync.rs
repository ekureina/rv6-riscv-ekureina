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
}
