use tokio::net::TcpListener;
use crate::data_frame::read_next_dataframe_from_socket;
use crate::tcp_handler::read_specified_bytes_from_socket;
use crate::model::Message;

pub async fn listen_for_messages_from_other_services(addr: String) {
    let listener = TcpListener::bind(addr).await.unwrap();

    loop {
        let (socket, _) = listener.accept().await.unwrap();
        let (mut read_half, _) = socket.into_split();

        let data_frame = read_next_dataframe_from_socket(&mut read_half).await;

        let data = read_specified_bytes_from_socket(&mut read_half,data_frame.payload_length).await.unwrap();

        let message: Message = serde_json::from_str(std::str::from_utf8(&data).unwrap()).unwrap();
        let user_id_mapping = crate::USER_ID_MAPPING.lock().await;
        let user_sender = user_id_mapping.get(&message.sender_user_id).unwrap();
        let mut vec_to_send: Vec<Vec<u8>> = Vec::new();
        vec_to_send.push(data_frame.raw_bytes);
        vec_to_send.push(data);

        let tx2 = user_sender.clone();
        for bytes in vec_to_send {
            tx2.send(bytes).await;
        }

    }
}
