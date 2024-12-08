#[derive(Debug)]
pub struct Bio {
    pub offset: u64,
    pub size: u64,
    pub is_metadata: bool,
    pub is_flush: bool,
    pub is_write: bool,
    pub start: u64,
    pub end: Option<u64>,
}