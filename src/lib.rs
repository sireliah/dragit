#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
    }
}

extern crate dbus;

pub mod bluetooth;
pub mod dnd;

pub use self::bluetooth::adapter;