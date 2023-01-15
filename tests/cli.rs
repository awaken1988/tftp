use assert_cmd::prelude::*; // Add methods on commands
use predicates::prelude::*; // Used for writing assertions
use std::{process::Command, thread, time::Duration, fs::{File, self}, path::Path}; // Run programs
use assert_fs::prelude::*;


//FIXME:
//  run these tests with: 
//      cargo test -- --test-threads=1
//  otherwise test will propably fail because there can only be on server on port 69
//  -> use alternative ports for this scenario

#[test]
fn download_smaller_blocksize() -> Result<(), Box<dyn std::error::Error>> {
    tftp_transfer(&[0,1,2,3], true, 55001)
}

#[test]
fn download_exact_blocksize() -> Result<(), Box<dyn std::error::Error>> {
    tftp_transfer(&generate_data(512), true, 55002)
}

#[test]
fn download_mult_blocksize() -> Result<(), Box<dyn std::error::Error>> {
    tftp_transfer(&generate_data(3*512), true, 55003)
}

#[test]
fn upload_smaller_blocksize() -> Result<(), Box<dyn std::error::Error>> {
    tftp_transfer(&[0,1,2,3], false, 55004)
}

#[test]
fn upload_exact_blocksize() -> Result<(), Box<dyn std::error::Error>> {
    tftp_transfer(&generate_data(512), false, 55005)
}

#[test]
fn upload_mult_blocksize() -> Result<(), Box<dyn std::error::Error>> {
    tftp_transfer(&generate_data(3*512), false, 55006)
}

fn generate_data(size: usize) -> Vec<u8> {
    let mut buf:Vec<u8> = Vec::new();

    for i in 0..size {
        buf.push(i as u8);
    }

    buf
}

fn tftp_transfer(data: &[u8], is_read: bool, port: u16) -> Result<(), Box<dyn std::error::Error>> {
    let server_root = assert_fs::TempDir::new().unwrap().into_persistent();
    let client_root = assert_fs::TempDir::new().unwrap().into_persistent();
    let server_file_path = server_root.join("download.bin");
    let client_file_path = client_root.join("download.bin");

    let generated_file = if is_read {
        &server_root
    } else {
        &client_root
    };

    let transfer_path = generated_file.child("download.bin");    
    transfer_path.write_binary(data).unwrap();

    let mut cmd_path = Command::cargo_bin("tftp").unwrap().get_program().to_os_string();

    //start server
    let server_hndl = {
        let mut cmd_path = cmd_path.clone();
        let server_root = server_root.path().to_path_buf();
        thread::spawn(move || {    
            println!("server started");
            std::process::Command::new(cmd_path)
                .arg("server")
                .arg("--rootdir").arg(server_root)
                .arg("--exit-with-client")
                .arg("--port").arg(format!("{}",port))
                .output().unwrap();
        })
    };

    thread::sleep(Duration::from_secs(2));

    let client_hndl = {
        let mut cmd_path = cmd_path.clone();
        let client_file_path = client_file_path.clone();
        let server_file_path = server_file_path.clone();
        thread::spawn(move || {    
            println!("client started");
            let mut cmd = std::process::Command::new(cmd_path);
            cmd.arg("client");
            cmd.arg("--remote").arg(format!("127.0.0.1:{}",port));

            if is_read {
                cmd.arg("--read");
                cmd.arg("download.bin");
                cmd.arg(&*client_file_path.to_string_lossy());
            } else {
                cmd.arg("--write");
                cmd.arg(&*client_file_path.to_string_lossy());
                cmd.arg("download.bin");  
            }
            
            
            let output = cmd.output().unwrap();
            println!("{:?}", output);
        })
    };

    client_hndl.join();
    println!("client ready");
    
    server_hndl.join();
    println!("server ready");

    compare(&server_file_path, &client_file_path);

    Ok(())
}

fn compare(l: &Path, r: &Path) {
    let l = fs::read(l).unwrap();
    let r = fs::read(r).unwrap();

    let mut is_same = *l == *r;

    assert!(is_same);
}