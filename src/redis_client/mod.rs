use redis::{Connection, RedisResult};

pub async fn initialize_redis_connection() -> RedisResult<Connection>{
    let client = redis::Client::open("redis://13.233.167.237/")?;
    Ok( client.get_connection()?)
}

