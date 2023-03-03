mod recv;
mod send;

pub use recv::*;
pub use send::*;

use std::collections::HashMap;
use std::ops::Range;
use std::str::FromStr;
use std::time::{Duration, Instant};
use std::default::Default;

use num_traits::FromPrimitive;


pub const DEFAULT_BLOCKSIZE:  usize            = 512;
pub const DEFAULT_WINDOWSIZE: usize            = 1;
pub const MAX_BLOCKSIZE:      usize            = 1024;
pub const MAX_PACKET_SIZE:    usize            = MAX_BLOCKSIZE + DATA_BLOCK_NUM.end;

pub const SEND_RECV_BLOCK_TIMEOUT:  Duration   = Duration::from_millis(1000);
pub const RESEND_TIMEOUT:           Duration   = Duration::from_millis(2000);
pub const RECV_TIMEOUT:             Duration   = Duration::from_millis(6500);
pub const RECV_ACK_TIMEOUT:         Duration   = Duration::from_millis(2500);

pub const RETRY_COUNT:              usize      = 3;                 //rename to MAX_RETRIES

pub const OPCODE_LEN:         usize            = 2;
pub const ACK_LEN:            usize            = 4;
pub const DATA_OFFSET:        usize            = 4;
pub const DATA_BLOCK_NUM:     Range<usize>     = 2..4;
pub const PACKET_SIZE_MAX:    usize            = 4096;
pub const BLKSIZE_STR:        &str             = "blksize";
pub const WINDOW_STR:         &str             = "windowsize";

#[derive(Clone,Copy,Debug,PartialEq, FromPrimitive,ToPrimitive)]
pub enum Opcode {
    Read  = 1,
    Write = 2,
    Data  = 3,
    Ack   = 4,
    Error = 5,
    Oack  = 6,
}

#[allow(dead_code)]
#[derive(Clone,Copy,Debug, FromPrimitive,ToPrimitive)]
pub enum ErrorNumber {
    NotDefined           = 0,
    FileNotFound         = 1,
    AccessViolation      = 2,
    DiskFull             = 3,
    IllegalOperation     = 4,
    UnknownTransferID    = 5,
    FileAlreadyExists    = 6,
    NoSuchUser           = 7,
}

pub struct ErrorResponse {
    pub number: ErrorNumber,
    pub msg:    Option<String>,
}

#[allow(dead_code)]
#[derive(Clone,Copy,Debug)]
pub enum TransferMode {
    Netascii,
    Octet,
    Mail,
}

pub struct PacketBuilder<'a> {
    buf: &'a mut Vec<u8>,
}

pub struct PacketParser<'a> {
    pub buf:    &'a[u8],
    pub pos:    usize,
}

pub struct Timeout {
    start:   Option<Instant>,
    timeout: Duration,
}

impl Timeout {
    pub fn new(timeout: Duration) -> Self {
        Timeout { start: Option::None, timeout: timeout }
    }

    pub fn reset(&mut self) {
        self.start = Option::None;
    }

    pub fn is_timeout(&mut self) -> bool {
        if let Some(start) = self.start {
            if start.elapsed() < self.timeout {
                return false;
            } else {
                return true;
            }
        }
        
        self.start = Some(Instant::now());
        return false;
    }
}

impl<'a> PacketParser<'a> {
    pub fn new(buf: &'a[u8]) -> Self {
        PacketParser {
            buf: buf,
            pos: 0,
        }
    }

    pub fn remaining_bytes(&self) -> &'a[u8] {
        &self.buf[self.pos..]
    }

    pub fn opcode(&mut self) -> Option<Opcode> {
        let result = parse_opcode_raw(self.remaining_bytes());
     
        if result.is_some() {
            self.pos += OPCODE_LEN;
        }
        
        return result;
    }

    pub fn opcode_expect(&mut self, opcode: Opcode) -> bool {
        if let Some(parsed) = self.opcode() {
            return parsed == opcode;
        }

        return false;
    }

    pub fn string_with_separator(&mut self) -> Option<String> {
        let data = self.remaining_bytes();

        let mut read_data = Vec::new();

        for i_digit in data.iter() {
            if i_digit == &0 {
                self.pos += read_data.len() + 1;
                return String::from_utf8(read_data).ok()
            }
            read_data.push(*i_digit);
        }

        return Option::None;
    }

    pub fn extended_options(&mut self) -> Result<HashMap<String,String>,()> {
        let mut ret = HashMap::new();
        let mut lastkey: Option<String> = Option::None;

        for i in self.remaining_bytes().split(|x| x == &0) {
            let field = if let Ok(x) = String::from_utf8(i.into()) {x} 
                else {return Err(())};
            
            if let Some(ref x) = lastkey {
                ret.insert(x.clone(), field);
                let _ = lastkey.take();
            }  else {
                lastkey = Some(field);
            }
        }    
        
        //TODO: check for the last 0

        return Ok(ret);
    }

    pub fn number16(&mut self) -> Option<u16> {
        let data = self.remaining_bytes();

        if let Some(num) = raw_to_num::<u16>(data) {
            self.pos += 2;
            return Some(num);
        } else { 
            return Option::None; 
        };
    }

    pub fn number16_expected(&mut self, num: u16) -> bool {
        if let Some(x) = self.number16() {
            return x == num;
        }
        
        return false;
    }

    pub fn parse_error(&mut self) -> Option<ErrorResponse> {
        let undef_error = ErrorResponse::new_custom("response error with invalid error number".to_owned());

        if !self.opcode_expect(Opcode::Error) {
            return None;    
        }

      
        let err = if let Some(err_raw) = self.number16() {
            if let Some(err) = ErrorNumber::from_u16(err_raw) 
                {err}
            else { return Some(undef_error) }
        } else {return Some(undef_error)};

        let err_str = if let Some(err_str) = self.string_with_separator() {
            err_str
        } else {
            "".to_owned()
        };

        return Some(match err {
            ErrorNumber::NotDefined => { ErrorResponse::new_custom(err_str)},
            _                       => { ErrorResponse::from(err) }
        });
    }
}



impl ToString for ErrorNumber {
    fn to_string(&self) -> String {
        return match *self {
            ErrorNumber::NotDefined          => "Not defined".to_string(),
            ErrorNumber::FileNotFound        => "File not found.".to_string(),
            ErrorNumber::AccessViolation     => "Access violation.".to_string(),
            ErrorNumber::DiskFull            => "Disk full or allocation exceeded.".to_string(),
            ErrorNumber::IllegalOperation    => "Illegal TFTP operation.".to_string(),
            ErrorNumber::UnknownTransferID   => "Unknown transfer ID.".to_string(),
            ErrorNumber::FileAlreadyExists   => "File already exists.".to_string(),
            ErrorNumber::NoSuchUser          => "No such user.".to_string(),
        };    
    }
}

impl ToString for ErrorResponse {
    fn to_string(&self) -> String {
        if let Some(msg) = &self.msg {
            msg.clone()
        }
        else {
           self.number.to_string() 
        }
    }
}

impl  ErrorResponse {
    pub fn new_custom(msg: String) -> ErrorResponse {
        ErrorResponse {
            number: ErrorNumber::NotDefined,
            msg: Some(msg)
        }
    }
}



impl From<ErrorNumber> for ErrorResponse {
    fn from(number: ErrorNumber) -> Self {
        ErrorResponse { number: number, msg: Option::None }
    }
}



pub fn raw_to_num<T: Copy + From<u8> + core::ops::BitOrAssign + core::ops::Shl<usize,Output=T>+Default>(data: &[u8]) -> Option<T> {
    let outlen = std::mem::size_of::<T>();
    if outlen > data.len() {
        return None
    }

    let mut out: T = Default::default();
    for i in 0..outlen {
        let curr = T::from(data[i]);
        out |= curr  << ((outlen-1-i)*8);
    }

    return Some(out);
}

impl Opcode {
    pub fn raw(self) -> [u8; OPCODE_LEN] {
       return (self as u16).to_be_bytes();
    }
}

impl From<Opcode> for u16 {
    fn from(value: Opcode) -> Self {
        return value as u16;
    }
}

impl ToString for TransferMode {
    fn to_string(&self) -> String {
       return match *self {
        TransferMode::Netascii => "netascii".to_string(),
        TransferMode::Octet    => "octet".to_string(),
        TransferMode::Mail     => "mail".to_string(),
        }
    }
}

impl FromStr for TransferMode {
    type Err = ();

    fn from_str(s: &str) -> Result<Self,Self::Err> {
             if s == "netascii" { Ok(TransferMode::Netascii)}
        else if s == "octet"    { Ok(TransferMode::Octet)}
        else if s == "mail"     { Ok(TransferMode::Mail)}
        else {return Err(())}
    }
}


pub fn parse_opcode(raw: u16) -> Option<Opcode> {
    match raw {
        x if x == Opcode::Read  as u16 => Some(Opcode::Read),
        x if x == Opcode::Write as u16 => Some(Opcode::Write),
        x if x == Opcode::Data  as u16 => Some(Opcode::Data),
        x if x == Opcode::Ack   as u16 => Some(Opcode::Ack),
        x if x == Opcode::Error as u16 => Some(Opcode::Error),
        x if x == Opcode::Oack  as u16 => Some(Opcode::Oack),
        _ => None,
    }
}

pub fn parse_opcode_raw(data: &[u8]) -> Option<Opcode> {
    let num =   match raw_to_num::<u16>(data) {
        Some(num) => num,
        None          => return None
    };
    return parse_opcode(num);
}

impl<'a> PacketBuilder<'a> {
    pub fn new(buf: &'a mut Vec<u8>) -> PacketBuilder {
        buf.clear();
        PacketBuilder {
            buf: buf,
        }
    }

    pub fn opcode(self, opcode: Opcode) -> Self {
        self.buf.extend_from_slice(&opcode.raw());
        return self;
    }

    pub fn transfer_mode(self, mode: TransferMode) -> Self {
        return self.str(&mode.to_string())   //Note: prevent create a String
    }
    
    pub fn separator(self) -> Self {
        self.buf.push(0);
        return self;
    }
    
    pub fn str(self, txt: &str) -> Self {
        self.buf.extend_from_slice(txt.as_bytes());
        return self;
    }

    pub fn number16(self, num: u16) -> Self {
        self.buf.extend_from_slice(&num.to_be_bytes());
        return self;
    }

    pub fn raw_data(self, data: &[u8]) -> Self {
        self.buf.extend_from_slice(data);
        return self;
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.buf
    }
}

pub struct ExtendedOptions {
    pub blksize:    u16,
    pub windowsize: u16,
}

impl ExtendedOptions {
    pub fn new() -> ExtendedOptions {
        ExtendedOptions {
            blksize:    DEFAULT_BLOCKSIZE  as u16,
            windowsize: DEFAULT_WINDOWSIZE as u16,
        }
    }
}

pub fn filter_extended_options(options: &HashMap<String,String>) -> Result<(ExtendedOptions, HashMap<String,String>), ()> {
    let mut known  = ExtendedOptions::new();
    let mut unknown = HashMap::new();
    
    for (name,value) in options {
        match name.as_str() {
            BLKSIZE_STR => {
                known.blksize    = if let Ok(x) = u16::from_str_radix(&value, 10) {x} else {return Err(());};
            },
            WINDOW_STR  => {
                known.windowsize = if let Ok(x) = u16::from_str_radix(&value, 10) {x} else {return Err(());};
            },
            _                 => {
                unknown.insert(name.clone(), value.clone());
            } 
        };
    }

    return Ok((known, unknown));
}

fn ring_diff(a: u16, b: u16) -> usize {
    return if a <= b {
        a.abs_diff(b) as usize
    } else {
        (u8::MAX as usize + b as usize + 1) - a as usize
    };
}

//RecvStateMachine
//  is deprecated; use RecvController as soon as it stable
pub struct RecvStateMachine<'a> {
    windowssize:      usize,
    blksize:          usize,
    bufs:             Vec<Option<Vec<u8>>>,
    acked:            u16,
    is_end:           bool,
    is_timeout:       bool,
    timeout:          OneshotTimer,
    lost_ack_timeout: Option<Instant>,
    writer:           &'a mut dyn std::io::Write,
}

impl<'a> RecvStateMachine<'a> {
    pub fn new(writer: &'a mut dyn std::io::Write, blksize: usize, windowssize: usize) -> Self {
        RecvStateMachine {
            windowssize: windowssize,
            blksize: blksize,
            bufs: vec![None; windowssize],
            acked: 0,
            is_end: false,
            is_timeout: false,
            timeout: OneshotTimer::new(RECV_TIMEOUT),
            lost_ack_timeout: None,
            writer: writer
        }
    }

    pub fn is_end(&self) -> bool {
        return self.is_end;
    }

    pub fn is_timeout(&self) -> bool {
        return self.is_end && self.is_timeout;
    }

    pub fn insert_frame(&mut self, data: &[u8]) {
        let _ = self.timeout.is_timeout();

        let mut pp = PacketParser::new(data);
        let is_data = pp.opcode_expect(Opcode::Data);
        let blocknr = pp.number16().unwrap();
        let data = pp.remaining_bytes();

        if !is_data {return};

        let diff = ring_diff(self.acked, blocknr); 
        if diff > self.windowssize || diff == 0 { return; }
        
        let idx = diff.overflowing_sub(1).0;

        self.bufs[idx] = Some(Vec::from(data));
        self.lost_ack_timeout = None;

        self.timeout.explicit_start();
    }

    pub fn sync(&mut self) -> Option<u16> {
        let (ready_blocks, is_last) = self.is_complete();
        let is_blocks = ready_blocks > 0;
        let is_all_blocks = ready_blocks == self.windowssize || is_blocks && is_last;
        let is_timeout = self.timeout.is_timeout();

        self.is_end = is_last;

        if let Some(lost_ack_timeout) = self.lost_ack_timeout {
            if lost_ack_timeout.elapsed() > RECV_ACK_TIMEOUT {
                self.lost_ack_timeout = Some(Instant::now());
                return Some(self.acked)
            }
        }

        if !is_blocks && is_timeout {
            self.is_timeout = true;
            self.is_end     = true;
            return None;
        }
        
        if !is_timeout && !is_all_blocks {
            return None;
        }

        for i in 0..ready_blocks {
            self.writer.write(self.bufs[i].as_ref().unwrap().as_ref()).expect("write to file failed");
        }
        for _ in 0..ready_blocks {
            self.bufs.remove(0);
            self.bufs.push(None);
        }

        self.acked  = self.acked.overflowing_add(ready_blocks as u16).0;
        self.is_end = is_last;

        if !self.is_end {
            self.lost_ack_timeout = Some(Instant::now());
        }
    
        return Some(self.acked);
    }     

    fn is_complete(&self) -> (usize,bool) {
        if self.is_end { return (0, true); }

        let mut ready_blocks = 0;
        let mut is_last = false;

        for i in &self.bufs {
            if let Some(data) =  i {
                ready_blocks+=1;
                if data.len() < self.blksize {
                    is_last = true;
                    break;
                }
            } else {
                return (ready_blocks, is_last);
            }
        }

        return (ready_blocks, is_last);
    }
}










//TODO: move this to another place
#[derive(Debug)]
pub struct OneshotTimer {
    start:   Option<Instant>,
    timeout: Duration,
}

impl OneshotTimer {
    pub fn new(timeout: Duration) -> OneshotTimer {
        OneshotTimer { start: None, timeout: timeout }
    }

    pub fn explicit_start(&mut self) {
        self.start = Some(Instant::now());
    }

    pub fn is_timeout(&mut self) -> bool {
        if let Some(x) = self.start {
            return x.elapsed() > self.timeout;
        } else {
            self.explicit_start();
            return false;
        }
    }

    pub fn reset(&mut self) {
        self.start = None;
    }
}

