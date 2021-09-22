use serde::{Deserialize,Serialize};

#[derive(Deserialize,Serialize)]
pub struct User {
    user_id: String
}
