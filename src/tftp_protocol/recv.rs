use super::*;

pub enum RecvCallbackArg<'a> {
    WriteSink(&'a [u8]),
    Ack(&'a [u8]),
    Recv(&'a mut Vec<u8>, Duration),
}

pub struct RecvController<'a> {
    windowssize:      usize,
    blksize:          usize,
    callback:         Box<dyn FnMut(RecvCallbackArg) + 'a>,
    acked:            u16,
    window_buf:       Vec<Option<Vec<u8>>>, //TODO: use ringbuffer
    ack_buf:          Vec<u8>,
}

impl<'a> RecvController<'a> {

    pub fn new(windowsize: usize, blksize: usize, callback: Box<dyn FnMut(RecvCallbackArg) + 'a>) -> RecvController<'a> {
        RecvController {
            windowssize: windowsize,
            blksize: blksize,
            callback: callback,
            acked: 0,
            window_buf: vec![None; windowsize],
            ack_buf: vec![0;MAX_PACKET_SIZE],
        }
    }

    pub fn run(&mut self) -> Result<(), String> {
        let mut  bufs:  Vec<Option<Vec<u8>>> = vec![None; self.windowssize];

        loop { 
            self.fill_window()?;
            if self.write_window() { return Ok(()); }
        }
    }

    fn write_window(&mut self) -> bool {
        let (write_count, is_last) = self.is_complete();

        if write_count == self.windowssize || is_last {
            //println!("write_window={}; is_last={}; windowsize={}; acked={}", write_count,is_last, self.windowssize, self.acked);

            for i_write in 0..write_count {
                let data = RecvCallbackArg::WriteSink(&(self.window_buf[i_write].as_ref().unwrap()));
                (self.callback)(data);
            }
            for i_write in 0..write_count {
                self.window_buf.remove(0);
                self.window_buf.push(None);
            }

            self.incr_send_ack(write_count);

            return is_last;
        }

        return false;
    }

    fn fill_window(&mut self) -> Result<(), String> {
        let mut buf: Vec<u8> = Vec::new();
        
        for i_retry in 0..RETRY_COUNT {
            if i_retry > 0 && self.acked > 0 {
                buf.clear();
                self.resend_ack();
            }

            buf.clear();
            (self.callback)(RecvCallbackArg::Recv(&mut buf, RECV_TIMEOUT));

            if buf.is_empty() {continue;}

            //parse packet
            let mut pp = PacketParser::new(&buf);
            let is_data = pp.opcode_expect(Opcode::Data);
            if !is_data {continue;}

            let blocknr = if let Some(blocknr) = pp.number16() {blocknr} else {continue;};
            let data = pp.remaining_bytes();

            //fit blocknummer in our windows
            let diff = ring_diff(self.acked, blocknr); 
            if diff > self.windowssize || diff == 0 { continue; }
            let idx = diff.overflowing_sub(1).0;

            if self.window_buf[idx].is_some() {
                continue;
            }

            self.window_buf[idx] = Some(data.to_owned());

            return Ok(());
        }
        
        return Err("timeout".into());
    }

    fn send_ack(&mut self, blocknr: u16) {
        PacketBuilder::new(&mut self.ack_buf)
            .opcode(Opcode::Ack)
            .number16(blocknr);
        (self.callback)(RecvCallbackArg::Ack(&self.ack_buf));
    }

    fn incr_send_ack(&mut self, window_count: usize) {
        self.acked = self.acked.overflowing_add(window_count as u16).0 ;

        //println!("incr_send_ack acked={}", self.acked);
        self.send_ack(self.acked);
    }

    fn resend_ack(&mut self) {
        self.send_ack(self.acked);
    }

    fn is_complete(&self) -> (usize,bool) {
        let mut ready_blocks = 0;
        let mut is_last = false;

        for i in &self.window_buf {
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

// pub fn test_run() {
// let mut x = 0;
// {
//     // let mut ctrl = RecvController {
//     //     windowssize: 1,
//     //     blksize: 1,
//     //     write_fn: Box::new(|data: &[u16]|{
//     //         println!("write {:?}", data);
//     //     }),
//     //     ack_fn: Box::new(|blk: u16| {
//     //         println!("ack {}", blk);
//     //         x += 1;
//     //     })
//     // };

//     // ctrl.run();
// }

// println!("x {}", x)
// }