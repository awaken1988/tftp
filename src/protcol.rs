use std::ops::Range;
use std::path::Path;
use std::time::{Duration, Instant};
use std::default::Default;

pub const DEFAULT_BLOCKSIZE: usize    = 512;
pub const MAX_BLOCKSIZE:     usize    = 1024;
pub const MAX_PACKET_SIZE:   usize    = MAX_BLOCKSIZE + DATA_BLOCK_NUM.end;
pub const RECV_TIMEOUT:      Duration = Duration::from_secs(2);
pub const OPCODE_LEN:        usize    = 2;
pub const ACK_LEN:           usize    = 4;
pub const DATA_OFFSET:       usize        = 4;
pub const DATA_BLOCK_NUM:    Range<usize> = 2..4;
pub const PACKET_SIZE_MAX:   usize    = 4096;

#[derive(Clone,Copy,Debug,PartialEq)]
pub enum Opcode {
    Read  = 1,
    Write = 2,
    Data  = 3,
    Ack   = 4,
    Error = 5,
}

#[allow(dead_code)]
#[derive(Clone,Copy,Debug)]
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
    buf:    &'a[u8],
    pos:    usize,
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

pub fn check_datablock(data: &[u8], expected: u16) -> bool {
    let mut parser = PacketParser::new(data);

    if !parser.opcode_expect(Opcode::Data) {
        return false;
    }
    
    let block_num = if let Some(block_num) = parser.number16() {
        block_num
    } else { 
        return false; 
    };

    if expected == block_num {
        return true;
    }
    
    
    return false;
}

// 
// pub type Checker<T,R> = fn(&[u8],&T) -> Option<R>;

// pub fn poll<T,R>(buf: &mut Vec<u8>, reader: &mut Reader, checker: &mut Checker<T,R>, timeout: Duration) -> Option<R> {
//     let start = Instant::now();

//     loop {
//         let time_diff = timeout.checked_sub(start.elapsed()).unwrap_or(Duration::from_secs(0));

//         if time_diff.is_zero() {
//             break;
//         }
        
//         buf.clear();
//         if !reader(buf, time_diff) {
//             continue;
//         }

//         if let Some(x) = checker(&buf) {
//             return Some(x);
//         }
//     }

//     return Option::None;
// }

// pub fn expect_block_data(data: &[u8], block_num: &u16) -> Option<()> {
//     let mut parser = PacketParser::new(data);

//     let opcode = if let Some(opcode) = parser.opcode() {
//         match opcode {
//             Opcode::Data => opcode,
//             _            => return Option::None,
//         }
//     } else {return Option::None};

//     let num = if let Some(num) = parser.number16() {
//         num
//     } else {return Option::None;};


//     if num == *block_num {
//         return Some(())
//     }

//     return Option::None;
// }

// pub fn poll_block_ack(buf: &mut Vec<u8>, reader: &mut Reader, timeout: Duration, block_number: u16) -> bool {
//     if let Some(_) = poll::<u16,()>(buf, &mut reader, &mut expect_block_data, timeout) {
//         return true;
//     }
//     else {
//         return false;
//     }
// }



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

    // pub fn separator(&mut self) -> bool {
    //     let data = self.remaining_bytes();

    //     if data.len() == 0 || data[0] != 0 {
    //       false
    //     }
    //     else {
    //         true
    //     } 
    // }

    // pub fn str_with_sep(&mut self) -> Option<String> {
    //     let data = self.remaining_bytes();
    //     let mut sepos = Option::None;
        
    //     for (i, d) in data.iter().enumerate() {
    //         if *d == 0 {
    //             sepos = Some(i)
    //         }
    //     }

    //     if sepos.is_none() {
    //         return Option::None;
    //     }

    //     let raw = &data[..sepos.unwrap()];
    //     let txt = String::from_utf8(raw.to_vec()).ok();

    //     if let Some(_) = txt {
    //         self.pos += raw.len();
    //     }

    //     return txt;
    // }

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
    // pub fn new(number: ErrorNumber) -> ErrorResponse {
    //     ErrorResponse {
    //         number: number,
    //         msg: None
    //     }
    // }

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

pub fn parse_opcode(raw: u16) -> Option<Opcode> {
    match raw {
        x if x == Opcode::Read  as u16 => Some(Opcode::Read),
        x if x == Opcode::Write as u16 => Some(Opcode::Write),
        x if x == Opcode::Data  as u16 => Some(Opcode::Data),
        x if x == Opcode::Ack   as u16 => Some(Opcode::Ack),
        x if x == Opcode::Error as u16 => Some(Opcode::Error),
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

pub fn parse_entries(data: &[u8]) -> Option<Vec<Vec<u8>>> {
    let mut ret: Vec<Vec<u8>> = vec![];

    if data.len() <= (OPCODE_LEN+1) {
        return None;
    };

    let entry_data = &data[OPCODE_LEN..];

    let mut next: Vec<u8> = vec![];
    for i in entry_data {
        if *i == 0x00 {
            ret.push(next.clone());
            next.resize(0, 0);
        }
        else {
            next.push(*i);
        }
    }

    if !next.is_empty() {
        return None;
    }
  
    return Some(ret);
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

    pub fn as_bytes(&self) -> &[u8] {
        &self.buf
    }
}




//TODO: function not required anymore but why does it not work
//pub fn num_to_raw<T>(number: T) -> Vec<u8>
//    where 
//        T: Copy + std::ops::Shr<usize,Output = T> + Into<u8>,
//{
//    let len = std::mem::size_of::<T>();
//
//    let mut ret: Vec<u8> = Vec::new();
//    for i in 0..len {
//        let val_shifted = number>>(i*8);
//        ret.push(Into::<u8>::into(val_shifted));
//    }
//
//    return ret;
//}