use crate::base::eel;
use helgoboss_learn::{
    create_raw_midi_events_singleton, AbsoluteValue, FeedbackValue, MidiSourceAddress,
    MidiSourceScript, MidiSourceScriptOutcome, RawMidiEvent,
};
use std::borrow::Cow;

use std::sync::Arc;

#[derive(Debug)]
struct EelUnit {
    // Declared above VM in order to be dropped before VM is dropped.
    program: eel::Program,
    vm: eel::Vm,
    y: eel::Variable,
    msg_size: eel::Variable,
    address: eel::Variable,
}

#[derive(Clone, Debug)]
pub struct EelMidiSourceScript {
    // Arc because EelUnit is not cloneable
    eel_unit: Arc<EelUnit>,
}

impl EelMidiSourceScript {
    pub fn compile(eel_script: &str) -> Result<Self, String> {
        if eel_script.trim().is_empty() {
            return Err("script empty".to_string());
        }
        let vm = eel::Vm::new();
        let program = vm.compile(eel_script)?;
        let y = vm.register_variable("y");
        let msg_size = vm.register_variable("msg_size");
        let address = vm.register_variable("address");
        let eel_unit = EelUnit {
            program,
            vm,
            y,
            msg_size,
            address,
        };
        Ok(Self {
            eel_unit: Arc::new(eel_unit),
        })
    }
}

impl MidiSourceScript for EelMidiSourceScript {
    fn execute(
        &self,
        input_value: FeedbackValue,
    ) -> Result<MidiSourceScriptOutcome, Cow<'static, str>> {
        let y_value = match input_value {
            // TODO-medium Find a constant for this which is defined in EEL
            FeedbackValue::Off => f64::MIN,
            FeedbackValue::Numeric(v) => match v.value {
                AbsoluteValue::Continuous(v) => v.get(),
                AbsoluteValue::Discrete(f) => f.actual() as f64,
            },
            // Rest not supported for EEL
            _ => f64::MIN,
        };
        let (slice, address) = unsafe {
            self.eel_unit.y.set(y_value);
            self.eel_unit.msg_size.set(0.0);
            self.eel_unit.address.set(0.0);
            self.eel_unit.program.execute();
            let msg_size = self.eel_unit.msg_size.get().round() as i32;
            if msg_size < 0 {
                return Err("invalid message size".into());
            };
            let slice = self.eel_unit.vm.get_mem_slice(0, msg_size as u32);
            let address = self.eel_unit.address.get();
            (slice, address)
        };
        if slice.is_empty() {
            return Err("empty message".into());
        }
        let mut array = [0; RawMidiEvent::MAX_LENGTH];
        let mut i = 0u32;
        for byte in slice.iter().take(RawMidiEvent::MAX_LENGTH) {
            array[i as usize] = byte.round() as u8;
            i += 1;
        }
        let raw_midi_event = RawMidiEvent::new(0, i, array);
        let outcome = MidiSourceScriptOutcome {
            address: if address == 0.0 {
                None
            } else {
                Some(MidiSourceAddress::Script {
                    bytes: address as u64,
                })
            },
            events: create_raw_midi_events_singleton(raw_midi_event),
        };
        Ok(outcome)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use helgoboss_learn::{FeedbackStyle, NumericFeedbackValue, UnitValue};

    #[test]
    fn basics() {
        // Given
        let text = "
            address = 0x4bb0;
            msg_size = 3;
            0[] = 0xb0;
            1[] = 0x4b;
            2[] = y * 10;
        ";
        let script = EelMidiSourceScript::compile(text).unwrap();
        // When
        let fb_value = NumericFeedbackValue::new(
            FeedbackStyle::default(),
            AbsoluteValue::Continuous(UnitValue::new(0.5)),
        );
        let outcome = script.execute(FeedbackValue::Numeric(fb_value)).unwrap();
        // Then
        assert_eq!(
            outcome.address,
            Some(MidiSourceAddress::Script { bytes: 0x4bb0 })
        );
        assert_eq!(
            outcome.events,
            vec![RawMidiEvent::try_from_slice(0, &[0xb0, 0x4b, 5]).unwrap()]
        );
    }
}
