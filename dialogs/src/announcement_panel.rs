use crate::base::*;
use crate::constants::{ANNOUNCEMENT_PANEL_WIDTH, FOOTER_PANEL_HEIGHT};

pub fn create(context: ScopedContext, ids: &mut IdGenerator) -> Dialog {
    use Style::*;
    Dialog {
        id: ids.named_id("ID_ANNOUNCEMENT_PANEL"),
        kind: DialogKind::DIALOGEX,
        rect: context.rect(0, 0, ANNOUNCEMENT_PANEL_WIDTH, FOOTER_PANEL_HEIGHT),
        styles: Styles(vec![DS_SETFONT, DS_CONTROL, WS_CHILD]),
        controls: vec![],
        ..context.default_dialog()
    }
}
