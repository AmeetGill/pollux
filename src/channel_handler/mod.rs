use tokio::sync::mpsc::Receiver;

pub async fn read_specified_bytes_from_channel(rx: &mut Receiver<u8>, bytes_to_read: usize) -> Result<Vec<u8>, &'static str> {
    let mut bytes_data:Vec<u8> = vec![0;bytes_to_read];
    let mut i = 0;

    while i < bytes_to_read {
        bytes_data[i] = rx.recv().await.unwrap();
        i += 1;
    }
    // if n != bytes_to_read {
    //     return Err("Not able to read complete data");
    // }
    Ok(bytes_data)

}
