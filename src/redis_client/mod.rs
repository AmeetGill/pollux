use redis::{Connection, RedisResult};

pub async fn initialize_redis_connection() -> RedisResult<Connection>{
    let client = redis::Client::open("redis://127.0.0.1/")?;
    Ok( client.get_connection()?)
}

