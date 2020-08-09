use helgoboss_learn::{MidiSource, MidiSourceValue, SourceCharacter};
use helgoboss_midi::{
    Channel, ControllerNumber, RawShortMessage, ShortMessage, StructuredShortMessage, U7,
};
use std::cmp::Ordering;
use std::time::{Duration, Instant};

const MAX_CC_MSG_COUNT: usize = 10;
const MAX_CC_WAITING_TIME: Duration = Duration::from_millis(250);

enum State {
    Initial,
    WaitingForMoreCcMsgs(ControlChangeState),
}

pub struct MidiSourceScanner {
    state: State,
}

impl Default for MidiSourceScanner {
    fn default() -> Self {
        Self {
            state: State::Initial,
        }
    }
}

struct ControlChangeState {
    start_time: Instant,
    channel: Channel,
    controller_number: ControllerNumber,
    msg_count: usize,
    values: [U7; MAX_CC_MSG_COUNT],
}

impl ControlChangeState {
    fn new(channel: Channel, controller_number: ControllerNumber) -> ControlChangeState {
        ControlChangeState {
            start_time: Instant::now(),
            channel,
            controller_number,
            msg_count: 0,
            values: [U7::MIN; MAX_CC_MSG_COUNT],
        }
    }

    fn add_value(&mut self, value: U7) {
        assert!(self.msg_count < MAX_CC_MSG_COUNT);
        self.values[self.msg_count] = value;
        self.msg_count += 1;
    }

    fn time_to_guess(&self) -> bool {
        self.msg_count >= MAX_CC_MSG_COUNT || Instant::now() - self.start_time > MAX_CC_WAITING_TIME
    }

    fn matches(&self, channel: Channel, controller_number: ControllerNumber) -> bool {
        channel == self.channel && controller_number == self.controller_number
    }
}

impl MidiSourceScanner {
    pub fn feed(&mut self, source_value: MidiSourceValue<RawShortMessage>) -> Option<MidiSource> {
        use State::*;
        match &mut self.state {
            Initial => {
                if let MidiSourceValue::Plain(msg) = source_value {
                    if let StructuredShortMessage::ControlChange {
                        channel,
                        controller_number,
                        control_value,
                    } = msg.to_structured()
                    {
                        let mut cc_state = ControlChangeState::new(channel, controller_number);
                        cc_state.add_value(control_value);
                        self.state = WaitingForMoreCcMsgs(cc_state);
                        None
                    } else {
                        MidiSource::from_source_value(source_value)
                    }
                } else {
                    MidiSource::from_source_value(source_value)
                }
            }
            WaitingForMoreCcMsgs(cc_state) => {
                if let MidiSourceValue::Plain(msg) = source_value {
                    if let StructuredShortMessage::ControlChange {
                        channel,
                        controller_number,
                        control_value,
                    } = msg.to_structured()
                    {
                        if cc_state.matches(channel, controller_number) {
                            cc_state.add_value(control_value);
                        }
                    }
                }
                self.guess_or_not()
            }
        }
    }

    pub fn poll(&mut self) -> Option<MidiSource> {
        self.guess_or_not()
    }

    pub fn reset(&mut self) {
        self.state = State::Initial;
    }

    fn guess_or_not(&mut self) -> Option<MidiSource> {
        if let State::WaitingForMoreCcMsgs(cc_state) = &self.state {
            if cc_state.time_to_guess() {
                let guessed_source = guess_source(cc_state);
                self.reset();
                Some(guessed_source)
            } else {
                None
            }
        } else {
            None
        }
    }
}

fn guess_source(cc_state: &ControlChangeState) -> MidiSource {
    MidiSource::ControlChangeValue {
        channel: Some(cc_state.channel),
        controller_number: Some(cc_state.controller_number),
        custom_character: guess_custom_character(cc_state.msg_count, &cc_state.values),
    }
}

fn guess_custom_character(count: usize, values: &[U7; MAX_CC_MSG_COUNT]) -> SourceCharacter {
    use SourceCharacter::*;
    #[allow(clippy::if_same_then_else)]
    if count == 1 {
        // Only one message received. Looks like a switch has been pressed and not released.
        Switch
    } else if count == 2 && values[1] == U7::MIN {
        // Two messages received and second message has value 0. Looks like a switch has been
        // pressed and released.
        Switch
    } else {
        // Multiple messages received. Switch character is ruled out already. Check continuity.
        let mut prev_ord = Ordering::Equal;
        for i in 1..count {
            let current_ord = values[i - 1].cmp(&values[i]);
            if current_ord == Ordering::Equal {
                // Same value twice. Not continuous so it's probably an encoder.
                return guess_encoder_type(values);
            }
            if i > 1 && current_ord != prev_ord {
                // Direction changed. Not continuous so it's probably an encoder.
                return guess_encoder_type(values);
            }
            prev_ord = current_ord
        }
        // Was continuous until now so it's probably a knob/fader
        SourceCharacter::Range
    }
}

fn guess_encoder_type(values: &[U7; MAX_CC_MSG_COUNT]) -> SourceCharacter {
    use SourceCharacter::*;
    match values[0].get() {
        1..=7 | 121..=127 => Encoder1,
        57..=71 => Encoder2,
        _ => Encoder3,
    }
}
