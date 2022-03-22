use crossbeam_channel::{Receiver, Sender, TrySendError};
use reaper_high::Reaper;
use std::error::Error;
use std::fmt;
use std::fmt::{Debug, Display, Formatter};

pub trait NamedChannelSender {
    type Msg;

    /// Sends the given message if the channel still has space, otherwise does nothing.
    fn send_if_space(&self, msg: Self::Msg);

    /// Sends the given message if the channel still has space, otherwise panics.
    fn send_complaining(&self, msg: Self::Msg);
}

/// A channel intended to send messages to a normal (non-real-time) thread.
pub struct SenderToNormalThread<T> {
    channel_name: &'static str,
    sender: Sender<T>,
}

impl<T> Debug for SenderToNormalThread<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("SenderToNormalThread")
            .field("channel_name", &self.channel_name)
            .field("sender", &self.sender)
            .finish()
    }
}

impl<T> NamedChannelSender for SenderToNormalThread<T> {
    type Msg = T;

    fn send_if_space(&self, msg: T) {
        let _ = self.send_internal(msg);
    }

    fn send_complaining(&self, msg: T) {
        self.send_internal(msg).unwrap();
    }
}

impl<T> SenderToNormalThread<T> {
    pub fn new_bounded_channel(name: &'static str, capacity: usize) -> (Self, Receiver<T>) {
        let (sender, receiver) = crossbeam_channel::bounded(capacity);
        (
            Self {
                channel_name: name,
                sender,
            },
            receiver,
        )
    }

    pub fn new_unbounded_channel(name: &'static str) -> (Self, Receiver<T>) {
        let (sender, receiver) = crossbeam_channel::unbounded();
        (
            Self {
                channel_name: name,
                sender,
            },
            receiver,
        )
    }

    pub fn try_to_send(&self, msg: T) -> bool {
        self.sender.try_send(msg).is_ok()
    }

    pub fn is_bounded(&self) -> bool {
        self.sender.capacity().is_some()
    }

    fn send_internal(&self, msg: T) -> Result<(), NamedChannelTrySendError<T>> {
        try_send_on_named_channel(&self.sender, self.channel_name, msg)
    }
}

impl<T> Clone for SenderToNormalThread<T> {
    fn clone(&self) -> Self {
        Self {
            channel_name: self.channel_name,
            sender: self.sender.clone(),
        }
    }
}

/// A channel intended to send messages to real-time threads.
///
/// It has special logic which makes sure the queue doesn't run full when audio is not running.
#[derive(Debug)]
pub struct SenderToRealTimeThread<T> {
    channel_name: &'static str,
    sender: Sender<T>,
}

impl<T> Clone for SenderToRealTimeThread<T> {
    fn clone(&self) -> Self {
        Self {
            channel_name: self.channel_name,
            sender: self.sender.clone(),
        }
    }
}

impl<T> NamedChannelSender for SenderToRealTimeThread<T> {
    type Msg = T;

    fn send_if_space(&self, msg: T) {
        let _ = self.send_internal(msg);
    }

    fn send_complaining(&self, msg: T) {
        self.send_internal(msg).unwrap();
    }
}

impl<T> SenderToRealTimeThread<T> {
    pub fn new_channel(name: &'static str, capacity: usize) -> (Self, Receiver<T>) {
        let (sender, receiver) = crossbeam_channel::bounded(capacity);
        (
            Self {
                channel_name: name,
                sender,
            },
            receiver,
        )
    }

    fn send_internal(&self, msg: T) -> Result<(), NamedChannelTrySendError<T>> {
        if Reaper::get().audio_is_running() {
            // Audio is running so sending should always work. If not, it's an unexpected error and
            // we must return it.
            try_send_on_named_channel(&self.sender, self.channel_name, msg)
        } else {
            // Audio is not running. Maybe this is just a very temporary outage or a short initial
            // non-running state.
            if self.channel_still_has_some_headroom() {
                // Channel still has some headroom, so we send the task in order to support a
                // temporary outage. This should not fail unless another sender has exhausted the
                // channel in the meanwhile. Even then, so what. See "else" branch.
                let _ = self.sender.try_send(msg);
                Ok(())
            } else {
                // Channel has already accumulated lots of tasks. Don't send!
                // It's not bad if we don't send this task because the real-time processor will
                // not be able to process it anyway at the moment (it's not going to be called
                // because the audio engine is stopped). Fear not, ReaLearn's audio hook has logic
                // that detects a "rebirth" - the moment when the audio cycle starts again. In this
                // case it will request a full resync of everything so nothing should get lost
                // in theory.
                Ok(())
            }
        }
    }

    fn channel_still_has_some_headroom(&self) -> bool {
        self.sender.len() <= self.sender.capacity().unwrap() / 2
    }
}

fn try_send_on_named_channel<T>(
    sender: &Sender<T>,
    channel_name: &'static str,
    msg: T,
) -> Result<(), NamedChannelTrySendError<T>> {
    sender.try_send(msg).map_err(|e| NamedChannelTrySendError {
        channel_name,
        try_send_error: e,
    })
}

#[derive(Copy, Clone, Eq, PartialEq)]
struct NamedChannelTrySendError<T> {
    channel_name: &'static str,
    try_send_error: TrySendError<T>,
}

impl<T> Debug for NamedChannelTrySendError<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Channel [{}]: {:?}",
            self.channel_name, self.try_send_error
        )
    }
}

impl<T> Display for NamedChannelTrySendError<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Channel [{}]: {}",
            self.channel_name, self.try_send_error
        )
    }
}

impl<T: Send> Error for NamedChannelTrySendError<T> {}
