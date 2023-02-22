use std::ffi::OsString;
use std::net::{SocketAddr, UdpSocket};
use std::ops::DerefMut;
use std::time::{Instant, Duration};
use std::{sync::mpsc::Receiver};
use std::str::{self, FromStr};
use std::path::{Path, PathBuf};
use std::fs::File;
use std::path;


use crate::server::defs::{ServerSettings,WriteMode,FileLockMap, FileLockMode};

use crate::{protcol::*, tlog};

pub struct Connection {
    recv:         Receiver<Vec<u8>>,
    remote:       SocketAddr,
    socket:       UdpSocket,
    settings:     ServerSettings,
    start:        Instant,
    bytecount:    usize,
    lockmap:      FileLockMap,
    locked:       Option<PathBuf>,
    buf:          Option<Vec<u8>>,
}

pub struct ParsedRequest {
    opcode:            Opcode, 
    filename:          String , 
    //TODO: mode:              TransferMode, 
}

type Result<T> = std::result::Result<T,ErrorResponse>;

impl Connection {
    fn send_raw_release(&mut self, buf: Vec<u8>) {
        self.socket.send_to(&buf, self.remote).unwrap();
        self.buf = Some(buf);
    }

    fn send_error(&mut self, error: &ErrorResponse) {
        let mut buf = self.buf.take().unwrap();
        
        //TODO: to many allocations
        let err_str = if let Some(x) = error.msg.as_ref() {
            x.clone()
        } else {
            "unknown".to_string()
        };
        
        let _ = PacketBuilder::new(&mut buf)
            .opcode(Opcode::Error)
            .number16(error.number as u16)
            .str(&err_str)
            .separator()
            .as_bytes();
        self.send_raw_release(buf);
    }

    fn send_ack(&mut self, blocknr: u16) {
        let mut buf = self.buf.take().unwrap();
        let _ = PacketBuilder::new(&mut buf)
            .opcode(Opcode::Ack)
            .number16(blocknr)
            .as_bytes();

        self.send_raw_release(buf);
    }

    fn get_file_path(&self, path_relative: &str) -> Result<PathBuf> {
        let base_path    = OsString::from(&self.settings.root_dir);
        let request_path = OsString::from(&path_relative);
        let full_path     = Path::new(&base_path).join(request_path);

        if !full_path.starts_with(base_path) {
            return Err(ErrorNumber::FileNotFound.into());
        }

        return Ok(full_path.to_path_buf());
    }

    fn check_lock_file(&mut self, path: &Path, mode: FileLockMode) -> bool {
        let mut lockset = self.lockmap.lock().unwrap();
        let lockset = lockset.deref_mut();

        if let Some(curr) = lockset.get_mut(path) {
            let _ = match (mode,curr) {
               (FileLockMode::Read(_mode), FileLockMode::Read(curr))   => {
                    *curr += 1; 
                    self.locked = Some(path.to_path_buf());
                    return true;
                },    
              _ => {return false;},    
            };
        }
        else {
            lockset.insert(path.to_path_buf(), mode);
            return true;
        }
    }

    fn unlock_file(&mut self, path: &Path) {
        let mut lockset = self.lockmap.lock().unwrap();
        let lockset = lockset.deref_mut();

        if lockset.contains_key(path) {
            let mut is_remove = false;
            if let Some(FileLockMode::Read(x)) = lockset.get_mut(path) {
                *x -= 1;
                if *x == 0 {is_remove = true;}
            }
            else if let Some(FileLockMode::Write) = lockset.get(path) {
                is_remove = true;
            }

            if is_remove {
                lockset.remove(path);
            }
        }
        else {
            tlog::warning!("WARN: {:?} double unlock file = {:?}", self.remote, path);

            tlog::warning!("blablub {:?}", self.remote);
        }
    }

    fn download(&mut self, filename: &str) -> Result<()> {
        tlog::info!("{:?} Read file {}", self.remote, filename);

        let full_path     = self.get_file_path(filename)?;

        if !self.check_lock_file(&full_path, FileLockMode::Read(1)) {
            return Err(ErrorResponse::new_custom("file is locked".to_string()));
        }

        let mut file = match File::open(&full_path) {
            Err(_)      => return Err(ErrorNumber::NotDefined.into()),
            Ok(x) => x,
        };

        let blocksize  = self.settings.blocksize;
        let windowsize = self.settings.windowsize;

        let mut window_buffer = SendStateMachine::new(&mut file, blocksize, windowsize);

        while let action = window_buffer.next() {
            match action {
                SendAction::SendBuffer(bufs) => {
                    for i_frame in window_buffer.send_data() {
                        let _ = self.socket.send_to(i_frame, self.remote);
                    }
                },
                SendAction::Timeout => { return Err(ErrorResponse::new_custom("ack timeout".into()));  }
                SendAction::End => break,
                _ => {}
            }

            if let Ok(data) =  self.recv.recv_timeout(SEND_RECV_BLOCK_TIMEOUT) {
                window_buffer.ack_packet(&data);
            }        
        }

        self.bytecount = window_buffer.read_len();

        return Ok(())
    }

    fn open_upload_file(&mut self, filename: &str) -> Result<File> {
        if self.settings.write_mode == WriteMode::Disabled {
            return Err(ErrorNumber::AccessViolation.into());
        }

        let full_path     = self.get_file_path(filename)?;

        let is_file = path::Path::new(full_path.as_os_str()).exists();
        let is_overwrite = self.settings.write_mode == WriteMode::WriteOverwrite;

        if is_file && !is_overwrite {
            return Err(ErrorNumber::FileAlreadyExists.into());
        }

        if !self.check_lock_file(&full_path, FileLockMode::Write) {
            return Err(ErrorResponse::new_custom("file is locked".to_string()));
        }

        //TODO: use better varaint... like ok_or
        return match File::create(&full_path) {
            Err(_)      => Err(ErrorNumber::NotDefined.into()),
            Ok(file) => Ok(file),
        };  
    }

    fn upload(&mut self, filename: &str) -> Result<()> {
        tlog::info!("{:?} Write file {}", self.remote, filename);

        let timeout_msg = format!("upload timeout; path={}", filename).to_string();
        let mut file = self.open_upload_file(filename)?;
        let mut window_buffer = RecvStateMachine::new(&mut file, self.settings.blocksize, self.settings.windowsize);
        
        self.send_ack(0);

        while !window_buffer.is_end() {
            let recv = if let Ok(recv) = self.recv.recv_timeout(SEND_RECV_BLOCK_TIMEOUT) {
                recv
            } else { continue; };

            window_buffer.insert_frame(&recv);
            
            if let Some(ack) = window_buffer.sync() {
                self.send_ack(ack);
            }
        }

        if window_buffer.is_timeout() {
            return Err(ErrorResponse::new_custom(timeout_msg.clone()));
        }

        return Ok(());
    }

    pub fn new(recv: Receiver<Vec<u8>>, remote: SocketAddr, socket: UdpSocket, settings: ServerSettings, lockmap: FileLockMap) -> Connection {
        return Connection{
            recv:         recv,
            remote:       remote,
            socket:       socket,
            settings:     settings,
            start:        Instant::now(),
            bytecount:    0,
            lockmap,
            locked:       Option::None,
            buf:          Some(Vec::new()),
        };
    }

    fn parsed_request(&mut self, data: &[u8]) -> Result<ParsedRequest> {
        let mut parser = PacketParser::new(&data);

        let opcode = if let Some(opcode) = parser.opcode() {
            opcode
        } else {
            return Err(ErrorResponse::new_custom("invalid opcode".to_string()));
        };

        let filename = if let Some(filename) = parser.string_with_separator() {
            filename
        } else {
            return Err(ErrorResponse::new_custom("invalid filename".to_string()));
        };

        //TODO: make this more pretty which chaining
        //TODO: mode is currently ignored

        let _mode = if let Ok(mode) = 
            parser.string_with_separator()
            .map_or(Err(()), |a| TransferMode::from_str(&a).to_owned())      
        {
            mode
        } else {
            return Err(ErrorResponse::new_custom("invalid mode".to_string()));
        };

        if let Ok(recv_map) = parser.extended_options() {
            if let Ok((options,_other)) = filter_extended_options(&recv_map) {
                self.settings.blocksize  = options.blksize    as usize;
                self.settings.windowsize = options.windowsize as usize;
            }
            else {
                tlog::warning!("{:?} recv extended options but format invalid", self.remote);
            }
        }
        else {
            tlog::warning!("{:?} recv extended options but format invalid", self.remote);
        }
  
        return Ok(ParsedRequest {
            opcode: opcode,
            filename: filename,
            //TODO: mode: mode,
        });
    }

    fn handle_extendes_request(&mut self) {
        //send OACK
        let mut builder = PacketBuilder::new(self.buf.as_mut().unwrap()).opcode(Opcode::Oack);
        let mut is_oack = false;

        if self.settings.blocksize != DEFAULT_BLOCKSIZE {
            builder = builder.str(BLKSIZE_STR).separator().str(&self.settings.blocksize.to_string()).separator();
            is_oack = true;
        }
        if self.settings.windowsize != DEFAULT_WINDOWSIZE {
            builder = builder.str(WINDOW_STR).separator().str(&self.settings.windowsize.to_string()).separator();
            is_oack = true;
        }

        let _ = builder;

        if !is_oack {
            return;
        }

        let buf = self.buf.take().unwrap();
        self.send_raw_release(buf);
    }

    pub fn run(&mut self)  {
        let data   = &self.recv.recv_timeout(RECV_TIMEOUT).unwrap()[..];
   
        let request = match self.parsed_request(data) {
            Ok(request) => request,
            Err(err) => {
                tlog::error!("{:?} {}", self.remote, err.to_string());
                self.send_error(&err);
                return;
            }
        };

        let opcode = request.opcode;
        let filename = request.filename;
        tlog::info!("{:?}; {:?} {}", self.remote, request.opcode, &filename);

        self.handle_extendes_request();

        let result = match opcode {
            Opcode::Read  => self.download(&filename),
            Opcode::Write => self.upload(&filename),
            _             => return 
        };

        match result {
            Err(err) => {
                tlog::error!("{:?} {}", self.remote, err.to_string());
                self.send_error(&err);
            },
            _ => {},
        };

        //cleanup locks
        if let Some(ref locked) = self.locked.clone() {
            self.unlock_file(&locked);
        }

        //statistics
        let runtime = self.start.elapsed().as_secs_f32();
        let mib_s      = ((self.bytecount as f32) / runtime) / 1000000.0;
        tlog::info!("{:?} {:?} runtime = {}s; speed = {}MiB/s", self.remote, opcode, runtime, mib_s );

    }    
}