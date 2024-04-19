use std::fmt::{Arguments, Debug};
use std::io::{IoSlice, Write};
use std::sync::mpsc::{Receiver, Sender};
use std::time::Duration;
use std::{mem, thread};

use reaper_high::Reaper;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{registry, EnvFilter};

use crate::infrastructure::plugin::tracing_spam_filter::SpamFilter;

#[derive(Debug)]
pub struct TracingHook {
    sender: Sender<AsyncLoggerCommand>,
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
        let (sender, receiver) = std::sync::mpsc::channel();
        thread::Builder::new()
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
        let fmt_layer = tracing_subscriber::fmt::layer()
            .pretty()
            // .with_thread_ids(true)
            // .with_thread_names(true)
            // .compact()
            .with_writer(move || AsyncWriter::new(std::io::stdout(), cloned_sender.clone()));
        let inner_subscriber = registry().with(env_filter).with(fmt_layer);
        let spam_filter = SpamFilter::new(1, Duration::from_secs(30), inner_subscriber);
        tracing::subscriber::set_global_default(spam_filter)
            .expect("setting default subscriber failed");
        let hook = Self { sender };
        Some(hook)
    }
}

impl Drop for TracingHook {
    fn drop(&mut self) {
        // This prevents the metrics recorder thread from lurking around after the library
        // is unloaded (which is of importance if "Allow complete unload of VST plug-ins"
        // is enabled in REAPER for Windows). Ideally, we would just destroy the sender to achieve
        // the same effect. But the sender is buried within the tracing framework and probably
        // cloned multiple times. Unloading the library will just free the memory without triggering
        // the drop, so that wouldn't work either.
        let _ = self.sender.send(AsyncLoggerCommand::Finish);
        // Joining the thread here somehow leads to a deadlock. Not sure why. It doesn't
        // seem to be necessary anyway. The thread will end no matter what.
    }
}

struct AsyncWriter<W> {
    sender: Sender<AsyncLoggerCommand>,
    inner: W,
    data: Vec<u8>,
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
            let _ = self.sender.send(AsyncLoggerCommand::Log(data));
            Ok(())
        } else {
            self.inner.write_all(buf)
        }
    }

    fn write_fmt(&mut self, _fmt: Arguments<'_>) -> std::io::Result<()> {
        unimplemented!()
    }
}
