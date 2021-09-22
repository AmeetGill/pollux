use tokio::net::tcp::OwnedReadHalf;
use tokio::io::AsyncReadExt;
use std::cmp::min;

pub async fn read_specified_bytes_from_socket(read_half: &mut OwnedReadHalf, bytes_to_read: usize) -> Result<Vec<u8>, &'static str> {
    let mut bytes_data = vec![0;bytes_to_read];
    let n: usize = match read_half.read(&mut bytes_data).await {
        Ok(n) => n,
        Err(e) => return Err("Not able to read data")
    };
    if n != bytes_to_read {
        return Err("Not able to read complete data");
    }
    Ok(bytes_data)
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
