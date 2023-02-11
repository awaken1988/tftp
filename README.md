TFTP server and client

# Overview
This is an implementation of an TFTP client and server. 

# Example

To start a server
```
tftp server --rootdir C:\tftp\
```

Download a file from server
```
tftp client --remote 127.0.0.1:69 --read forest01.jpg --blksize 2048 --windowsize 10
```

# State
Currently under development

# Features
* Bascic Send/Recv with 512 Blksize
* Extended Options (Ourrently only Download)
    * Blocksize
    * Windowsize
    
 # Planned
 * Implement Extended Options for Upload
 * Implement Generators (Download from e.g e number generator)
 * Add UnitTest which tests timeout and missing packets 
