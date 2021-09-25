use serde::{Deserialize,Serialize};

#[derive(Deserialize,Serialize)]
pub struct User {
    user_id: u32
}

#[derive(Deserialize,Serialize)]
pub struct Message {
    pub sender_user_id: u32,
    pub message: String
}
