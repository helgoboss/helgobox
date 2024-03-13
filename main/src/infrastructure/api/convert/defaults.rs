use realearn_api::persistence::Interval;

pub const MAPPING_CONTROL_ENABLED: bool = true;
pub const MAPPING_FEEDBACK_ENABLED: bool = true;
pub const MAPPING_ENABLED: bool = true;
pub const MAPPING_VISIBLE_IN_PROJECTION: bool = true;

pub const GROUP_CONTROL_ENABLED: bool = true;
pub const GROUP_FEEDBACK_ENABLED: bool = true;

pub const SOURCE_OSC_IS_RELATIVE: bool = false;
pub const SOURCE_MACKIE_LCD_EXTENDER_INDEX: u8 = 0;
pub const SOURCE_X_TOUCH_MACKIE_LCD_EXTENDER_INDEX: u8 = 0;

pub const UNIT_INTERVAL: Interval<f64> = Interval(0.0, 1.0);
pub const GLUE_STEP_SIZE_INTERVAL: Interval<f64> = Interval(0.01, 0.01);
// Should be the same as GLUE_STEP_SIZE_INTERVAL because the native data structure only saves one.
pub const GLUE_STEP_FACTOR_INTERVAL: Interval<i32> = Interval(1, 1);
pub const GLUE_SOURCE_INTERVAL: Interval<f64> = UNIT_INTERVAL;
pub const GLUE_TARGET_INTERVAL: Interval<f64> = UNIT_INTERVAL;
pub const GLUE_JUMP_INTERVAL: Interval<f64> = UNIT_INTERVAL;
pub const GLUE_REVERSE: bool = false;
pub const GLUE_WRAP: bool = false;
pub const GLUE_ROUND_TARGET_VALUE: bool = false;
pub const FIRE_MODE_PRESS_DURATION_INTERVAL: Interval<u32> = Interval(0, 0);
pub const FIRE_MODE_TIMEOUT: u32 = 0;
pub const FIRE_MODE_RATE: u32 = 0;
pub const FIRE_MODE_SINGLE_PRESS_MAX_DURATION: u32 = 0;

pub const TARGET_TRACK_MUST_BE_SELECTED: bool = false;
pub const TARGET_FX_MUST_HAVE_FOCUS: bool = false;
pub const TARGET_TRACK_SELECTED_ALLOW_MULTIPLE: bool = false;
pub const TARGET_BY_NAME_ALLOW_MULTIPLE: bool = false;
pub const TARGET_BOOKMARK_SET_TIME_SELECTION: bool = false;
pub const TARGET_BOOKMARK_SET_LOOP_POINTS: bool = false;
pub const TARGET_POLL_FOR_FEEDBACK: bool = true;
pub const TARGET_RETRIGGER: bool = false;
pub const TARGET_TRACK_SELECTION_SCROLL_ARRANGE_VIEW: bool = false;
pub const TARGET_TRACK_SELECTION_SCROLL_MIXER: bool = false;
pub const TARGET_SEEK_USE_TIME_SELECTION: bool = false;
pub const TARGET_SEEK_USE_LOOP_POINTS: bool = false;
pub const TARGET_SEEK_USE_REGIONS: bool = false;
pub const TARGET_SEEK_USE_PROJECT: bool = true;
pub const TARGET_SEEK_MOVE_VIEW: bool = true;
pub const TARGET_SEEK_SEEK_PLAY: bool = true;
pub const TARGET_LOAD_MAPPING_SNAPSHOT_ACTIVE_MAPPINGS_ONLY: bool = false;
pub const TARGET_SAVE_MAPPING_SNAPSHOT_ACTIVE_MAPPINGS_ONLY: bool = false;
pub const TARGET_STOP_COLUMN_IF_SLOT_EMPTY: bool = false;
pub const TARGET_USE_SELECTION_GANGING: bool = false;
pub const TARGET_USE_TRACK_GROUPING: bool = false;
