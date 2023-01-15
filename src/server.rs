use std::{net::{UdpSocket, SocketAddr}, time::{Duration, Instant}, sync::{Mutex, Arc, mpsc::channel}, thread, collections::HashMap, path::PathBuf};

use clap::*;

use crate::protcol;

use self::defs::{WriteMode, ServerSettings, FileLockMode, ClientState};

mod connection;
mod defs;

pub fn server_main(args: &ArgMatches) {
    //TODO: there is a more elegant way with clap; but for now simple redundant strings used
    let writemode = args.get_one::<String>("writemode").unwrap();
    let writemode = match writemode.as_str() {
        "disabled" => WriteMode::Disabled,
        "new" => WriteMode::WriteNew,
        "overwrite" => WriteMode::WriteOverwrite,
        other => panic!("writemode {} does not exist", other),
    };

    //TODO: let verbose = args.get_one::<bool>("verbose").unwrap();
    

    let rootdir = args.get_one::<String>("rootdir").unwrap();

    if !std::path::Path::new(rootdir).is_dir(){
        panic!("rootdir = \"{}\" does not exists", rootdir);
    }

    let port = args.get_one::<String>("port").unwrap_or(&"69".to_string()).clone();
    let port     = u16::from_str_radix(&port, 10).expect("wrong port");

    let settings = ServerSettings {
        write_mode:        writemode,
        root_dir:          rootdir.clone(),
        blocksize:         protcol::DEFAULT_BLOCKSIZE,
        verbose:           true, 
        exit_with_client:  *args.get_one::<bool>("exit-with-client").unwrap(),
        port:              port

    };

    run_server(settings);
}

pub fn run_server(settings: ServerSettings) {
    let socket = UdpSocket::bind(format!("127.0.0.1:{}", settings.port)).unwrap();
    let _ = socket.set_read_timeout(Some(Duration::from_secs(1)));  //TODO: check for error
    let mut connections = HashMap::<SocketAddr,ClientState>::new();
    let mut cleanpup_stopwatch = Instant::now();

    let files_locked = Arc::new(Mutex::new(HashMap::<PathBuf,FileLockMode>::new()));

    let mut buf = Vec::<u8>::new();

    loop {
        //cleanup
        if cleanup_connections(&mut connections, &mut cleanpup_stopwatch) && settings.exit_with_client {
            break;
        }
    
        buf.resize(protcol::MAX_PACKET_SIZE, 0);

        let (amt, src) = match socket.recv_from(&mut buf) {
            Ok((size,socket)) => (size,socket),
            Err(_) => {
                continue;
            }
        };

        buf.resize(amt, 0);
    
        if connections.contains_key(&src) {
            let _ = connections.get(&src).unwrap().tx.send(buf.clone());
        }
        else {
            let (sender, receiver) = channel();

            let mut client_state = ClientState {
                tx: sender,
                join_handle: Option::None,
            };

            let remote = src;
            
            let socket = socket.try_clone().unwrap();
            let settings = settings.clone();
            let files_locked = files_locked.clone();
            client_state.join_handle = Some(thread::spawn(move|| {
                connection::Connection::new(
                    receiver, 
                    remote,
                    socket,
                    settings,
                    files_locked).run();
            }));


            connections.insert(src,client_state);
            let _ = connections.get(&src).unwrap().tx.send(buf.clone());
        }       
    }
}

fn cleanup_connections(connections: &mut HashMap::<SocketAddr,ClientState>, stopwatch: &mut Instant) -> bool {
    if stopwatch.elapsed() < defs::CLEANUP_TIMEOUT {
        return false; 
    }

    let mut todo_delete: Vec<SocketAddr> = vec![];
    for i_con in connections.iter_mut() {
        if i_con.1.join_handle.as_mut().unwrap().is_finished() {
            todo_delete.push(*i_con.0);
        }
    }
    for i_con in todo_delete.iter() {
        let state = connections.remove(&i_con).unwrap();
        println!("INFO: {:?} quit", i_con);
        let _ = state.join_handle.unwrap().join();
    }


    *stopwatch = Instant::now();

    return if todo_delete.len() > 0 {true} else {false};
}