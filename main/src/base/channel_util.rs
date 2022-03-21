use crossbeam_channel::{Sender, TrySendError};
use std::error::Error;
use std::fmt;
use std::fmt::{Debug, Display, Formatter};

pub fn try_send_on_named_channel<T>(
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
pub struct NamedChannelTrySendError<T> {
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
