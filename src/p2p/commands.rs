#[derive(Debug)]
pub enum TransferCommand {
    Accept(String),
    Deny(String),
}
