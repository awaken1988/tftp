use std::collections::HashMap;
use std::ffi::OsString;
use std::io::{Read, Write};
use std::net::{SocketAddr, UdpSocket};
use std::ops::DerefMut;
use std::time::Instant;
use std::{sync::mpsc::Receiver, time::Duration};
use std::str::{self, FromStr};
use std::path::{Path, PathBuf};
use std::fs::File;
use std::path;


use crate::server::defs::{ServerSettings,WriteMode,FileLockMap, FileLockMode};
use crate::protcol::*;

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
    blksize:      u16,
    windowsize:   u16,
}

pub struct ParsedRequest {
    opcode:            Opcode, 
    filename:          String , 
    mode:              TransferMode, 
    options_extension: HashMap<String,String>,
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

    fn wait_ack(&mut self, timeout: Duration, blocknr: u16) -> Result<()> {
        let mut timeout = Timeout::new(timeout);

        loop {
            if timeout.is_timeout() {
                break;
            }

            let     data      = &self.recv.recv_timeout(Duration::from_secs(4)).unwrap()[..];
            let mut pp = PacketParser::new(&data);

            if data.len() != ACK_LEN || !pp.opcode_expect(Opcode::Ack) || !pp.number16_expected(blocknr)  {
                continue;
            } else {
                return Ok(());
            }         
        }

        return Result::Err(ErrorResponse::new_custom("timeout wait ACK".to_string()));
    }

    fn wait_data(&mut self, timeout: Duration, blocknr: u16, out: &mut Vec<u8>) -> Result<()> {
        let now = Instant::now();

        loop {
           if now.elapsed() > timeout {
            break;
           }

            let     data      = &self.recv.recv_timeout(Duration::from_secs(100)).unwrap()[..];
            let mut pp = PacketParser::new(&data);

            if !pp.opcode_expect(Opcode::Data) || !pp.number16_expected(blocknr) {
                continue;
            }


            let recv_data    = &data[DATA_OFFSET..];
            out.clear();
            out.extend_from_slice(recv_data);
            return Ok(());
        }

        return Result::Err(ErrorResponse::new_custom("timeout wait DATA".to_string()));
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
            println!("WARN: {:?} double unlock file = {:?}", self.remote, path);
        }
    }

    fn read(&mut self, filename: &str) -> Result<()> {
        println!("INFO: {:?} Read file {}", self.remote, filename);

        let full_path     = self.get_file_path(filename)?;

        if !self.check_lock_file(&full_path, FileLockMode::Read(1)) {
            return Err(ErrorResponse::new_custom("file is locked".to_string()));
        }

        let mut file = match File::open(&full_path) {
            Err(_)      => return Err(ErrorNumber::NotDefined.into()),
            Ok(x) => x,
        };

        let mut filebuf: Vec<u8> = vec![];
        let mut sendbuf: Vec<u8> = vec![];
        let mut blocknr: u16     = 1;

        filebuf.resize(self.settings.blocksize,0);
        
        loop {
            let payload_len = match file.read(&mut filebuf[0..self.settings.blocksize]) {
                Ok(len) => len,
                _ => return Err(ErrorResponse::new_custom("cannot read file".to_string())),
            };
            
            //send data
            sendbuf.resize(0, 0);
            sendbuf.extend_from_slice(&Opcode::Data.raw());
            sendbuf.push( (blocknr>>8) as u8 );
            sendbuf.push( (blocknr>>0) as u8 );
            self.bytecount += payload_len;

            if payload_len > 0 {
                sendbuf.extend_from_slice(&filebuf[0..payload_len]);
            }

            let _ = self.socket.send_to(&sendbuf, self.remote);

            //wait for ACK
            self.wait_ack(RECV_TIMEOUT, blocknr)?;

            //break condition
            if payload_len < self.settings.blocksize {
                break;
            }

            blocknr = blocknr.overflowing_add(1).0;
        }       

        return Ok(());
    }

    fn write(&mut self, filename: &str) -> Result<()> {
        println!("INFO: {:?} Write file {}", self.remote, filename);

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

        let mut file = match File::create(&full_path) {
            Err(_)      => return Err(ErrorNumber::NotDefined.into()),
            Ok(x) => x,
        };  

        let mut block_num = 0u16;
        let mut data: Vec<u8> = vec![];
        self.send_ack(block_num);
        loop {
            block_num = block_num.wrapping_add(1);
            
            self.wait_data(RECV_TIMEOUT, block_num, &mut data)?;

            self.bytecount += data.len();

            match file.write(&data) {
                Err(_) => return Err(ErrorResponse::new_custom("write to file error".to_string())),
                Ok(_) => {},
            }
            
            self.send_ack(block_num);

            if data.len() < self.settings.blocksize {
                break;
            }
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
            blksize:      DEFAULT_BLOCKSIZE  as u16,
            windowsize:   DEFAULT_WINDOWSIZE as u16,
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

        let mode = if let Ok(mode) = 
            parser.string_with_separator()
            .map_or(Err(()), |a| TransferMode::from_str(&a).to_owned())      
        {
            mode
        } else {
            return Err(ErrorResponse::new_custom("invalid mode".to_string()));
        };

        let mut options_extension = HashMap::new();

        while parser.remaining_bytes().len() > 0 {
            let (opt_name, opt_value) = match (parser.string_with_separator(), parser.string_with_separator()) {
                (Some(x), Some(y)) => (x, y),
                _ => return Err(ErrorResponse::new_custom("invalid option format".to_string())),
            };

            match opt_name.as_str() {
                BLKSIZE_STR => {
                    if let Ok(num) = u16::from_str_radix(&opt_value, 10) {
                        self.blksize = num;
                        options_extension.insert(opt_name, opt_value);
                    }
                },
                WINDOW_STR => {
                    if let Ok(num) = u16::from_str_radix(&opt_value, 10) {
                        self.windowsize = num;
                        options_extension.insert(opt_name, opt_value);
                    }
                },
                _ => println!("INFO: {:?} unkown option {}={} ignored", self.remote, opt_name, opt_name),
            };
        };
        
        return Ok(ParsedRequest {
            opcode: opcode,
            filename: filename,
            mode: mode,
            options_extension: options_extension,
        });
    }

    fn handle_extendes_request(&mut self, extended_request: &HashMap<String,String>) {
        if extended_request.is_empty() {
            return;
        }

        //send OACK
        let mut builder = PacketBuilder::new(self.buf.as_mut().unwrap()).opcode(Opcode::Oack);

        for option in extended_request {
            builder = builder.str(&option.0).separator().str(&option.1).separator();
        }  

        let buf = self.buf.take().unwrap();
        self.send_raw_release(buf);
    }

    pub fn run(&mut self)  {
        let data   = &self.recv.recv_timeout(RECV_TIMEOUT).unwrap()[..];
   
        let request = match self.parsed_request(data) {
            Ok(request) => request,
            Err(err) => {
                println!("ERR:  {:?} {}", self.remote, err.to_string());
                self.send_error(&err);
                return;
            }
        };

       

        let opcode = request.opcode;
        let filename = request.filename;
        println!("INFO: {:?}; {:?} {}", self.remote, request.opcode, &filename);

        self.handle_extendes_request(&request.options_extension);

        let result = match opcode {
            Opcode::Read  => self.read(&filename),
            Opcode::Write => self.write(&filename),
            _             => return 
        };

        match result {
            Err(err) => {
                println!("ERR:  {:?} {}", self.remote, err.to_string());
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
        println!("INFO: {:?} {:?} runtime = {}s; speed = {}MiB/s", self.remote, opcode, runtime, mib_s );

    }    
}