// Attention: We can't calculate a constant main panel height at this point because different
// scaling factors will be applied to the header panel, depending on the operating system.
pub const MAIN_PANEL_WIDTH: u32 = 470;
pub const HEADER_PANEL_HEIGHT: u32 = 124;
pub const HEADER_PANEL_WIDTH: u32 = MAIN_PANEL_WIDTH;
// Need to leave some space for the scrollbar.
pub const MAPPING_ROW_PANEL_WIDTH: u32 = MAIN_PANEL_WIDTH - 10;
pub const MAPPING_ROW_PANEL_HEIGHT: u32 = 48;
pub const FOOTER_PANEL_HEIGHT: u32 = 43;
pub const MAPPING_ROW_COUNT: u32 = 5;
pub const MAPPING_ROWS_PANEL_WIDTH: u32 = MAIN_PANEL_WIDTH;
pub const MAPPING_ROWS_PANEL_HEIGHT: u32 = MAPPING_ROW_PANEL_HEIGHT * MAPPING_ROW_COUNT;
