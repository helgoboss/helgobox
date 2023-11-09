use crate::base::*;

pub fn create(context: ScopedContext, ids: &mut IdGenerator) -> Dialog {
    use Style::*;
    Dialog {
        id: ids.named_id("ID_EMPTY_PANEL"),
        optional: true,
        caption: "Editor",
        rect: context.rect(0, 0, 600, 250),
        styles: Styles(vec![
            // Places the window into the center by default
            DS_CENTER,
            // Displays a close button
            WS_SYSMENU,
            // Displays a maximize button
            WS_MAXIMIZEBOX,
            // Allows user to change size of window
            WS_THICKFRAME,
        ]),
        ..context.default_dialog()
    }
}
