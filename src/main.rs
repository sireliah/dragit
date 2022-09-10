// Uncomment to hide the terminal on Windows
// #![windows_subsystem = "windows"]
use std::env;

use env_logger::Env;
use log::info;

use dragit::dnd;

fn main() {
    let env = Env::default().filter_or("LOG_LEVEL", "info");
    let app_name = env::var("APPLICATION_NAME").unwrap_or("com.sireliah.Dragit".to_string());

    env_logger::init_from_env(env);

    info!("Starting {}", app_name);
    dnd::start_window(app_name);
}
