use tokio::sync::mpsc::Receiver;

pub async fn read_vector_from_channel(rx: &mut Receiver<Vec<u8>>) -> Result<Vec<u8>, &'static str> {
    let bytes_data = rx.recv().await.unwrap();
    Ok(bytes_data)
}
