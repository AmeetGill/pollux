// In this case, a server MAY send a Close
// frame with a status code of 1002 (protocol error)
// A client MUST close a connection if it detects a masked
//    frame

// read frames in to text

use std::ops::Index;
use crate::buffer::buffer::Buffer;
use tokio::net::tcp::OwnedReadHalf;
use tokio::io::AsyncReadExt;

static FIN_BITMASK: u8 = 0b10000000;
static OPCODE_BITMASK: u8 = 0b00001111;
static WEBSOCKET_MASK_BITMASK: u8 = 0b10000000;
static INITIAL_PAYLOAD_LENGTH_MASK: u8 = 0b01111111;

#[derive(Copy, Clone,Debug)]
pub enum Opcode {
    ContinuationFrame,
    TextFrame,
    BinaryFrame,
    ConnectionClose,
    Ping,
    Pong,
    NoOpcodeFound
}

static OPCODES_ARRAY: [(u8,Opcode);6] = [
    (0b00000000,Opcode::ContinuationFrame), // denotes a continuation frame
    (0b00000001,Opcode::TextFrame), // denotes a text frame
    (0b00000010,Opcode::BinaryFrame), // denotes a binary frame
    (0b00001000,Opcode::ConnectionClose), // denotes a connection close
    (0b00001001,Opcode::Ping), // denotes a ping
    (0b00001010,Opcode::Pong) // denotes a pong
];

#[derive(Debug)]
pub struct DataFrameInfo {
    pub mask_key: [u8;4],
    pub contain_masked_data: bool,
    pub total_bytes_to_read: usize,
    pub opcode: Opcode,
    pub is_this_final_frame: bool,
}

// only supported on 64bit server
pub fn parse_data_to_string(data_frame_info: &DataFrameInfo, data: &mut [u8]) -> String {
    let str_res = "";

    if data_frame_info.contain_masked_data {
        mask_unmask_data(data,&data_frame_info.mask_key)
    }

    let str_res = std::str::from_utf8(data).unwrap();

    str_res.to_string()
}


pub async fn read_next_websocket_dataframe(read_half: &mut OwnedReadHalf) -> DataFrameInfo {
    let mut data_frame_info = DataFrameInfo {
        mask_key: [0;4],
        total_bytes_to_read: 0,
        opcode: Opcode::NoOpcodeFound,
        is_this_final_frame: false,
        contain_masked_data: false,
    };
    let first_byte = read_half.read_u8().await.unwrap();
    data_frame_info.is_this_final_frame = first_byte & FIN_BITMASK != 0;

    // skip rsv flags 3 bits
    let opcode = OPCODE_BITMASK & first_byte;
    data_frame_info.opcode = parse_opcode(opcode);
    let second_byte = read_half.read_u8().await.unwrap();
    let websocket_mask_set = WEBSOCKET_MASK_BITMASK & second_byte != 0;
    let mut payload_length: usize = 0;
    let payload_length_field = INITIAL_PAYLOAD_LENGTH_MASK & second_byte;

    if payload_length_field <= 125 {
        payload_length = payload_length_field as usize;
    }

    if payload_length_field == 126 {
        payload_length = read_half.read_u16().await.unwrap() as usize;
    }
    if payload_length_field == 127 {
        let mut u64_number: u64 = read_half.read_u64().await.unwrap();
        payload_length = u64_number as usize;
    }

    data_frame_info.total_bytes_to_read = payload_length;

    let mut mask_key:[u8;4] = [0;4];
    if websocket_mask_set {
        mask_key[0] = read_half.read_u8().await.unwrap();
        mask_key[1] = read_half.read_u8().await.unwrap();
        mask_key[2] = read_half.read_u8().await.unwrap();
        mask_key[3] = read_half.read_u8().await.unwrap();
        data_frame_info.mask_key = mask_key.clone();
        data_frame_info.contain_masked_data = true;
    }

    data_frame_info
}

fn parse_opcode(opcode: u8) -> Opcode{
    for opcode_mapping in OPCODES_ARRAY.iter() {
        if opcode & opcode_mapping.0 != 0 {
            return opcode_mapping.1;
        }
    }
    Opcode::NoOpcodeFound
}

fn get_payload_length_bits(data_len: usize, mut masked_bit: u8) -> Vec<u8> {
    let mut vec: Vec<u8> = Vec::new();
    if data_len <= 125 {
        masked_bit |= data_len as u8;
        vec.push(masked_bit);
    } else {
        if data_len <= u16::MAX as usize {
            masked_bit |= 126;
            vec.push(masked_bit);
            vec.push((data_len >> 8) as u8 );
            vec.push(data_len as u8 );
        } else {
            masked_bit |= 127;
            vec.push(masked_bit);
            vec.push((data_len >> 56) as u8 );
            vec.push((data_len >> 48) as u8 );
            vec.push((data_len >> 40) as u8 );
            vec.push((data_len >> 32) as u8 );
            vec.push((data_len >> 24) as u8 );
            vec.push((data_len >> 16) as u8 );
            vec.push((data_len >> 8) as u8 );
            vec.push(data_len as u8 );
        }
    }
    vec
}

pub fn create_pong_frame(data_len: usize) -> Vec<u8> {
    let mut pong_frame = Buffer::new_unbound();
    let pong_opcode: u8 = 0b00001010;
    pong_frame.append_byte(0b10000000 | pong_opcode);
    let mut masked_bit: u8 = 0;

    pong_frame.append_vec8_array(
        &get_payload_length_bits(data_len,masked_bit)
    );

    pong_frame.get_arr()
}

pub fn mask_unmask_data(data: &mut [u8], mask_key: &[u8;4]) {
    for i in 0..data.len() {
        let mut j = i % 4;
        data[i] ^= mask_key[j];
    }
}

pub fn create_text_frame(data: &[u8]) -> Vec<u8> {
    let mut text_frame = Buffer::new_unbound();
    let text_opcode: u8 = 0b00000001;
    text_frame.append_byte(0b10000000 | text_opcode);
    let mut masked_bit: u8 = 0;

    text_frame.append_vec8_array(
        &get_payload_length_bits(data.len(),masked_bit)
    );

    text_frame.append_u8_array(data);

    text_frame.get_arr()
}

pub fn create_text_frame_with_data_length(data_len: usize) -> Vec<u8> {
    let mut text_frame = Buffer::new_unbound();
    let text_opcode = 0b10000001;
    text_frame.append_byte(text_opcode);
    let mut masked_bit: u8 = 0;

    text_frame.append_vec8_array(
        &get_payload_length_bits(data_len,masked_bit)
    );

    text_frame.get_arr()
}


