pub fn os_name() -> &'static str {
    "linux"
}

pub fn accessibility_trusted() -> bool {
    false
}

pub fn open_accessibility_settings() -> Result<(), String> {
    Err("Opening Linux accessibility settings is not implemented yet.".into())
}

pub fn mouse_click(_x: f64, _y: f64, _button: &str) -> Result<(), String> {
    unsupported()
}

pub fn mouse_move(_x: f64, _y: f64) -> Result<(), String> {
    unsupported()
}

pub fn scroll(_x: f64, _y: f64, _dy: i32) -> Result<(), String> {
    unsupported()
}

pub fn keystroke(_text: &str) -> Result<(), String> {
    unsupported()
}

pub fn shortcut(_key: &str, _modifiers: &[String]) -> Result<(), String> {
    unsupported()
}

fn unsupported() -> Result<(), String> {
    Err("GUI control is not supported on this operating system yet.".into())
}
