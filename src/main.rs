use clap::{Command, Arg, builder::PossibleValue, ArgAction};

mod server;
mod client;
mod protcol;
mod tlog;

fn main()  {
    let args = Command::new("tftpserver")
        .author("Martin.K, martin.awake1@gmail.com")
        .version("0.1.1")
        .about("TFTP client and server")
        .subcommand(
            Command::new("server")
                .arg(Arg::new("rootdir")
                    .long("rootdir")
                    .required(true)
                    .help("base dir of the server")
                )
                .arg(Arg::new("writemode")
                    .long("writemode")
                    .required(false)
                    .value_parser([PossibleValue::new("disabled"), PossibleValue::new("new"), PossibleValue::new("overwrite")])
                    .default_value("new")
                    .help("Disabled: write not possible; New: New files can be uploaded; Overwrite: overwrite existing files allowed")
                )
                .arg(Arg::new("verbose")
                    .long("verbose")
                    .short('v')
                    .required(false)
                    .help("print verbose messages")
                    .default_value("false")
                )
                .arg(Arg::new("exit-with-client")
                    .long("exit-with-client")
                    .action(ArgAction::SetTrue)
                    .help("exit server after client disconnects")
                )
                .arg(Arg::new("port")
                    .long("port")
                    .help("port number server connect to; default is 69")
                )
        )
        .subcommand(Command::new("client")
            .arg(Arg::new("remote")
                .long("remote")
                .required(true)
                .help("address of the remote host; ipv4 or ipv6 address; port can also be appended e.g localhost:69")
            )
            .arg(Arg::new("download")
                .long("download")
                .required(false)
                .num_args(1..=2)
                .help("download a file with the given name from the remote server")
            )
            .arg(Arg::new("upload")
                .long("upload")
                .required(false)
                .num_args(1..=2)
                .help("upload a file with the given name to the remote server")
            )
            .arg(Arg::new("port")
                .long("port")
                .help("port number client connect to; default is 69")
            )
            .arg(Arg::new("blksize")
                .long("blksize")
                .short('b')
                .help("set the block size of the transfer; default is 512")
            )
            .arg(Arg::new("windowsize")
                .long("windowsize")
                .short('w')
                .help("set the windows size of the transfer; means number of blocks for one ack; default is 1")
            )
        )
        .get_matches();

    match args.subcommand() {
        Some(("server", args)) => server::server_main(args),
        Some(("client", args)) => client::client_main(args),
        _ => panic!("Invalid command; only allowed: server, client")
    }

    
}


