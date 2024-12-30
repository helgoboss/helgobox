use crossbeam_channel::{Receiver, Sender, TryRecvError, TrySendError};
use reaper_high::Reaper;
use std::error::Error;
use std::fmt;
use std::fmt::{Debug, Display, Formatter};
use std::sync::atomic::{AtomicBool, Ordering};

pub trait NamedChannelSender {
    type Msg;

    /// Sends the given message if the channel still has space and the receiver is still
    /// connected, otherwise does nothing.
    fn send_if_space(&self, msg: Self::Msg);

    /// Sends the given message if the channel still has space, otherwise panics.
    ///
    /// If the receiver is disconnected, does nothing.
    fn send_complaining(&self, msg: Self::Msg);
}

/// A channel intended to send important messages from a real-time thread to a normal (non-real-time) thread.
///
/// The way this currently works is that it uses 2 senders: One that has an initial capacity and is normally used.
/// And another one that is unbounded (and can therefore allocate) that is only used if the initial one is full.
///
/// TODO-medium Find a channel library that allows pre-allocated unbounded channels (with a high initial capacity).
pub struct ImportantSenderFromRtToNormalThread<T> {
    channel_name: &'static str,
    bounded_normal_sender: Sender<T>,
    unbounded_emergency_sender: Sender<T>,
}

impl<T> ImportantSenderFromRtToNormalThread<T> {
    pub fn new(
        channel_name: &'static str,
        capacity: usize,
    ) -> (Self, ImportantReceiverFromRtToNormalThread<T>) {
        // Main sender should belong to a bounded channel pre-allocated for normal usage.
        //
        // Emergency sender should belong to an unbounded channel and is only used if sending to the main channel would
        // block because it's full. Better allocate than block or discard the event.
        let (bounded_normal_sender, bounded_normal_receiver) = crossbeam_channel::bounded(capacity);
        let (unbounded_emergency_sender, unbounded_emergency_receiver) =
            crossbeam_channel::unbounded();
        (
            ImportantSenderFromRtToNormalThread {
                channel_name,
                bounded_normal_sender,
                unbounded_emergency_sender,
            },
            ImportantReceiverFromRtToNormalThread {
                channel_name,
                bounded_normal_receiver,
                unbounded_emergency_receiver,
            },
        )
    }
}

impl<T> ImportantSenderFromRtToNormalThread<T> {
    /// Returns `false` if receiver gone.
    pub fn send(&self, msg: T) -> bool {
        if let Err(e) = self.bounded_normal_sender.try_send(msg) {
            match e {
                TrySendError::Full(msg) => {
                    tracing::warn!(
                        msg = "Main sequence channel was full, using emergency channel (may allocate)!",
                        %self.channel_name,
                    );
                    let _ = self.unbounded_emergency_sender.send(msg);
                    true
                }
                TrySendError::Disconnected(_) => false,
            }
        } else {
            true
        }
    }
}

impl<T> Clone for ImportantSenderFromRtToNormalThread<T> {
    fn clone(&self) -> Self {
        Self {
            channel_name: self.channel_name,
            bounded_normal_sender: self.bounded_normal_sender.clone(),
            unbounded_emergency_sender: self.unbounded_emergency_sender.clone(),
        }
    }
}

pub struct ImportantReceiverFromRtToNormalThread<T> {
    channel_name: &'static str,
    bounded_normal_receiver: Receiver<T>,
    unbounded_emergency_receiver: Receiver<T>,
}

impl<T> ImportantReceiverFromRtToNormalThread<T> {
    pub fn try_recv(&self) -> Result<T, TryRecvError> {
        self.bounded_normal_receiver
            .try_recv()
            .or_else(|_| self.unbounded_emergency_receiver.try_recv())
    }
}

impl<T> Debug for ImportantReceiverFromRtToNormalThread<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("ImportantReceiverFromRtToNormalThread")
            .field("channel_name", &self.channel_name)
            .field("bounded_normal_receiver", &self.bounded_normal_receiver)
            .field(
                "unbounded_emergency_receiver",
                &self.unbounded_emergency_receiver,
            )
            .finish()
    }
}

impl<T> Debug for ImportantSenderFromRtToNormalThread<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("ImportantSenderFromRtToNormalThread")
            .field("channel_name", &self.channel_name)
            .field("bounded_normal_sender", &self.bounded_normal_sender)
            .field(
                "unbounded_emergency_sender",
                &self.unbounded_emergency_sender,
            )
            .finish()
    }
}

/// A channel intended to send messages to a normal (non-real-time) thread.
///
/// - Either unbounded (should only be used if the sender is also a normal thread).
/// - Or bounded (can also be used if the sender is a real-time thread).
///
/// If you need an unbounded one that is okay to use from a real-time thread, look into
/// [`ImportantSenderFromRtToNormalThread`].
pub struct SenderToNormalThread<T> {
    channel_name: &'static str,
    sender: Sender<T>,
    complained_already: AtomicBool,
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
        let result = self.send_internal(msg);
        if !receiver_is_disconnected(&result)
            && !self.complained_already.swap(true, Ordering::Relaxed)
        {
            // Complain
            result.unwrap();
        }
    }
}

fn receiver_is_disconnected<T>(result: &Result<(), NamedChannelTrySendError<T>>) -> bool {
    if let Err(e) = &result {
        matches!(e.try_send_error, TrySendError::Disconnected(_))
    } else {
        false
    }
}

impl<T> SenderToNormalThread<T> {
    /// Creates a bounded channel.
    ///
    /// - **Pro:** Never allocates when sending and is therefore safe to use from real-time threads.
    /// - **Con:** We can get "channel full" errors on load spikes if the capacity is not high
    ///   enough. Choosing an extremely high capacity to avoid this is not a good idea either
    ///   because it consumes memory that's almost never going to be used.
    pub fn new_bounded_channel(name: &'static str, capacity: usize) -> (Self, Receiver<T>) {
        let (sender, receiver) = crossbeam_channel::bounded(capacity);
        (
            Self {
                channel_name: name,
                sender,
                complained_already: AtomicBool::new(false),
            },
            receiver,
        )
    }

    /// Creates an unbounded channel.
    ///
    /// - **Pro:** We don't get "channel full" errors on load spikes.
    /// - **Con:** This can allocate when sending, so don't use this if the sender is used in
    ///   real-time threads! If you still do so, it will complain in debug mode because we forbid
    ///   allocation in real-time threads.
    ///
    /// We set a (very high) upper limit even for unbounded channels just to avoid memory exhaustion
    /// if the channel grows endlessly because of another error. This limit is not ensured by
    /// pre-allocating the channel with a certain capacity but by checking the current number
    /// of messages in the channel before sending.
    pub fn new_unbounded_channel(name: &'static str) -> (Self, Receiver<T>) {
        let (sender, receiver) = crossbeam_channel::unbounded();
        (
            Self {
                channel_name: name,
                sender,
                complained_already: AtomicBool::new(false),
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
        if !self.is_bounded() {
            // The channel is not bounded but we still want to panic if the number of messages
            // in the channel is extremely high, to prevent memory exhaustion.
            let msg_count = self.sender.len();
            if msg_count > 1_000_000 {
                panic!(
                    "Unbounded channel {} is extremely full ({} messages). \
                    Not accepting new messages in order to prevent memory exhaustion.",
                    self.channel_name, msg_count
                );
            }
        }
        try_send_on_named_channel(&self.sender, self.channel_name, msg)
    }
}

impl<T> Clone for SenderToNormalThread<T> {
    fn clone(&self) -> Self {
        Self {
            channel_name: self.channel_name,
            sender: self.sender.clone(),
            complained_already: AtomicBool::new(false),
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
    complained_already: AtomicBool,
}

impl<T> Clone for SenderToRealTimeThread<T> {
    fn clone(&self) -> Self {
        Self {
            channel_name: self.channel_name,
            sender: self.sender.clone(),
            complained_already: AtomicBool::new(false),
        }
    }
}

impl<T> NamedChannelSender for SenderToRealTimeThread<T> {
    type Msg = T;

    fn send_if_space(&self, msg: T) {
        let _ = self.send_internal(msg);
    }

    fn send_complaining(&self, msg: T) {
        let result = self.send_internal(msg);
        if !receiver_is_disconnected(&result)
            && !self.complained_already.swap(true, Ordering::Relaxed)
        {
            // Complain
            result.unwrap();
        }
    }
}

impl<T> SenderToRealTimeThread<T> {
    pub fn new_channel(name: &'static str, capacity: usize) -> (Self, Receiver<T>) {
        let (sender, receiver) = crossbeam_channel::bounded(capacity);
        (
            Self {
                channel_name: name,
                sender,
                complained_already: AtomicBool::new(false),
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
