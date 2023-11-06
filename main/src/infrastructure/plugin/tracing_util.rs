use crossbeam_channel::{Receiver, Sender};
use reaper_high::Reaper;
use std::fmt::Arguments;
use std::io::{IoSlice, Write};
use std::thread::JoinHandle;
use std::{mem, thread};
use tracing_subscriber::{EnvFilter, FmtSubscriber, Layer};

#[derive(Debug)]
pub struct TracingHook {
    sender: Sender<AsyncLoggerCommand>,
    join_handle: Option<JoinHandle<()>>,
}

impl TracingHook {
    /// Initializes tracing/logging if the env variable `REALEARN_LOG` is set.
    ///
    /// This doesn't just set the global tracing subscriber, it also starts an async logger
    /// thread, which is responsible for logging messages that come from real-time threads (reducing
    /// the amount of audio stuttering).
    ///
    /// This should be called only once within the lifetime of the loaded shared library! On Linux
    /// and macOS, this means it must only be called once within the lifetime of REAPER because once
    /// a shared library is loaded, it's not unloaded anymore. On Windows, it can be called again
    /// after REAPER unloaded the library via `FreeLibrary` and reloaded it again.
    ///
    /// The returned tracing hook must be dropped before the library is unloaded, otherwise the
    /// async logger thread sticks around and that can't be good.
    pub fn init() -> Option<Self> {
        let env_var = std::env::var("REALEARN_LOG").ok()?;
        let env_filter = EnvFilter::new(env_var);
        let (sender, receiver) = crossbeam_channel::unbounded();
        let join_handle = thread::Builder::new()
            .name(String::from("ReaLearn async logger"))
            .spawn(move || keep_logging(receiver, std::io::stdout()))
            .expect("ReaLearn async logger thread couldn't be created");
        // In the beginning, I wrapped the subscriber in one that calls permit_alloc() in on_event()
        // in order to prevent assert_no_alloc() from aborting because of logging. However, this was
        // not enough. Some tracing-core stuff also did allocations and it was not possible to wrap it.
        // The approach now is that we wrap the log macros (see example in playtime-clip-matrix crate).
        // TODO-medium Maybe we can improve on that by triggering the initial allocations in the
        //  audio hook and using a large ring buffer to avoid any allocation when sending the log
        //  messages?
        let cloned_sender = sender.clone();
        let subscriber = FmtSubscriber::builder()
            .pretty()
            // .with_thread_ids(true)
            // .with_thread_names(true)
            // .compact()
            .with_env_filter(env_filter)
            .with_writer(move || AsyncWriter::new(std::io::stdout(), cloned_sender.clone()))
            .finish();
        tracing::subscriber::set_global_default(subscriber)
            .expect("setting default subscriber failed");
        let hook = Self {
            sender,
            join_handle: Some(join_handle),
        };
        Some(hook)
    }
}

impl Drop for TracingHook {
    fn drop(&mut self) {
        let _ = self.sender.try_send(AsyncLoggerCommand::Finish);
        if let Some(join_handle) = self.join_handle.take() {
            let _ = join_handle.join();
        }
    }
}

struct AsyncWriter<W> {
    sender: Sender<AsyncLoggerCommand>,
    inner: W,
    data: Vec<u8>,
}

impl<W> Drop for AsyncWriter<W> {
    fn drop(&mut self) {
        println!("Dropping AsyncWriter...");
    }
}

enum AsyncLoggerCommand {
    Finish,
    Log(Vec<u8>),
}

impl<W: Write> AsyncWriter<W> {
    pub fn new(inner: W, sender: Sender<AsyncLoggerCommand>) -> Self {
        Self {
            sender,
            inner,
            data: vec![],
        }
    }
}

fn keep_logging(receiver: Receiver<AsyncLoggerCommand>, mut inner: impl Write) {
    while let Ok(cmd) = receiver.recv() {
        match cmd {
            AsyncLoggerCommand::Finish => break,
            AsyncLoggerCommand::Log(msg) => {
                inner.write_all(&msg).unwrap();
                inner.flush().unwrap();
            }
        }
    }
    println!("Async logging finished");
}

impl<W: Write> Write for AsyncWriter<W> {
    fn write(&mut self, _buf: &[u8]) -> std::io::Result<usize> {
        unimplemented!()
    }

    fn write_vectored(&mut self, _bufs: &[IoSlice<'_>]) -> std::io::Result<usize> {
        unimplemented!()
    }

    fn flush(&mut self) -> std::io::Result<()> {
        unimplemented!()
    }

    fn write_all(&mut self, buf: &[u8]) -> std::io::Result<()> {
        if Reaper::get().medium_reaper().is_in_real_time_audio() {
            self.data.write_all(buf)?;
            let data = mem::take(&mut self.data);
            let _ = self.sender.try_send(AsyncLoggerCommand::Log(data));
            Ok(())
        } else {
            self.inner.write_all(buf)
        }
    }

    fn write_fmt(&mut self, _fmt: Arguments<'_>) -> std::io::Result<()> {
        unimplemented!()
    }
}
