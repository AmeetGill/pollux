use redis::{Connection, RedisResult};

pub async fn initialize_redis_connection() -> RedisResult<Connection>{
    let client = redis::Client::open("redis://52.66.11.144/")?;
    Ok( client.get_connection()?)
}

