// Uncomment to hide the terminal on Windows
#![windows_subsystem = "windows"]
use env_logger::Env;

use dragit::dnd;

fn main() {
    let env = Env::default().filter_or("LOG_LEVEL", "info");
    env_logger::init_from_env(env);
    dnd::start_window();
}
