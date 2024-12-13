use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Bio {
    pub offset: u64,
    pub size: u64,
    pub is_metadata: bool,
    pub is_flush: bool,
    pub is_write: bool,
    pub start: i64,
    pub end: Option<i64>,
    pub stack_trace: usize,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum SyscallKind {
    Fsync,
    Write(Write)
}


#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Write{
    pub offset: u64,
    pub bytes: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Syscall {
    pub kind: SyscallKind,
    pub start: i64,
    pub end: Option<i64>,
    pub tid: u64,
    pub stats: Option<SyscallStats>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SyscallStats {
    pub write_sectors: u64,
    pub flushes: u64,
    pub frac_io_time: f64,
}