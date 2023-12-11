use tokio::net::TcpStream;
use tokio::io::AsyncWriteExt;

pub async fn transmit(to_send: Vec<Vec<u8>>, ip_address: String) {
    let mut tcp_stream =TcpStream::connect(ip_address).await.unwrap();
    for vec in to_send {
        tcp_stream.write(&vec).await;
    }
}
