use crate::base::*;

pub fn create(context: ScopedContext, ids: &mut IdGenerator) -> Dialog {
    use Style::*;
    Dialog {
        id: ids.named_id("ID_COLOR_PANEL"),
        rect: context.rect(0, 0, 250, 250),
        styles: Styles(vec![DS_SETFONT, DS_CONTROL, WS_CHILD, WS_VISIBLE]),
        ..context.default_dialog()
    }
}
