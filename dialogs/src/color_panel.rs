use crate::base::*;

pub fn create(context: ScopedContext, ids: &mut IdGenerator) -> Dialog {
    use Style::*;
    Dialog {
        id: ids.named_id("ID_COLOR_PANEL"),
        rect: context.rect(0, 0, 250, 250),
        styles: Styles(vec![DS_SETFONT, DS_CONTROL, WS_CHILD, WS_VISIBLE]),
        // controls: vec![ltext(
        //     "Label",
        //     ids.named_id("ID_COLOR_PANEL_LABEL"),
        //     context.rect(5, 0, 100, 9),
        // )],
        ..context.default_dialog()
    }
}
