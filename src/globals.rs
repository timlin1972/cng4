use once_cell::sync::Lazy;
use std::sync::Mutex;

const DEFFAULT_SYS_NAME: &str = "default";

struct Global {
    pub sys_name: String,
}
static SYS_NAME: Lazy<Mutex<Global>> = Lazy::new(|| {
    Mutex::new(Global {
        sys_name: DEFFAULT_SYS_NAME.to_string(),
    })
});

pub fn get_sys_name() -> String {
    let g = SYS_NAME.lock().unwrap();
    g.sys_name.clone()
}

pub fn set_sys_name(name: &str) {
    let mut g = SYS_NAME.lock().unwrap();
    g.sys_name = name.to_string();
}
