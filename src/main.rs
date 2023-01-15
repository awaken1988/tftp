use clap::{Command, Arg, builder::PossibleValue, ArgAction};

mod server;
mod client;
mod protcol;

fn main()  {
    let args = Command::new("tftpserver")
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
            .arg(Arg::new("read")
                .long("read")
                .required(false)
                .num_args(1..=2)
                .help("read a file with the given name from the remote server")
            )
            .arg(Arg::new("write")
                .long("write")
                .required(false)
                .num_args(1..=2)
                .help("write a file with the given name to the remote server")
            )
            .arg(Arg::new("port")
                .long("port")
                .help("port number client connect to; default is 69")
            )
        )
        .get_matches();

    match args.subcommand() {
        Some(("server", args)) => server::server_main(args),
        Some(("client", args)) => client::client_main(args),
        _ => panic!("Invalid command; only allowed: server, client")
    }

    
}


