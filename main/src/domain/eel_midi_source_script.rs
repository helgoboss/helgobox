use crate::base::eel;
use helgoboss_learn::{AbsoluteValue, MidiSourceScript, RawMidiEvent};

use std::sync::Arc;

#[derive(Debug)]
struct EelUnit {
    // Declared above VM in order to be dropped before VM is dropped.
    program: eel::Program,
    vm: eel::Vm,
    y: eel::Variable,
    msg_size: eel::Variable,
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
        let eel_unit = EelUnit {
            program,
            vm,
            y,
            msg_size,
        };
        Ok(Self {
            eel_unit: Arc::new(eel_unit),
        })
    }
}

impl MidiSourceScript for EelMidiSourceScript {
    fn execute(&self, input_value: AbsoluteValue) -> Result<Box<RawMidiEvent>, &'static str> {
        let y_value = match input_value {
            AbsoluteValue::Continuous(v) => v.get(),
            AbsoluteValue::Discrete(f) => f.actual() as f64,
        };
        let slice = unsafe {
            self.eel_unit.y.set(y_value);
            self.eel_unit.msg_size.set(0.0);
            self.eel_unit.program.execute();
            let msg_size = self.eel_unit.msg_size.get().round() as i32;
            if msg_size < 0 {
                return Err("invalid message size");
            };
            self.eel_unit.vm.get_mem_slice(0, msg_size as u32)
        };
        if slice.is_empty() {
            return Err("empty message");
        }
        let mut array = [0; RawMidiEvent::MAX_LENGTH];
        let mut i = 0u32;
        for byte in slice.iter().take(RawMidiEvent::MAX_LENGTH) {
            array[i as usize] = byte.round() as u8;
            i += 1;
        }
        let raw_midi_event = RawMidiEvent::new(0, i, array);
        Ok(Box::new(raw_midi_event))
    }
}
