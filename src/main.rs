use std::net::SocketAddr;
use log::{info, error, LevelFilter};
use log4rs::Config;
use log4rs;
use log4rs::append::console::ConsoleAppender;
use log4rs::config::{Appender, Root};
use log4rs::encode::pattern::PatternEncoder;
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};

use http::{Request, Response, StatusCode};
use crate::data_frame::{Opcode, DataFrameInfo, ReadFrom};
use crate::model::{User, Message};
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio::net::tcp::OwnedReadHalf;
use tokio::sync::mpsc::Receiver;
use std::collections::HashMap;
use tokio::sync::mpsc::Sender;
use http::header::HeaderName;
use redis::{Connection, RedisResult, Client};
use std::future::Future;
use std::borrow::BorrowMut;

#[macro_use]
extern crate lazy_static;
extern crate base64;
extern crate redis;

mod http_handler;
mod error;
mod buffer;
mod data_frame;
mod model;
mod tcp_handler;
mod redis_client;
mod channel_handler;
mod workers;

//           ws-URI = "ws:" "//" host [ ":" port ] path [ "?" query ]
//           wss-URI = "wss:" "//" host [ ":" port ] path [ "?" query ]
//
//           host = <host, defined in [RFC3986], Section 3.2.2>
//           port = <port, defined in [RFC3986], Section 3.2.3>
//           path = <path-abempty, defined in [RFC3986], Section 3.3>
//           query = <query, defined in [RFC3986], Section 3.4>
//            The port component is OPTIONAL; the default for "ws" is port 80,
//           while the default for "wss" is port 443.

lazy_static! {
    static ref USER_ID_MAPPING: Mutex<HashMap<u32,Sender<u8>>> = {
        let mut m = Mutex::new(HashMap::new());
        m
    };
    static ref REDIS_CONNECTION: Mutex<Option<Connection>> = {
        Mutex::new(None)
    };
    static ref MY_ADDRESS: String = {
        format!("{}{}","127.0.0.1:",rand::random::<u16>())
    };
    static ref TCP_WORKER_ADDRESS: String = {
        format!("{}{}","127.0.0.1:",rand::random::<u16>())
    };
}

#[tokio::main]
async fn main() {

    let stdout = ConsoleAppender::builder()
        .encoder(Box::new(PatternEncoder::new("{d(%Y-%m-%d %H:%M:%S %Z)(utc)} {h({l})} {T} [{f:1.10}:{L}] [{M}] {m}{n}")))
        .build();

    let config = Config::builder()
        .appender(Appender::builder().build("stdout", Box::new(stdout)))
        .build(Root::builder().appender("stdout").build(LevelFilter::Trace))
        .unwrap();

    let _handle = log4rs::init_config(config).unwrap();
    info!("My Address, {}",MY_ADDRESS.to_ascii_lowercase());
    // Bind the listener to the address
    let listener = TcpListener::bind(MY_ADDRESS.to_ascii_lowercase()).await.unwrap();

    match redis_client::initialize_redis_connection().await {
        Ok(connection) => {
            REDIS_CONNECTION.lock().await.insert(connection);
        }
        Err(_) => {
            panic!("Not able to connect to redis");
        }
    }

    tokio::spawn(async move {
        workers::tcp_message_listener::listen_for_messages_from_other_services(TCP_WORKER_ADDRESS.to_ascii_lowercase()).await;
    });

    loop {
        // The second item contains the IP and port of the new connection.
        info!("Waiting for Clients at Address: {}",MY_ADDRESS.to_ascii_lowercase());
        let (socket, socket_address) = listener.accept().await.unwrap();
        info!("Client connected : {:?}", socket_address);

        tokio::spawn(async move{
            process(socket, socket_address).await;
        });
    }
}
// The server MUST close the connection upon receiving a
//    frame that is not masked. In this case, a server MAY send a Close
//    frame with a status code of 1002 (protocol error)
async fn process(socket: TcpStream, _ip: SocketAddr )  {
    info!("Processing TcpStream: Start");
    let ( mut read_half, mut write_half) = socket.into_split();

    let bytes = tcp_handler::read_bytes_from_socket(&mut read_half).await.unwrap();
    println!("{}", std::str::from_utf8(&bytes).unwrap());
    let http_request: Request<()> = http_handler::parse_http_request_bytes(bytes).unwrap();

    // handshake
    let (mut http_resp,user_id) = http_handler::create_websocket_response(http_request).unwrap_or_else(|_error| {
        return (http_handler::create_401_response(),0);
    });
    let http_response_status = http_resp.status().clone();

    let http_resp_bytes = http_handler::get_http_response_bytes(http_resp);
    match write_half.write(&*http_resp_bytes.unwrap()).await {
        Ok(n) => info!("Data sent size: {}",n),
        Err(e) => {
            error!("Enable to send Data : {}",e);
            return;
        }
    };

    if http_response_status != StatusCode::SWITCHING_PROTOCOLS{
        info!("Connection Unsuccessful, Dropping thread");
        return;
    }

    let (tx, mut rx) = mpsc::channel(100);

    USER_ID_MAPPING.lock().await.insert(user_id,tx.clone());

    redis::cmd("SET").arg(user_id).arg(TCP_WORKER_ADDRESS.to_ascii_lowercase()).query::<()>(REDIS_CONNECTION.lock().await.as_mut().unwrap()).unwrap();

    loop {

        let mut data_frame = DataFrameInfo{
            mask_key: [0;4],
            contain_masked_data: false,
            total_bytes_to_read: 0,
            opcode: Opcode::NoOpcodeFound,
            is_this_final_frame: false,
            read_from: ReadFrom::Socket,
            raw_bytes: Vec::new()
        };
        tokio::select! {
            val = data_frame::read_next_dataframe_from_socket(&mut read_half) => {
                data_frame = val;
            }
            val = data_frame::read_dataframe_from_rx(&mut rx) => {
                data_frame = val;
            }
        };

        let mut vec_to_send: Vec<Vec<u8>> = Vec::new();

        match data_frame.opcode {
            Opcode::TextFrame => {
                info!("TextFrame: {:?}",data_frame);
                let mut text_data = read_data(&data_frame,&mut rx,&mut read_half).await;
                match data_frame.read_from {
                    ReadFrom::Socket => {
                        data_frame::mask_unmask_data(&mut text_data, &data_frame.mask_key);
                        vec_to_send.push(data_frame::create_text_frame_with_data_length(text_data.len()));
                        vec_to_send.push(text_data);
                    }
                    ReadFrom::Channel => {
                        info!("message received from Channel");
                        vec_to_send.push(data_frame.raw_bytes);
                        vec_to_send.push(text_data);
                    }
                }
            }
            Opcode::Ping => {
                let mut ping_data = read_data(&data_frame,&mut rx,&mut read_half).await;
                data_frame::mask_unmask_data(&mut ping_data, &data_frame.mask_key);
                let mut vec_to_send: Vec<Vec<u8>> = Vec::new();
                vec_to_send.push(data_frame::create_pong_frame(ping_data.len()));
                vec_to_send.push(ping_data);
            }
            Opcode::BinaryFrame => {
                let mut vec_to_send: Vec<Vec<u8>> = Vec::new();
                let mut binary_data = read_data(&data_frame,&mut rx,&mut read_half).await;
                match data_frame.read_from {
                    ReadFrom::Socket => {
                        data_frame::mask_unmask_data(&mut binary_data, &data_frame.mask_key);
                        vec_to_send.push(data_frame::create_binary_frame_with_data_length(binary_data.len()));
                        vec_to_send.push(binary_data);
                    }
                    ReadFrom::Channel => {
                        info!("message received from Channel");
                        vec_to_send.push(data_frame.raw_bytes);
                        vec_to_send.push(binary_data);
                    }
                }
            }
            Opcode::ConnectionClose => {
                info!("Close Connection Opcode received");
                break;
            }
            _ => {
                error!("Opcode not supported");
                break;
            }
        };

        let message: Message = serde_json::from_str(std::str::from_utf8( vec_to_send.get(1).unwrap()).unwrap()).unwrap();

        match data_frame.read_from {
            ReadFrom::Socket => {
                match data_frame.opcode {
                    Opcode::Ping => {
                        for vec_data in vec_to_send {
                            write_half.write(&vec_data).await;
                        }
                    }
                    _ => {
                        match USER_ID_MAPPING.lock().await.get(&message.sender_user_id) {
                            None => {
                                info!("User not connected to this server");
                                let mut guarded_con = REDIS_CONNECTION.lock().await;
                                let con = guarded_con.as_mut().unwrap();
                                let ip: String = redis::cmd("GET").arg(&message.sender_user_id).query(con).unwrap_or_else(|err| {
                                    "".to_string()
                                });
                                info!("User connected to the server: {}",ip);

                                if ip.len() > 0 {
                                    workers::tcp_message_transmitter::transmit(vec_to_send,ip).await;
                                }
                            }
                            Some(tx2) => {
                                let tx2 = tx2.clone();
                                for bytes in vec_to_send {
                                    for byte in bytes {
                                        tx2.send(byte).await;
                                    }
                                }
                            }
                        }
                    }
                }
            }
            ReadFrom::Channel => {
                for vec_data in vec_to_send {
                    write_half.write(&vec_data).await;
                }
            }
        }
    }

    info!("Removing entry from hashMap: End");
    USER_ID_MAPPING.lock().await.remove(&user_id);
    redis::cmd("DEL").arg(user_id).query::<()>(REDIS_CONNECTION.lock().await.as_mut().unwrap()).unwrap();


    info!("Processing TcpStream: End");

}

async fn read_data(data_frame: &DataFrameInfo, rx: &mut Receiver<u8>, read_half: &mut OwnedReadHalf) -> Vec<u8>{
    match data_frame.read_from {
        ReadFrom::Socket => {
            tcp_handler::read_specified_bytes_from_socket(read_half, data_frame.total_bytes_to_read).await.unwrap()
        }
        ReadFrom::Channel => {
            channel_handler::read_specified_bytes_from_channel(rx,data_frame.total_bytes_to_read).await.unwrap()
        }
    }
}
