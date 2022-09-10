use std::fs;
use serde::{Deserialize,Serialize};

#[derive(Deserialize,Serialize,Debug)]
pub struct ServiceConfig {
    pub cluster_mode: bool,
    pub websocket_port: String
}

#[derive(Deserialize,Serialize,Debug)]
pub struct GlobalConfig {
    pub test: ServiceConfig,
    pub prod: ServiceConfig
}

pub fn new_config(env: String) -> ServiceConfig{
    let data = fs::read_to_string("./config.json")
        .expect("Unable to read file");

    let json: GlobalConfig = serde_json::from_str(&data)
        .expect("JSON does not have correct format.");

    if env == "prod" {
        return json.prod;
    }

    json.test
}

pub fn get_default_config() -> ServiceConfig {
    ServiceConfig {
        cluster_mode: false,
        websocket_port: "3999".to_owned()
    }
}