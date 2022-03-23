use reaper_high::Reaper;
use reaper_medium::{TranslateAccel, TranslateAccelArgs, TranslateAccelResult};

#[derive(Debug)]
pub struct RealearnAccelerator;

impl RealearnAccelerator {
    pub fn new() -> Self {
        Self
    }
}
// 2022-03-23T17:30:12.361144Z DEBUG realearn::domain::accelerator: Captured TranslateAccelArgs { msg: MSG { hwnd: 0xf50724, message: 256, wParam: 79, lParam: 1572865, time: 675186859, pt: POINT { x: 2906, y: 684 } }, ctx: AcceleratorRegister(0x14862f80) }
// Decrypt: O,
// Decrypt: O,
// Decrypt: O,
// Decrypt: O,
// Decrypt: Ctrl+Alt+O,
//
// 2022-03-23T17:30:12.361234Z DEBUG realearn::domain::accelerator: Captured TranslateAccelArgs { msg: MSG { hwnd: 0xf50724, message: 258, wParam: 111, lParam: 1572865, time: 675186859, pt: POINT { x: 2906, y: 684 } }, ctx: AcceleratorRegister(0x14862f80) }
// Decrypt: o,
// Decrypt: o,
// Decrypt: o,
// Decrypt: -,
// Decrypt: Ctrl+Alt+o,
//
// 2022-03-23T17:30:12.457135Z DEBUG realearn::domain::accelerator: Captured TranslateAccelArgs { msg: MSG { hwnd: 0xf50724, message: 257, wParam: 79, lParam: 3222798337, time: 675186953, pt: POINT { x: 2906, y: 684 } }, ctx: AcceleratorRegister(0x14862f80) }
// Decrypt: O,
// Decrypt: O,
// Decrypt: O,
// Decrypt: O,
// Decrypt: Ctrl+Alt+O,

impl TranslateAccel for RealearnAccelerator {
    fn call(&mut self, args: TranslateAccelArgs) -> TranslateAccelResult {
        let reaper = Reaper::get().medium_reaper();
        let high_w = hiword(args.msg.wParam);
        let low_w = loword(args.msg.wParam);
        let high_l = hiword(args.msg.lParam as _);
        let low_l = loword(args.msg.lParam as _);
        tracing_debug!(
            "\
            Captured {:?}\n\
            Decrypt: {},            
            Decrypt: {},            
            Decrypt: {},            
            Decrypt: {},            
            Decrypt: {},            
        ",
            &args,
            // Only low_w seems to matter. It can display keys such as A-Z, 0-9.
            reaper.kbd_format_key_name(0, low_w, high_w),
            reaper.kbd_format_key_name(0, low_w, low_l),
            reaper.kbd_format_key_name(0, low_w, high_l),
            // This seems to be most correct displays STRG, +, ... and also A-Z, 0-9.
            reaper.kbd_format_key_name(low_l as _, low_w, high_l),
            reaper.kbd_format_key_name(high_l as _, low_w, low_l),
        );

        TranslateAccelResult::NotOurWindow
    }
}

fn loword(wparam: usize) -> u16 {
    (wparam & 0xffff) as _
}

fn hiword(wparam: usize) -> u16 {
    ((wparam >> 16) & 0xffff) as _
}
