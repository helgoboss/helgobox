use crate::base::*;

pub fn create(context: ScopedContext, ids: &mut IdGenerator) -> Dialog {
    use Style::*;
    let controls = vec![
        pushbutton(
            "Open in text editor",
            ids.named_id("ID_YAML_TEXT_EDITOR_BUTTON"),
            context.rect(371, 291, 68, 14),
        ),
        edittext(
            ids.named_id("ID_YAML_EDIT_CONTROL"),
            context.rect(0, 0, 490, 284),
        ) + ES_MULTILINE
            + ES_WANTRETURN
            + WS_VSCROLL,
        pushbutton(
            "Help",
            ids.named_id("ID_YAML_HELP_BUTTON"),
            context.rect(445, 291, 40, 14),
        ),
        ltext(
            "",
            ids.named_id("ID_YAML_EDIT_INFO_TEXT"),
            context.rect(5, 294, 355, 9),
        ) + NOT_WS_GROUP,
    ];
    Dialog {
        id: ids.named_id("ID_YAML_EDITOR_PANEL"),
        caption: "Editor",
        rect: context.rect(0, 0, 490, 310),
        styles: Styles(vec![
            DS_SETFONT,
            DS_MODALFRAME,
            DS_3DLOOK,
            DS_FIXEDSYS,
            DS_CENTER,
            WS_POPUP,
            WS_VISIBLE,
            WS_CAPTION,
            WS_SYSMENU,
        ]),
        controls,
        ..context.default_dialog()
    }
}
