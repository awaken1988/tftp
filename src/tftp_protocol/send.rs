use super::*;

#[derive(Debug)]
pub enum SendAction<'a> {
    SendBuffer(&'a Vec<Vec<u8>>),
    NoOp,
    Timeout,
    End,
}

pub struct SendStateMachine<'a>
{
    windowssize:   usize,
    blksize:       usize,
    bufs:          Vec<Vec<u8>>,
    acked:         u16,
    new_acked:     bool,
    reader:        &'a mut dyn std::io::Read,
    is_reader_end: bool,
    is_end:        bool,
    timeout:       OneshotTimer,
    retry:         usize,
    data_read:     usize,
}

impl<'a> SendStateMachine<'a> {
    pub fn new(reader: &'a mut dyn std::io::Read, blksize: usize, windowssize: usize) -> SendStateMachine {
        SendStateMachine {
            windowssize: windowssize,
            blksize: blksize,
            bufs: vec![],
            acked: 0,
            new_acked: true,
            reader: reader,
            is_reader_end: false,
            is_end: false,
            timeout: OneshotTimer::new(RESEND_TIMEOUT),
            retry: RETRY_COUNT,
            data_read: 0,
        }
    }

    pub fn fill_level(&self) -> usize {
        return self.bufs.len();
    }

    pub fn send_data(&self) -> &Vec<Vec<u8>> {
        return &self.bufs;
    }

    pub fn next(&mut self) -> SendAction {
        //DELETE: println!("{:?} {:?} {:?} {:?}", self.is_reader_end, self.is_end, self.acked, self.new_acked);

        if self.is_end {
            return SendAction::End;
        }

        if !self.is_reader_end {
            self.impl_next();
        };

        if self.new_acked {
            self.new_acked  = false;
            return SendAction::SendBuffer(&self.bufs);
        }
        
        if self.timeout.is_timeout() {
            if self.retry == 0 {
                return SendAction::Timeout;
            }
            else {
                self.retry -= 1;
                return SendAction::SendBuffer(&self.bufs);
            }
        };

        return SendAction::NoOp;

    }

    pub fn read_len(&self) -> usize {
        return self.data_read;
    }

    fn impl_next(&mut self) {  
        for i in self.fill_level()..self.windowssize {
            let mut filebuf    = vec![0u8; self.blksize];
            let mut packet_buf = vec![0u8; MAX_BLOCKSIZE];

            let read_len  =  self.reader.read(filebuf.as_mut()).unwrap();   //TODO: make proper error handling

            //fill header
            let next_blknum = self.acked
                .overflowing_add(i as u16).0
                .overflowing_add(1).0;

            PacketBuilder::new(packet_buf.as_mut())
                .opcode(Opcode::Data)
                .number16((next_blknum) as u16)
                .raw_data(&filebuf[0..(read_len as usize)]);

            self.bufs.push(packet_buf);

            self.data_read += read_len;

            if read_len < self.blksize {
                self.is_reader_end = true;
                break;
            }
        } 
    }

    pub fn ack_packet(&mut self, frame: &[u8]) {
        let mut pp = PacketParser::new(frame);

        if frame.len() != ACK_LEN || !pp.opcode_expect(Opcode::Ack)  {
           return;
        }

        if let Some(num) = pp.number16() {
            self.ack(num);
        } 
    }

    pub fn ack(&mut self, blknum: u16) {
        let diff = ring_diff(self.acked, blknum);

        if diff > self.windowssize {
            return;
        }
        
        for _ in 0..diff {
            self.new_acked = true;
            self.bufs.remove(0);
            self.acked = self.acked.overflowing_add(1).0;
        }

        if self.is_reader_end && self.bufs.is_empty() {
            self.is_end = true;
        }

        if self.new_acked {
            self.timeout.reset();
            self.retry = RETRY_COUNT;
        }
        
    }

}
