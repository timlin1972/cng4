use once_cell::sync::Lazy;
use std::sync::Mutex;

const DEFFAULT_SYS_NAME: &str = "default";
const DEFFAULT_SERVER_NAME: &str = "default";

struct Global {
    pub sys_name: String,
    pub server: String,
    pub server_ip: Option<String>,
}
static SYS_INFO: Lazy<Mutex<Global>> = Lazy::new(|| {
    Mutex::new(Global {
        sys_name: DEFFAULT_SYS_NAME.to_string(),
        server: DEFFAULT_SERVER_NAME.to_string(),
        server_ip: None,
    })
});

pub fn get_sys_name() -> String {
    let g = SYS_INFO.lock().unwrap();
    g.sys_name.clone()
}

pub fn set_sys_name(name: &str) {
    let mut g = SYS_INFO.lock().unwrap();
    g.sys_name = name.to_string();
}

pub fn get_server() -> String {
    let g = SYS_INFO.lock().unwrap();
    g.server.clone()
}

pub fn set_server(name: &str) {
    let mut g = SYS_INFO.lock().unwrap();
    g.server = name.to_string();
}

pub fn get_server_ip() -> Option<String> {
    let g = SYS_INFO.lock().unwrap();
    g.server_ip.clone()
}

pub fn set_server_ip(ip: &str) {
    let mut g = SYS_INFO.lock().unwrap();
    g.server_ip = Some(ip.to_string());
}
