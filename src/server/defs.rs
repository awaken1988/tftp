use std::{sync::{mpsc::Sender, Arc, Mutex}, thread::JoinHandle, collections::{HashMap}, path::PathBuf, time::Duration};

pub const CLEANUP_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Clone,PartialEq)]
pub enum WriteMode {
    Disabled,
    WriteNew,
    WriteOverwrite,
}

#[derive(Clone)]
pub struct ServerSettings {
    pub write_mode:       WriteMode,
    pub root_dir:         String,
    pub blocksize:        usize,
    pub verbose:          bool,
    pub exit_with_client: bool,
    pub port:             u16,
}

pub struct ClientState {
    pub tx: Sender<Vec<u8>>,
    pub join_handle: Option<JoinHandle<()>>,
}

pub enum FileLockMode {
    Read(usize),
    Write,
}

pub type FileLockMap = Arc<Mutex<HashMap<PathBuf,FileLockMode>>>;

