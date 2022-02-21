use crossbeam_channel::{Receiver, Sender};
use reaper_high::Reaper;
use std::fmt::Arguments;
use std::io::{BufWriter, IoSlice, LineWriter, Write};
use std::thread::JoinHandle;
use std::{mem, thread};
use tracing_subscriber::{EnvFilter, FmtSubscriber};

pub fn setup_tracing() {
    // At the beginning, I wrapped the subscriber in one that calls permit_alloc() in on_event()
    // in order to prevent assert_no_alloc() from aborting because of logging. However, this was
    // not enough. Some tracing-core stuff also did allocations and it was not possible to wrap it.
    // The approach now is that we wrap the log macros (see example in playtime-clip-matrix crate).
    let (sender, receiver) = crossbeam_channel::unbounded();
    thread::Builder::new()
        .name(String::from("ReaLearn async logger"))
        .spawn(move || keep_logging(receiver, std::io::stdout()))
        .unwrap();
    let subscriber = FmtSubscriber::builder()
        .with_env_filter(EnvFilter::from_env("REALEARN_LOG"))
        .with_writer(move || AsyncWriter::new(std::io::stdout(), sender.clone()))
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");
}

struct AsyncWriter<W> {
    sender: Sender<Vec<u8>>,
    inner: W,
    data: Vec<u8>,
}

impl<W: Write> AsyncWriter<W> {
    pub fn new(inner: W, sender: Sender<Vec<u8>>) -> Self {
        Self {
            sender,
            inner,
            data: vec![],
        }
    }
}

fn keep_logging(receiver: Receiver<Vec<u8>>, mut inner: impl Write) {
    while let Ok(msg) = receiver.recv() {
        inner.write(&msg).unwrap();
        inner.flush().unwrap();
    }
}

impl<W: Write> Write for AsyncWriter<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        unimplemented!()
    }

    fn write_vectored(&mut self, bufs: &[IoSlice<'_>]) -> std::io::Result<usize> {
        unimplemented!()
    }

    fn flush(&mut self) -> std::io::Result<()> {
        unimplemented!()
    }

    fn write_all(&mut self, buf: &[u8]) -> std::io::Result<()> {
        if Reaper::get().medium_reaper().is_in_real_time_audio() {
            self.data.write_all(buf)?;
            let data = mem::take(&mut self.data);
            let _ = self.sender.try_send(data);
            Ok(())
        } else {
            self.inner.write_all(buf)
        }
    }

    fn write_fmt(&mut self, fmt: Arguments<'_>) -> std::io::Result<()> {
        unimplemented!()
    }
}
