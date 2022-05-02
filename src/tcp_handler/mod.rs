use tokio::net::tcp::OwnedReadHalf;
use tokio::io::{AsyncRead, AsyncReadExt};
use std::cmp::min;
use log::info;

pub async fn read_specified_bytes_from_socket(read_half: &mut OwnedReadHalf, bytes_to_read: usize) -> Result<Vec<u8>, &'static str> {
    let mut final_bytes_data:Vec<u8> = Vec::new();
    let mut bytes_read = 0;
    let mut bytes_left_to_read = bytes_to_read;
    while bytes_read < bytes_to_read {
        let mut bytes_data = vec![0;bytes_left_to_read];
        let n: usize = match read_half.read(&mut bytes_data).await {
            Ok(n) => n,
            Err(e) => return Err("Not able to read data")
        };
        bytes_read += n;
        bytes_left_to_read -= n;
        final_bytes_data.append(&mut bytes_data[0..n].to_owned());
        crate::info!("Bytes read from socket {}",n);
    }
    Ok(final_bytes_data)
}

// this function waits for some time
// TODO add timeout functionality
pub async fn read_bytes_from_socket_into_buffer(read_half: &mut OwnedReadHalf, bytes_data: &mut Vec<u8>) -> Result<(), &'static str> {
    let mut bytes_read:usize = 0;
    let n: usize = match read_half.read_buf(bytes_data).await {
        Ok(n) => n,
        Err(e) => return Err("Not able to read data")
    };
    crate::info!("Number of bytes read {}",n);
    Ok(())
}

pub async fn read_bytes_from_socket(read_half: &mut OwnedReadHalf) -> Result<Vec<u8>, std::io::Error> {
    let mut bytes_data = vec![0;4096];
    let mut ptr: usize = 0;
    let mut data_size = 0;
    loop {
        crate::info!("Start reading Data from socket");
        let mut buf = [0; 256];
        let n: usize = match read_half.read(&mut buf).await {
            Ok(n) => n,
            Err(e) => return Err(e)
        };
        if n <= 0 {
            break;
        }
        crate::info!("Read {} bytes",n);
        data_size += n;
        let end_ptr = min(n, bytes_data.len()-data_size);
        for i in 0..end_ptr {
            bytes_data[ptr] = buf[i];
            ptr += 1;
        }
        if n < buf.len() {
            break;
        }
        crate::info!("Copied Data");
    }
    crate::info!("Data read Complete");
    Ok(bytes_data[..data_size].to_owned())
}
