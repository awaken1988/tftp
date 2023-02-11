use std::{time::{Duration}, fs::File, io::{Write, Read}, path::{PathBuf}, str::FromStr, env, option};

use clap::ArgMatches;
use std::net::UdpSocket;
use crate::protcol::{Opcode,PacketBuilder, TransferMode, Timeout, RECV_TIMEOUT, check_datablock, self, DATA_OFFSET, DEFAULT_BLOCKSIZE, PACKET_SIZE_MAX, PacketParser, DEFAULT_WINDOWSIZE, BLKSIZE_STR, WINDOW_STR, filter_extended_options, RecvWindowBuffer};

struct ClientArguments {
    remote:     String,
    blksize:    usize,
    windowsize: usize,
}

impl ClientArguments {
    fn new(args: &ArgMatches) -> ClientArguments {
        ClientArguments {
            remote:  (args.get_one::<String>("remote").expect("invalid remote")).clone(),
            blksize: {
                if let Some(blksize ) = args.get_one::<String>("blksize") {
                    usize::from_str_radix(&*blksize, 10).expect("blksize value invalid")
                } else {
                    DEFAULT_BLOCKSIZE
                }
            },
            windowsize: {
                if let Some(windowsize ) = args.get_one::<String>("windowsize") {
                    usize::from_str_radix(&*windowsize, 10).expect("windowsize value invalid")
                } else {
                    DEFAULT_WINDOWSIZE
                }
            }
        }
    }
}

pub fn client_main(args: &ArgMatches) {
    let opcode = match (args.get_many::<String>("read"), args.get_many::<String>("write")) {
        (Some(_), None) => Opcode::Read,
        (None, Some(_)) => Opcode::Write,
        _               => panic!("invalid client action; only --read or --write possible")
    };

    let      paths             = get_connection_paths(opcode, args);
    let mut  client_arguments = ClientArguments::new(args);

    let mut socket = UdpSocket::bind("127.0.0.1:0").expect("Bind to interface failed");
    socket.connect(&client_arguments.remote).expect("Connection failed");

    let mut socket = SocketSendRecv::new(socket);

    send_initial_packet(opcode, &paths, &mut client_arguments, &mut socket);

    println!("ENDINIT");

    let mut timeout = Timeout::new(RECV_TIMEOUT);

    loop {
        if timeout.is_timeout() {
            break;
        }

        match opcode {
            Opcode::Read => {
                println!("local {:?}", &paths.local);
                let mut file = File::create(paths.local).expect("Cannot write file");
                read_action(&mut socket, &mut file, &client_arguments);
                break;
            }
            Opcode::Write => {
                let mut file = File::open(paths.local).expect("Cannot write file");
                write_action(&mut socket, &mut file, &client_arguments);
                break;
            }
            _ => panic!("not yet implemented"),
        }
    }


}


struct SocketSendRecv {
    socket:   UdpSocket,
    read_buf: Vec<u8>,
    defer:    bool,
}

impl std::io::Write for SocketSendRecv {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.send(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl SocketSendRecv {
    fn new(socket: UdpSocket) -> SocketSendRecv {
        SocketSendRecv {
            socket:    socket,
            read_buf:  Vec::new(),
            defer:     false,
        }
    }

    fn recv_next(&mut self) -> bool {
        if self.defer {
            self.defer = false;
            return true;
        }

        self.read_buf.resize(PACKET_SIZE_MAX, 0);
        let _           = self.socket.set_read_timeout(Some(Duration::from_secs(1))); 
        match self.socket.recv_from(&mut self.read_buf) {
            Ok((size, _)) =>  {
                self.read_buf.resize(size, 0);
                return true;
            }
            Err(_) => {
                self.read_buf.resize(0, 0);
                return false;
            }
        };
    }

    fn recv_buf(&self) -> &[u8] {
        return &self.read_buf;
    }

    fn send(&mut self, data: &[u8]) {
        self.socket.send(data).expect("ERR  : send tftp request failed");
    }

    fn defer_recv(&mut self) {
        self.defer = true;
    }



}


fn send_initial_packet(opcode: Opcode, paths: &ClientFilePath, args: &mut ClientArguments, socket: &mut SocketSendRecv) {
    //send initial packet
    {
        let mut buf = Vec::new();

        let mut pkg = PacketBuilder::new(&mut buf)
            .opcode(opcode)
            .str(paths.remote.clone().to_str().expect("invalid remote filepath"))
            .separator()
            .transfer_mode(TransferMode::Octet);
    
        if args.blksize != DEFAULT_BLOCKSIZE {
            pkg = pkg.separator().str(&BLKSIZE_STR).separator().str(&args.blksize.to_string());
        }
        if args.windowsize != DEFAULT_WINDOWSIZE {
            pkg = pkg.separator().str(&WINDOW_STR).separator().str(&args.windowsize.to_string());
        }
    
        pkg = pkg.separator();
    
        socket.send(pkg.as_bytes());
    }

    //try parse extended options
    {
        if !socket.recv_next() {
            return;
        }

        let mut pp = PacketParser::new(socket.recv_buf());

        if !pp.opcode_expect(Opcode::Oack) {
            socket.defer_recv();
            return;
        }

       

        if let Ok(recv_map) = pp.extended_options() {
            for (key,value) in &recv_map {
                println!("INFO: acknowledge {} = {}", key, value);
            }
 
            if let Ok((options,other)) = filter_extended_options(&recv_map) {
                args.blksize    = options.blksize    as usize;
                args.windowsize = options.windowsize as usize;
            }
            else {
                println!("WRN: recv extended options but format invalid");
            }
        }
        else {
            println!("WRN: recv extended options but format invalid");
        }
    }

}

struct ClientFilePath {
   local:  PathBuf,
   remote: PathBuf,
}

fn get_connection_paths(opcode: Opcode, args: &ArgMatches) -> ClientFilePath {
    let (values, remote_idx, local_idx) = match opcode {
        Opcode::Read  => (args.get_many::<String>("read"),  0, 1),
        Opcode::Write => (args.get_many::<String>("write"), 1, 0),
        _             => panic!("Invalid Operation: only Read or Write allowed"),
    };
    let values: Vec<&String> = values.unwrap().collect();

    //get from args
    let mut localfile = if let Some(l) = values.get(local_idx) {
        Some(PathBuf::from_str(l).unwrap())
    } else {
        Option::None
    };

    let mut remote = if let Some(r) = values.get(remote_idx) {
        Some(PathBuf::from_str(r).unwrap())
    } else {
        Option::None
    };

    //default missing
    if localfile.is_none() {
        localfile = Some(env::current_dir()
            .expect("cannot get current working directory")
            .join(remote.as_ref().unwrap().file_name().unwrap()))
    };
    if remote.is_none() {
        remote = Some(localfile.as_ref().unwrap().file_name().unwrap().into());
    };

    ClientFilePath {
        local:  localfile.unwrap(),
        remote: remote.unwrap(),
    }
}

fn check_readbuffer_full(buffers: &Vec<Vec<u8>>) {
    for i in buffers {
        
    }
}

fn read_action(socket: &mut SocketSendRecv, file: &mut File, arguments: &ClientArguments) {
    let mut timeout    =  Timeout::new(RECV_TIMEOUT);
    let mut expected_block = 1u16;
    let mut is_end        = false;

    let mut recvbuf: Vec<Vec<u8>> = vec![vec![]; arguments.windowsize];


    let mut window_buffer = RecvWindowBuffer::new(file, arguments.blksize, arguments.windowsize);

    while !window_buffer.is_end() {
        if !socket.recv_next() {continue;}
        
        window_buffer.insert_frame(socket.recv_buf());

        if let Some(ack_window) = window_buffer.sync() {
            let mut buf: Vec<u8>        = Vec::new();
            let _ = socket.send(PacketBuilder::new(&mut buf)
                .opcode(Opcode::Ack)
                .number16(ack_window).as_bytes());
        }        
    }

    //TODO: show timeout error
}

fn write_action(socket: &mut SocketSendRecv, file: &mut File, arguments: &ClientArguments) {
    let mut timeout    =  Timeout::new(RECV_TIMEOUT);
    let mut expected_ack = 0u16;
    let mut buf: Vec<u8>        = Vec::new();
    let mut filebuf: Vec<u8>    = Vec::new();
    let mut is_last       = false;

    filebuf.resize(protcol::MAX_BLOCKSIZE, 0);

    while !is_last {
        if timeout.is_timeout() {
            panic!("timeout");
        }

        //check ack
        {
            if !socket.recv_next() {
                continue;
            }

            let mut pp = PacketParser::new(socket.recv_buf());

            match pp.opcode() {
                Some(Opcode::Ack) => {
                    if !pp.number16_expected(expected_ack) {
                        continue;
                    }

                },
                Some(Opcode::Error) => {
                    panic!("tftp error recv");
                },
                _ => {
                    continue;
                },
            }

            expected_ack = expected_ack.overflowing_add(1).0;
            timeout.reset();
        }

        PacketBuilder::new(&mut buf).opcode(Opcode::Data).number16(expected_ack);

        let payload_len = match file.read(&mut filebuf[0..arguments.blksize]) {
            Ok(len) => len,
            _          => panic!("cannot read from file"),
        };

        if payload_len != DEFAULT_BLOCKSIZE  {
            is_last = true;
        }

        buf.extend_from_slice(&filebuf[0..payload_len]);
        socket.send(&buf);
    }
}