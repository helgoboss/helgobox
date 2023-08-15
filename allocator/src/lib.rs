//! A variation of https://github.com/Windfisch/rust-assert-no-alloc.git with the following changes:
//!
//! - Automatically offloads deallocation to other thread when in real-time thread, both in debug
//!   and release builds
//! - Provides a pluggable deallocation function (for recording stats etc.)
//! - Assertions are active in debug builds only
//! - When assertion violated, it always panics instead of aborting or printing an error (mainly for
//!   good testability but also nice otherwise as long as set_alloc_error_hook() is still unstable)
use once_cell::sync::OnceCell;
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::mpsc;
use std::sync::mpsc::{SyncSender, TrySendError};
use std::thread;
use std::thread::JoinHandle;

#[cfg(debug_assertions)]
thread_local! {
    static ALLOC_FORBID_COUNT: Cell<u32> = Cell::new(0);
    static ALLOC_PERMIT_COUNT: Cell<u32> = Cell::new(0);
}

#[cfg(not(debug_assertions))]
pub fn assert_no_alloc<T, F: FnOnce() -> T>(func: F) -> T {
    // no-op
    func()
}

#[cfg(not(debug_assertions))]
pub fn permit_alloc<T, F: FnOnce() -> T>(func: F) -> T {
    // no-op
    func()
}

#[cfg(debug_assertions)]
/// Calls the `func` closure, but forbids any (de)allocations.
///
/// If a call to the allocator is made, the program will abort with an error,
/// print a warning (depending on the `warn_debug` feature flag. Or ignore
/// the situation, when compiled in `--release` mode with the `disable_release`
///feature flag set (which is the default)).
pub fn assert_no_alloc<T, F: FnOnce() -> T>(func: F) -> T {
    // RAII guard for managing the forbid counter. This is to ensure correct behaviour
    // when catch_unwind is used
    struct Guard;
    impl Guard {
        fn new() -> Guard {
            ALLOC_FORBID_COUNT.with(|c| c.set(c.get() + 1));
            Guard
        }
    }
    impl Drop for Guard {
        fn drop(&mut self) {
            ALLOC_FORBID_COUNT.with(|c| c.set(c.get() - 1));
        }
    }

    let guard = Guard::new(); // increment the forbid counter
    let ret = func();
    std::mem::drop(guard); // decrement the forbid counter
    return ret;
}

#[cfg(debug_assertions)]
/// Calls the `func` closure. Allocations are temporarily allowed, even if this
/// code runs inside of assert_no_alloc.
pub fn permit_alloc<T, F: FnOnce() -> T>(func: F) -> T {
    // RAII guard for managing the permit counter
    struct Guard;
    impl Guard {
        fn new() -> Guard {
            ALLOC_PERMIT_COUNT.with(|c| c.set(c.get() + 1));
            Guard
        }
    }
    impl Drop for Guard {
        fn drop(&mut self) {
            ALLOC_PERMIT_COUNT.with(|c| c.set(c.get() - 1));
        }
    }

    let guard = Guard::new(); // increment the forbid counter
    let ret = func();
    std::mem::drop(guard); // decrement the forbid counter

    return ret;
}

/// The custom allocator that handles the checking.
pub struct HelgobossAllocator<I, D> {
    sync_deallocator: D,
    async_deallocation_machine: OnceCell<AsyncDeallocationMachine<I>>,
}

struct AsyncDeallocationMachine<I> {
    sender: SyncSender<DeallocationTask>,
    worker_thread_handle: JoinHandle<()>,
    integration: I,
}

impl<I> AsyncDeallocationMachine<I> {
    pub fn new<D: Deallocate + Send + 'static>(
        capacity: usize,
        deallocator: D,
        integration: I,
    ) -> Self {
        let (sender, receiver) = mpsc::sync_channel::<DeallocationTask>(capacity);
        let worker_thread_handle = thread::Builder::new()
            .name("Helgoboss deallocator".to_string())
            .spawn(move || {
                while let Ok(task) = receiver.recv() {
                    deallocator.deallocate(task.ptr, task.layout);
                }
            })
            .unwrap();
        Self {
            sender,
            worker_thread_handle,
            integration,
        }
    }
}

impl<I, D> Drop for HelgobossAllocator<I, D> {
    fn drop(&mut self) {
        if let Some(machine) = self.async_deallocation_machine.take() {
            let _ = machine.worker_thread_handle.join();
        }
    }
}

struct DeallocationTask {
    ptr: *mut u8,
    layout: Layout,
}

unsafe impl Send for DeallocationTask {}

pub trait AsyncDeallocationIntegration {
    /// Should return `true` if deallocation should be offloaded to the dedicated deallocation
    /// thread (e.g. when the current thread is a real-time thread).
    fn offload_deallocation(&self) -> bool;
}

pub trait Deallocate {
    /// The actual code executed on deallocation.
    fn deallocate(&self, ptr: *mut u8, layout: Layout);
}

impl<I, D> HelgobossAllocator<I, D> {
    /// Initializes the allocator.
    ///
    /// The deallocator that you pass here will only be used for synchronous deallocation.
    ///
    /// This is just the first step! In order to offload deallocations, you need to call
    /// [`Self::init`]!
    pub const fn new(deallocator: D) -> Self {
        Self {
            sync_deallocator: deallocator,
            async_deallocation_machine: OnceCell::new(),
        }
    }

    /// This call is necessary to initialize automatic deallocation offloading (asynchronous
    /// deallocation).
    ///
    /// The deallocator that you pass here will only be used for asynchronous deallocation.
    ///
    /// As soon as the given capacity is reached, deallocation will be done synchronously until
    /// the deallocation thread has capacity again.
    pub fn init(
        &self,
        capacity: usize,
        deallocator: impl Deallocate + Send + 'static,
        integration: I,
    ) {
        self.async_deallocation_machine
            .get_or_init(|| AsyncDeallocationMachine::new(capacity, deallocator, integration));
    }

    #[cfg(debug_assertions)]
    fn check(&self, layout: Layout) {
        let forbid_count = ALLOC_FORBID_COUNT.with(|f| f.get());
        let permit_count = ALLOC_PERMIT_COUNT.with(|p| p.get());
        if forbid_count > 0 && permit_count == 0 {
            permit_alloc(|| {
                eprintln!(
                    "Memory allocation of {} bytes failed from:\n{:?}",
                    layout.size(),
                    backtrace::Backtrace::new()
                );
            });
            // Comment out if you don't want to abort
            // std::alloc::handle_alloc_error(layout);
        }
    }
}

unsafe impl<I: AsyncDeallocationIntegration, D: Deallocate> GlobalAlloc
    for HelgobossAllocator<I, D>
{
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        #[cfg(debug_assertions)]
        self.check(layout);
        System.alloc(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let Some(deallocation_machine) = self.async_deallocation_machine.get() else {
            // We are not initialized yet. Attempt normal synchronous deallocation.
            #[cfg(debug_assertions)]
            self.check(layout);
            self.sync_deallocator.deallocate(ptr, layout);
            return;
        };
        if deallocation_machine.integration.offload_deallocation() {
            // Deallocation shall be offloaded and we are already initialized.
            let task = DeallocationTask { ptr, layout };
            if let Err(e) = deallocation_machine.sender.try_send(task) {
                match e {
                    TrySendError::Full(_) => {
                        // This is possible if we have very many deallocations in a row. Then do
                        // deallocation synchronously.
                        #[cfg(debug_assertions)]
                        self.check(layout);
                        self.sync_deallocator.deallocate(ptr, layout);
                    }
                    TrySendError::Disconnected(_) => {
                        // Should never happen because the receiver is owned by the deallocation thread
                        // and that thread should live at least as long as the sender
                        panic!("deallocation thread is dead");
                    }
                }
            }
        } else {
            // Synchronous deallocation is fine. It's still possible that we are in an
            // assert_no_alloc block. In that case, we want to report the violation.
            #[cfg(debug_assertions)]
            self.check(layout);
            self.sync_deallocator.deallocate(ptr, layout);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    

    struct TestIntegration;
    impl AsyncDeallocationIntegration for TestIntegration {
        fn offload_deallocation(&self) -> bool {
            true
        }
    }

    struct TestDeallocator(&'static str);
    impl Deallocate for TestDeallocator {
        fn deallocate(&self, ptr: *mut u8, layout: Layout) {
            println!("[{}] Deallocating {ptr:?} with layout {layout:?}", self.0);
            unsafe {
                System.dealloc(ptr, layout);
            }
        }
    }

    #[global_allocator]
    static GLOBAL_ALLOCATOR: HelgobossAllocator<TestIntegration, TestDeallocator> =
        HelgobossAllocator::new(TestDeallocator("SYNC"));

    fn init() {
        GLOBAL_ALLOCATOR.init(100, TestDeallocator("OFFLOADED"), TestIntegration);
    }

    #[test]
    fn offload_deallocate() {
        init();
        let mut bla = vec![];
        bla.push(2);
        assert_no_alloc(|| {
            drop(bla);
        });
    }

    #[test]
    #[should_panic]
    fn abort_on_allocate() {
        init();
        assert_no_alloc(|| {
            let mut bla: Vec<i32> = vec![];
            bla.push(2);
        });
    }
}
