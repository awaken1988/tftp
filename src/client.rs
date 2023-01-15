use std::{time::{Duration}, fs::File, io::{Write, Read}, path::{PathBuf}, str::FromStr, env};

use clap::ArgMatches;
use std::net::UdpSocket;
use crate::protcol::{Opcode,PacketBuilder, TransferMode, Timeout, RECV_TIMEOUT, check_datablock, self, DATA_OFFSET, DEFAULT_BLOCKSIZE, PACKET_SIZE_MAX, PacketParser, DEFAULT_WINDOWSIZE, BLKSIZE_STR, WINDOW_STR};

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

fn send_initial_packet(opcode: Opcode, paths: &ClientFilePath, args: &ClientArguments, socket: &mut UdpSocket) {
    let mut buf = Vec::new();

    let mut pkg = PacketBuilder::new(&mut buf)
        .opcode(opcode)
        .str(paths.remote.clone().to_str().expect("invalid remote filepath"))
        .separator()
        .transfer_mode(TransferMode::Octet)
        .separator();

    if args.blksize != DEFAULT_BLOCKSIZE {
        pkg = pkg.str(&BLKSIZE_STR);
    }
    if args.windowsize != DEFAULT_WINDOWSIZE {
        pkg = pkg.str(&WINDOW_STR);
    }

    socket.send(pkg.as_bytes()).expect("ERR  : send tftp request failed");
}

pub fn client_main(args: &ArgMatches) {
    let opcode = match (args.get_many::<String>("read"), args.get_many::<String>("write")) {
        (Some(_), None) => Opcode::Read,
        (None, Some(_)) => Opcode::Write,
        _               => panic!("invalid client action; only --read or --write possible")
    };

    let paths             = get_connection_paths(opcode, args);
    let client_arguments = ClientArguments::new(args);

    let mut socket = UdpSocket::bind("127.0.0.1:0").expect("Bind to interface failed");
    socket.connect(&client_arguments.remote).expect("Connection failed");

    send_initial_packet(opcode, &paths, &client_arguments, &mut socket);

    let mut timeout = Timeout::new(RECV_TIMEOUT);

    loop {
        if timeout.is_timeout() {
            break;
        }

        match opcode {
            Opcode::Read => {
                println!("local {:?}", &paths.local);
                let mut file = File::create(paths.local).expect("Cannot write file");
                read_action(&mut socket, &mut file);
                break;
            }
            Opcode::Write => {
                let mut file = File::open(paths.local).expect("Cannot write file");
                write_action(&mut socket, &mut file);
                break;
            }
            _ => panic!("not yet implemented"),
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

fn read_action(socket: &mut UdpSocket, file: &mut File) {
    let mut timeout    =  Timeout::new(RECV_TIMEOUT);
    let mut expected_block = 1u16;
    let mut buf: Vec<u8>        = Vec::new();
    let mut is_end        = false;

    while !is_end {
        buf.resize(protcol::MAX_PACKET_SIZE, 0);

        if timeout.is_timeout() {
            println!("timeout");
            break;
        }

        buf.resize(PACKET_SIZE_MAX, 0);
        let _                       = socket.set_read_timeout(Some(Duration::from_secs(1))); 
        let (amt, _src) = match socket.recv_from(&mut buf) {
            Ok((size,socket)) => (size,socket),
            Err(_) => {
               continue;
            }
        };

        buf.resize(amt, 0);

        if !check_datablock(&buf, expected_block) {
            continue;
        }

        //handle  data
        {
            buf.resize(amt, 0);
            let recv_data = &buf[DATA_OFFSET..];

            let _ = file.write(recv_data).expect("cannot write file");

            if recv_data.len() < DEFAULT_BLOCKSIZE {
                is_end = true;
            }
        }

        //send ACK
        let _ = socket.send(PacketBuilder::new(&mut buf)
            .opcode(Opcode::Ack)
            .number16(expected_block).as_bytes());

        expected_block = expected_block.overflowing_add(1).0;
        timeout.reset();
       
    }
}

fn write_action(socket: &mut UdpSocket, file: &mut File) {
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
            buf.resize(PACKET_SIZE_MAX, 0);
            let _                       = socket.set_read_timeout(Some(Duration::from_secs(1))); 
            let (amt, _src) = match socket.recv_from(&mut buf) {
                Ok((size,socket)) => (size,socket),
                Err(_) => {
                   continue;
                }
            };

            buf.resize(amt, 0);

            let mut pp = PacketParser::new(&mut buf);

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

        buf.resize(protcol::MAX_PACKET_SIZE, 0);
        PacketBuilder::new(&mut buf).opcode(Opcode::Data).number16(expected_ack);

        let payload_len = match file.read(&mut filebuf[0..DEFAULT_BLOCKSIZE]) {
            Ok(len) => len,
            _          => panic!("cannot read from file"),
        };

        if payload_len != DEFAULT_BLOCKSIZE  {
            is_last = true;
        }

        buf.extend_from_slice(&filebuf[0..payload_len]);
        socket.send(&buf).expect("cannot send data");
    }
}