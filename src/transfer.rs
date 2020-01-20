use std::error::Error;

pub trait Protocol {
    fn transfer_file(&self, path: &str) -> Result<(), Box<dyn Error>>;
}
