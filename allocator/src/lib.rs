//! A variation of https://github.com/Windfisch/rust-assert-no-alloc.git with the following changes:
//!
//! - Automatically offloads deallocation to other thread when in real-time thread, both in debug
//!   and release builds
//! - Provides a pluggable deallocation function (for recording stats etc.)
//! - Assertions are active in debug builds only
//! - When assertion violated, it always panics instead of aborting or printing an error (mainly for
//!   good testability but also nice otherwise as long as set_alloc_error_hook() is still unstable)
use std::alloc::{GlobalAlloc, Layout, System};
use std::ffi::c_void;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, TrySendError};
use std::sync::{mpsc, OnceLock};
use std::thread;
use std::thread::JoinHandle;

#[cfg(debug_assertions)]
thread_local! {
    static ALLOC_FORBID_COUNT: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
    static ALLOC_PERMIT_COUNT: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

static UNDESIRED_ALLOCATION_COUNTER: AtomicU32 = AtomicU32::new(0);

pub fn undesired_allocation_count() -> u32 {
    UNDESIRED_ALLOCATION_COUNTER.load(Ordering::Relaxed)
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
    ret
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
    ret
}

/// The custom allocator that handles the checking.
pub struct HelgobossAllocator<I, D> {
    sync_deallocator: D,
    async_deallocation_machine: OnceLock<AsyncDeallocationMachine<I>>,
}

struct AsyncDeallocationMachine<I> {
    sender: SyncSender<AsyncDeallocatorCommand>,
    integration: I,
}

#[derive(Debug)]
pub struct AsyncDeallocatorCommandReceiver(Receiver<AsyncDeallocatorCommand>);

#[derive(Debug)]
enum AsyncDeallocatorCommand {
    Stop,
    Deallocate {
        ptr: *mut u8,
        layout: Layout,
    },
    DeallocateForeign {
        value: *mut c_void,
        deallocate: unsafe extern "C" fn(value: *mut c_void),
    },
}

unsafe impl Send for AsyncDeallocatorCommand {}

pub trait AsyncDeallocationIntegration {
    /// Should return `true` if deallocation should be offloaded to the dedicated deallocation
    /// thread (e.g. when the current thread is a real-time thread).
    fn offload_deallocation(&self) -> bool;
}

pub trait Deallocate {
    /// The actual code executed on deallocation.
    fn deallocate(&self, ptr: *mut u8, layout: Layout);
}

pub fn start_async_deallocation_thread(
    deallocator: impl Deallocate + Send + 'static,
    receiver: AsyncDeallocatorCommandReceiver,
) -> JoinHandle<AsyncDeallocatorCommandReceiver> {
    thread::Builder::new()
        .name("Helgobox deallocator".to_string())
        .spawn(move || {
            while let Ok(cmd) = receiver.0.recv() {
                match cmd {
                    AsyncDeallocatorCommand::Stop => break,
                    AsyncDeallocatorCommand::Deallocate { ptr, layout } => {
                        deallocator.deallocate(ptr, layout);
                    }
                    AsyncDeallocatorCommand::DeallocateForeign { value, deallocate } => {
                        unsafe { deallocate(value) };
                    }
                }
            }
            receiver
        })
        .unwrap()
}

impl<I, D> HelgobossAllocator<I, D>
where
    I: AsyncDeallocationIntegration,
{
    /// Initializes the allocator.
    ///
    /// The deallocator that you pass here will only be used for synchronous deallocation.
    ///
    /// This is just the first step! In order to offload deallocations, you need to call
    /// [`Self::init`]! It needs to be 2 steps
    pub const fn new(deallocator: D) -> Self {
        Self {
            sync_deallocator: deallocator,
            async_deallocation_machine: OnceLock::new(),
        }
    }

    /// This call is necessary to initialize automatic deallocation offloading (asynchronous
    /// deallocation).
    ///
    /// The deallocator that you pass here will only be used for asynchronous deallocation.
    ///
    /// As soon as the given capacity is reached, deallocation will be done synchronously until
    /// the deallocation thread has capacity again.
    pub fn init(&self, capacity: usize, integration: I) -> AsyncDeallocatorCommandReceiver {
        let (sender, receiver) = mpsc::sync_channel::<AsyncDeallocatorCommand>(capacity);
        let machine = AsyncDeallocationMachine {
            sender,
            integration,
        };
        if self.async_deallocation_machine.set(machine).is_err() {
            panic!("attempted to initialize async deallocator more than once");
        }
        AsyncDeallocatorCommandReceiver(receiver)
    }

    /// Executes the given deallocation function on the given value, possibly asynchronously.
    ///
    /// This makes it possible to defer deallocation even with values that are not managed by Rust,
    /// e.g. C values.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub fn dealloc_foreign_value(
        &self,
        deallocate: unsafe extern "C" fn(ptr: *mut c_void),
        value: *mut c_void,
    ) {
        self.dealloc_internal(
            || self.check(None),
            || unsafe {
                deallocate(value);
            },
            || AsyncDeallocatorCommand::DeallocateForeign { value, deallocate },
        )
    }

    /// Sends a stop signal to the receiver of asynchronous deallocation commands (usually a
    /// dedicated thread).
    pub fn stop_async_deallocation(&self) {
        if let Some(machine) = self.async_deallocation_machine.get() {
            let _ = machine.sender.try_send(AsyncDeallocatorCommand::Stop);
        }
    }

    fn dealloc_internal(
        &self,
        check: impl FnOnce(),
        dealloc_sync: impl FnOnce(),
        create_async_command: impl FnOnce() -> AsyncDeallocatorCommand,
    ) {
        #[cfg(not(debug_assertions))]
        let _ = check;
        let Some(deallocation_machine) = self.async_deallocation_machine.get() else {
            // We are not initialized yet. Attempt normal synchronous deallocation.
            #[cfg(debug_assertions)]
            check();
            dealloc_sync();
            return;
        };
        if deallocation_machine.integration.offload_deallocation() {
            // Deallocation shall be offloaded and we are already initialized.
            if let Err(e) = deallocation_machine.sender.try_send(create_async_command()) {
                match e {
                    TrySendError::Full(_) => {
                        // This is possible if we have very many deallocations in a row or if for
                        // some reason the async deallocator thread is not running. Then do
                        // deallocation synchronously.
                        #[cfg(debug_assertions)]
                        check();
                        dealloc_sync();
                    }
                    TrySendError::Disconnected(_) => {
                        // Could happen on shutdown
                        dealloc_sync();
                    }
                }
            }
        } else {
            // Synchronous deallocation is fine. It's still possible that we are in an
            // assert_no_alloc block. In that case, we want to report the violation.
            #[cfg(debug_assertions)]
            check();
            dealloc_sync();
        }
    }

    /// For deallocation of foreign structs (e.g. C structs), the layout is not given.
    fn check(&self, layout: Option<Layout>) {
        #[cfg(debug_assertions)]
        {
            let forbid_count = ALLOC_FORBID_COUNT.with(|f| f.get());
            let permit_count = ALLOC_PERMIT_COUNT.with(|p| p.get());
            if forbid_count > 0 && permit_count == 0 {
                // Increase our counter (will be displayed in ReaLearn status line)
                UNDESIRED_ALLOCATION_COUNTER.fetch_add(1, Ordering::Relaxed);
                // Comment out if you don't want to log the violation
                // permit_alloc(|| {
                //     if let Some(layout) = layout {
                //         eprintln!(
                //             "Undesired memory (de)allocation of {} bytes from:\n{:?}",
                //             layout.size(),
                //             backtrace::Backtrace::new()
                //         );
                //     } else {
                //         eprintln!(
                //             "Undesired memory (de)allocation of a foreign value from:\n{:?}",
                //             backtrace::Backtrace::new()
                //         );
                //     }
                // });
                // Comment out if you don't want to abort
                // if let Some(layout) = layout {
                //     std::alloc::handle_alloc_error(layout);
                // }
            }
        }
        let _ = layout;
    }
}

unsafe impl<I: AsyncDeallocationIntegration, D: Deallocate> GlobalAlloc
    for HelgobossAllocator<I, D>
{
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        #[cfg(debug_assertions)]
        self.check(Some(layout));
        System.alloc(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        self.dealloc_internal(
            || self.check(Some(layout)),
            || self.sync_deallocator.deallocate(ptr, layout),
            || AsyncDeallocatorCommand::Deallocate { ptr, layout },
        );
    }
}
