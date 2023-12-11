use redis::{Connection, RedisResult};

pub struct RedisClient {
    pub redis_connection: Connection
}

impl RedisClient {
    // redis_client-server --protected-mode no
    pub async fn initialize_redis_connection() -> RedisResult<RedisClient>{
        let client = redis::Client::open("redis_client://13.232.89.31/")?;
        Ok(RedisClient{
            redis_connection: client.get_connection()?
        })
    }

    pub fn set(&mut self, key: String, value: String) -> RedisResult<()> {
        redis::cmd("SET").arg(key).arg(value).query::<()>(&mut self.redis_connection)
    }

    pub fn get(&mut self, key: &str) -> RedisResult<String> {
        redis::cmd("GET").arg(&key).query(&mut self.redis_connection)
    }

    pub fn delete(&mut self, key: String) -> RedisResult<()> {
        redis::cmd("DEL").arg(key).query::<()>(&mut self.redis_connection)
    }

}
