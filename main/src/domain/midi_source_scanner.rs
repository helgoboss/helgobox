use helgoboss_learn::{MidiSourceValue, RawMidiEvent, SourceCharacter};
use helgoboss_midi::{
    Channel, ControlChange14BitMessageScanner, ControllerNumber,
    PollingParameterNumberMessageScanner, RawShortMessage, ShortMessage, ShortMessageFactory,
    StructuredShortMessage, U7,
};
use reaper_medium::MidiInputDeviceId;
use std::cmp::Ordering;
use std::time::{Duration, Instant};

const MAX_CC_MSG_COUNT: usize = 10;
const MAX_CC_WAITING_TIME: Duration = Duration::from_millis(250);

#[derive(Debug)]
pub struct MidiScanner {
    // Scanners for more complex MIDI message types
    nrpn_scanner: PollingParameterNumberMessageScanner,
    cc_14_bit_scanner: ControlChange14BitMessageScanner,
    state: State,
    dev_id: Option<MidiInputDeviceId>,
}

impl Default for MidiScanner {
    fn default() -> Self {
        Self {
            nrpn_scanner: PollingParameterNumberMessageScanner::new(Duration::from_millis(1)),
            cc_14_bit_scanner: Default::default(),
            state: State::Initial,
            dev_id: None,
        }
    }
}

#[derive(Debug)]
enum State {
    Initial,
    WaitingForMoreCcMsgs(ControlChangeState),
}

#[derive(Debug)]
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

#[derive(Clone, PartialEq, Debug)]
pub struct MidiScanResult {
    pub value: MidiSourceValue<'static, RawShortMessage>,
    pub dev_id: Option<MidiInputDeviceId>,
    pub character: Option<SourceCharacter>,
}

impl MidiScanResult {
    pub fn new(
        value: MidiSourceValue<'static, RawShortMessage>,
        dev_id: Option<MidiInputDeviceId>,
        character: Option<SourceCharacter>,
    ) -> Self {
        Self {
            value,
            dev_id,
            character,
        }
    }

    /// This allocates!
    pub fn try_from_bytes(
        bytes: &[u8],
        dev_id: Option<MidiInputDeviceId>,
    ) -> Result<Self, &'static str> {
        let raw_event = RawMidiEvent::try_from_slice(0, bytes)?;
        // This allocates!
        let res = MidiScanResult {
            dev_id,
            value: {
                // We don't use this as feedback value.
                let feedback_address_info = None;
                MidiSourceValue::single_raw(feedback_address_info, raw_event)
            },
            character: None,
        };
        Ok(res)
    }
}

impl MidiScanner {
    pub fn feed_short(
        &mut self,
        msg: RawShortMessage,
        dev_id: Option<MidiInputDeviceId>,
    ) -> Option<MidiScanResult> {
        if let Some(nrpn_msg) = self.nrpn_scanner.feed(&msg)[0] {
            let res = self.feed(
                MidiSourceValue::<RawShortMessage>::ParameterNumber(nrpn_msg),
                dev_id,
            );
            if res.is_some() {
                return res;
            }
        }
        if let Some(cc14_msg) = self.cc_14_bit_scanner.feed(&msg) {
            let res = self.feed(
                MidiSourceValue::<RawShortMessage>::ControlChange14Bit(cc14_msg),
                dev_id,
            );
            if res.is_some() {
                return res;
            }
        }
        self.feed(MidiSourceValue::Plain(msg), dev_id)
    }

    fn feed(
        &mut self,
        source_value: MidiSourceValue<RawShortMessage>,
        dev_id: Option<MidiInputDeviceId>,
    ) -> Option<MidiScanResult> {
        // First encountered device ID rules.
        if self.dev_id.is_none() {
            self.dev_id = dev_id;
        }
        match &mut self.state {
            State::Initial => {
                if let MidiSourceValue::Plain(msg) = source_value {
                    if let StructuredShortMessage::ControlChange {
                        channel,
                        controller_number,
                        control_value,
                    } = msg.to_structured()
                    {
                        let mut cc_state = ControlChangeState::new(channel, controller_number);
                        cc_state.add_value(control_value);
                        self.state = State::WaitingForMoreCcMsgs(cc_state);
                        None
                    } else {
                        Some(MidiScanResult::new(
                            source_value.try_into_owned().ok()?,
                            dev_id,
                            None,
                        ))
                    }
                } else {
                    Some(MidiScanResult::new(
                        source_value.try_into_owned().ok()?,
                        dev_id,
                        None,
                    ))
                }
            }
            State::WaitingForMoreCcMsgs(cc_state) => {
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
                    self.guess_or_not()
                } else {
                    // Looks like in the meantime, the composite scanners ((N)RPN or
                    // 14-bit CC) have figured out that the combination is a composite
                    // message. This fixes https://github.com/helgoboss/helgobox/issues/95.
                    let res =
                        MidiScanResult::new(source_value.try_into_owned().ok()?, dev_id, None);
                    self.reset();
                    Some(res)
                }
            }
        }
    }

    pub fn poll(&mut self) -> Option<MidiScanResult> {
        for ch in 0..16 {
            if let Some(nrpn_msg) = self.nrpn_scanner.poll(Channel::new(ch)) {
                let source_value = MidiSourceValue::<RawShortMessage>::ParameterNumber(nrpn_msg);
                let res = self.feed(source_value, None);
                if res.is_some() {
                    return res;
                }
            }
        }
        self.guess_or_not()
    }

    pub fn reset(&mut self) {
        self.nrpn_scanner.reset();
        self.cc_14_bit_scanner.reset();
        self.state = State::Initial;
    }

    fn guess_or_not(&mut self) -> Option<MidiScanResult> {
        if let State::WaitingForMoreCcMsgs(cc_state) = &self.state {
            if cc_state.time_to_guess() {
                let guessed_result = guess(cc_state, self.dev_id);
                self.reset();
                Some(guessed_result)
            } else {
                None
            }
        } else {
            None
        }
    }
}

fn guess(cc_state: &ControlChangeState, dev_id: Option<MidiInputDeviceId>) -> MidiScanResult {
    let first_cc_msg = RawShortMessage::control_change(
        cc_state.channel,
        cc_state.controller_number,
        cc_state.values[0],
    );
    MidiScanResult {
        value: MidiSourceValue::Plain(first_cc_msg),
        dev_id,
        character: Some(guess_custom_character(
            &cc_state.values[0..cc_state.msg_count - 1],
        )),
    }
}

fn contains_direction_change(values: &[U7]) -> bool {
    #[derive(Copy, Clone, PartialEq)]
    enum Direction {
        Clockwise,
        CounterClockwise,
    }
    fn determine_direction(a: U7, b: U7) -> Option<Direction> {
        use Direction::*;
        use Ordering::*;
        match b.cmp(&a) {
            Greater => Some(Clockwise),
            Less => Some(CounterClockwise),
            Equal => None,
        }
    }
    let mut direction_so_far: Option<Direction> = None;
    for i in 1..values.len() {
        let new_direction = determine_direction(values[i - 1], values[i]);
        if new_direction.is_none() {
            continue;
        }
        if direction_so_far.is_none() {
            direction_so_far = new_direction;
            continue;
        }
        if new_direction != direction_so_far {
            return true;
        }
    }
    false
}

fn contains_consecutive_duplicates(values: &[U7]) -> bool {
    for i in 1..values.len() {
        if values[i] == values[i - 1] {
            return true;
        }
    }
    false
}

fn guess_custom_character(values: &[U7]) -> SourceCharacter {
    use SourceCharacter::*;
    // We don't just interpret 127 or 100 as button because we consider typical keyboard keys also
    // as buttons. They can be velocity-sensitive and therefore transmit any value.
    #[allow(clippy::if_same_then_else)]
    if values.len() == 1 {
        // Only one message received. Looks like a button has been pressed and not released.
        MomentaryButton
    } else if values.len() == 2 && values[1] == U7::MIN {
        // Two messages received and second message has value 0. Looks like a button has been
        // pressed and released.
        MomentaryButton
    } else {
        // Multiple messages received. Button character is ruled out already. Check continuity.
        if contains_direction_change(values) {
            // A direction change means it's very likely a (relative) encoder.
            guess_encoder_type(values)
        } else if contains_consecutive_duplicates(values) {
            if values.contains(&U7::MIN) {
                // For relative, zero means "don't do anything" - which is a bit pointless
                // to send. So it's probably an encoder which is
                // configured to transmit absolute values hitting
                // the lower boundary.
                RangeElement
            } else if values.contains(&U7::MAX) {
                // Here we rely on the fact that the user should turn clock-wise. So it
                // can't be relative type 1 because 127 means
                // decrement. It's also unlikely to be the
                // other relative types because this would happen with extreme acceleration
                // only. So it's probably an encoder which is configured to transmit
                // absolute values hitting the upper boundary.
                RangeElement
            } else {
                guess_encoder_type(values)
            }
        } else {
            // Was continuous without duplicates until now so it's probably a knob/fader.
            SourceCharacter::RangeElement
        }
    }
}

/// Unfortunately, encoder type 3 clockwise movement is not really distinguishable from 1 or 2.
/// So we won't support its detection.
fn guess_encoder_type(values: &[U7]) -> SourceCharacter {
    use SourceCharacter::*;
    match values[0].get() {
        1..=7 | 121..=127 => Encoder1,
        57..=71 => Encoder2,
        // The remaining values are supported but not so typical for encoders because they only
        // happen at high accelerations.
        _ => RangeElement,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod scanning {
        use super::*;
        use helgoboss_midi::test_util::{channel, control_change, nrpn_14_bit, u14};
        use helgoboss_midi::{ParameterNumberMessage, ParameterNumberMessageScanner};

        #[test]
        fn scan_nrpn() {
            // Given
            let mut source_scanner = MidiScanner::default();
            let mut nrpn_scanner = ParameterNumberMessageScanner::new();
            // When
            use MidiSourceValue::{ParameterNumber, Plain};
            // Message 1
            let msg_1 = control_change(1, 99, 0);
            let nrpn_1 = nrpn_scanner.feed(&msg_1);
            assert_eq!(nrpn_1, None);
            let source_1 = source_scanner.feed(Plain(msg_1), None);
            // Message 2
            let msg_2 = control_change(1, 98, 99);
            let nrpn_2 = nrpn_scanner.feed(&msg_2);
            let source_2 = source_scanner.feed(Plain(msg_1), None);
            assert_eq!(nrpn_2, None);
            // Message 3
            let msg_3 = control_change(1, 38, 3);
            let nrpn_3 = nrpn_scanner.feed(&msg_3);
            assert_eq!(nrpn_3, None);
            let source_3 = source_scanner.feed(Plain(msg_3), None);
            // Message 4
            let msg_4 = control_change(1, 6, 2);
            let nrpn_4 = nrpn_scanner.feed(&msg_4).unwrap();
            assert_eq!(
                nrpn_4,
                ParameterNumberMessage::non_registered_14_bit(channel(1), u14(99), u14(259))
            );
            let source_4_nrpn = source_scanner.feed(ParameterNumber(nrpn_4), None);
            let source_4_short = source_scanner.feed(Plain(msg_4), None);
            // Then
            // Even our source scanner is already waiting for more CC messages with the same number,
            // a suddenly arriving (N)RPN message should take precedence! Because our real-time
            // processor constantly scans for (N)RPN, it would detect at some point that this looks
            // like a valid (N)RPN message. This needs to happen *before* the 250 millis
            // MAX_CC_WAITING_TIME have expired. In practice this is always the case because there
            // should never be much delay between the single messages making up one (N)RPN message.
            assert_eq!(source_1, None);
            assert_eq!(source_2, None);
            assert_eq!(source_3, None);
            assert_eq!(
                source_4_nrpn.unwrap(),
                MidiScanResult {
                    value: MidiSourceValue::ParameterNumber(nrpn_14_bit(1, 99, 259)),
                    dev_id: None,
                    character: None
                }
            );
            assert_eq!(source_4_short, None);
        }
    }

    mod source_character_guessing {
        use super::*;
        use helgoboss_midi::test_util::u7;
        use SourceCharacter::*;

        #[test]
        fn typical_range() {
            assert_eq!(guess(&[40, 41, 42, 43, 44]), RangeElement);
        }

        #[test]
        fn typical_range_counter_clockwise() {
            assert_eq!(guess(&[44, 43, 42, 41, 40]), RangeElement);
        }

        #[test]
        fn typical_trigger_button() {
            assert_eq!(guess(&[100]), MomentaryButton);
            assert_eq!(guess(&[127]), MomentaryButton);
        }

        #[test]
        fn typical_switch_button() {
            assert_eq!(guess(&[100, 0]), MomentaryButton);
            assert_eq!(guess(&[127, 0]), MomentaryButton);
        }

        #[test]
        fn typical_encoder_1() {
            assert_eq!(guess(&[1, 1, 1, 1, 1]), Encoder1);
        }

        #[test]
        fn typical_encoder_2() {
            assert_eq!(guess(&[65, 65, 65, 65, 65]), Encoder2);
        }

        #[test]
        fn typical_encoder_2_counter_clockwise() {
            assert_eq!(guess(&[63, 63, 63, 63, 63]), Encoder2);
        }

        #[test]
        fn velocity_sensitive_trigger_button() {
            assert_eq!(guess(&[79]), MomentaryButton);
            assert_eq!(guess(&[10]), MomentaryButton);
        }

        #[test]
        fn velocity_sensitive_switch_button() {
            assert_eq!(guess(&[79, 0]), MomentaryButton);
            assert_eq!(guess(&[10, 0]), MomentaryButton);
        }

        #[test]
        fn range_with_gaps() {
            assert_eq!(guess(&[40, 42, 43, 46]), RangeElement);
        }

        #[test]
        fn range_with_gaps_counter_clockwise() {
            assert_eq!(guess(&[44, 41, 40, 37, 35]), RangeElement);
        }

        #[test]
        fn very_lower_range() {
            assert_eq!(guess(&[0, 1, 2, 3]), RangeElement);
        }

        #[test]
        fn lower_range() {
            assert_eq!(guess(&[1, 2, 3, 4]), RangeElement);
        }

        #[test]
        fn very_upper_range_counter_clockwise() {
            assert_eq!(guess(&[127, 126, 125, 124]), RangeElement);
        }

        #[test]
        fn upper_range_counter_clockwise() {
            assert_eq!(guess(&[126, 125, 124, 123]), RangeElement);
        }

        #[test]
        fn encoder_1_with_acceleration() {
            assert_eq!(guess(&[1, 2, 2, 1, 1]), Encoder1);
        }

        #[test]
        fn encoder_1_with_acceleration_counter_clockwise() {
            assert_eq!(guess(&[127, 126, 126, 127, 127]), Encoder1);
        }

        #[test]
        fn encoder_1_with_more_acceleration() {
            assert_eq!(guess(&[1, 2, 5, 5, 2]), Encoder1);
        }

        #[test]
        fn encoder_1_with_more_acceleration_counter_clockwise() {
            assert_eq!(guess(&[127, 126, 122, 122, 126]), Encoder1);
        }

        #[test]
        fn encoder_2_with_acceleration() {
            assert_eq!(guess(&[65, 66, 66, 65, 65]), Encoder2);
        }

        #[test]
        fn encoder_2_with_acceleration_counter_clockwise() {
            assert_eq!(guess(&[63, 62, 62, 63, 63]), Encoder2);
        }

        #[test]
        fn encoder_2_with_more_acceleration() {
            assert_eq!(guess(&[65, 66, 68, 68, 66]), Encoder2);
        }

        #[test]
        fn encoder_2_with_more_acceleration_counter_clockwise() {
            assert_eq!(guess(&[63, 62, 59, 59, 62]), Encoder2);
        }

        #[test]
        fn absolute_encoder_hitting_upper_boundary() {
            assert_eq!(guess(&[127, 127, 127, 127, 127]), RangeElement);
            assert_eq!(guess(&[125, 126, 127, 127, 127]), RangeElement);
        }

        #[test]
        fn absolute_encoder_hitting_lower_boundary_counter_clockwise() {
            assert_eq!(guess(&[0, 0, 0, 0, 0]), RangeElement);
            assert_eq!(guess(&[2, 1, 0, 0, 0]), RangeElement);
        }

        #[test]
        fn lower_range_with_duplicate_elements() {
            assert_eq!(guess(&[0, 0, 1, 1, 2, 2]), RangeElement);
        }

        #[test]
        fn lower_range_with_duplicate_elements_counter_clockwise() {
            assert_eq!(guess(&[2, 2, 1, 1, 0, 0]), RangeElement);
        }

        #[test]
        fn neutral_zone_range_with_duplicate_elements() {
            assert_eq!(guess(&[37, 37, 37, 38, 38, 38, 39, 39]), RangeElement);
        }

        #[test]
        fn neutral_zone_range_with_duplicate_elements_counter_clockwise() {
            assert_eq!(guess(&[100, 100, 100, 99, 99, 99, 98, 98]), RangeElement);
        }

        fn guess(values: &[u8]) -> SourceCharacter {
            let u7_values: Vec<_> = values.iter().map(|v| u7(*v)).collect();
            guess_custom_character(&u7_values)
        }
    }
}
