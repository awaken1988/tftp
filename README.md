TFTP server and client

# Overview
This is an implementation of an TFTP client and server.

# State
Currently under development

# Example

To start a server
```
tftp server --rootdir C:\tftp\
```

Download a file from server 
```
tftp client --remote 127.0.0.1:69 --download forest01.jpg -b 2048 -w 10
```

To speedup the transfer use the extended options. Here with blocksize=2048 and windowsize=10
```
tftp client --remote 127.0.0.1:69 --download forest01.jpg -b 2048 -w 10
```

# Features
* Basic Send/Recv with 512 Blksize
* Extended Options
    * Blocksize
    * Windowsize
    
 # Planned
 * Fix behaviour on packet loss (e.g ACK loss)
 * Implement Generators (Download from e.g e number generator)
 * Add UnitTest which tests timeout and missing packets 
