use crate::{
    c_bindings,
    sync::spinlock::{Spintex, SpintexGuard},
};
use core::ptr::{self, NonNull};

pub(crate) fn sleep_rust<T, U>(chan: NonNull<T>, guard: SpintexGuard<'_, '_, U>) {
    let proc = unsafe { c_bindings::myproc().as_mut().unwrap() };
    // Must acquire p->lock in order to
    // change p->state and then call sched.
    // Once we hold p->lock, we can be
    // guaranteed that we won't miss any wakeup
    // (wakeup locks p->lock),
    // so it's okay to release lock.
    unsafe {
        c_bindings::acquire(ptr::addr_of_mut!(proc.lock));
    }
    Spintex::unlock(guard);

    // Go to sleep
    proc.chan = chan.as_ptr().cast();
    proc.state = c_bindings::procstate::SLEEPING;

    unsafe {
        c_bindings::sched();
    }

    // Tidy up
    proc.chan = ptr::null_mut();

    // Reacquire original lock
    unsafe {
        c_bindings::release(ptr::addr_of_mut!(proc.lock));
    }
}
