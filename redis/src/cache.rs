//! copyright © ecdata.cn 2021 - present
//! 把 Redis 接进 wechat_sdk 的缓存抽象，让 token / ticket 缓存可跨进程共享
//! created by shaipe

use redis::Commands;
use std::sync::Arc;
use wechat_sdk::{cache::CacheStore, WechatResult as Result};

/// 基于 Redis 的缓存实现，对齐 EasyWeChat 注入 Redis cache 的用法
#[derive(Debug, Clone)]
pub struct RedisCacheStore {
    client: redis::Client,
    namespace: String,
}

impl RedisCacheStore {
    /// 用已建好的 redis 连接创建
    pub fn new(client: redis::Client) -> RedisCacheStore {
        RedisCacheStore {
            client: client,
            namespace: "wechat".to_owned(),
        }
    }

    /// 用 url 创建，如 `redis://127.0.0.1/`
    pub fn from_url<U: AsRef<str>>(url: U) -> Result<RedisCacheStore> {
        let client = redis::Client::open(url.as_ref()).map_err(|e| error! {
            code: 6001,
            msg: format!("Open redis failed: {:?}", e),
        })?;

        Ok(RedisCacheStore::new(client))
    }

    /// 设置 key 前缀，默认 `wechat`
    pub fn with_namespace<S: Into<String>>(mut self, namespace: S) -> RedisCacheStore {
        self.namespace = namespace.into();
        self
    }

    fn full_key(&self, key: &str) -> String {
        format!("{}:{}", self.namespace, key)
    }

    fn connection(&self) -> Option<redis::Connection> {
        self.client.get_connection().ok()
    }
}

impl CacheStore for RedisCacheStore {
    fn get(&self, key: &str) -> Option<String> {
        let mut conn = self.connection()?;
        let value: Option<String> = conn.get(self.full_key(key)).ok()?;

        value
    }

    fn set(&self, key: &str, value: &str, ttl_seconds: u64) {
        let Some(mut conn) = self.connection() else {
            return;
        };
        let full_key = self.full_key(key);

        let result: redis::RedisResult<()> = if ttl_seconds == 0 {
            conn.set(full_key, value)
        } else {
            conn.set_ex(full_key, value, ttl_seconds as usize)
        };

        if let Err(err) = result {
            log!("redis cache set failed: {:?}", err);
        }
    }

    fn forget(&self, key: &str) {
        let Some(mut conn) = self.connection() else {
            return;
        };

        let result: redis::RedisResult<()> = conn.del(self.full_key(key));

        if let Err(err) = result {
            log!("redis cache del failed: {:?}", err);
        }
    }

    fn keys(&self) -> Vec<String> {
        let Some(mut conn) = self.connection() else {
            return Vec::new();
        };

        let pattern = format!("{}:*", self.namespace);
        let prefix_len = self.namespace.len() + 1;
        let mut keys: Vec<String> = Vec::new();
        let mut cursor: u64 = 0;

        loop {
            let page: redis::RedisResult<(u64, Vec<String>)> =
                redis::cmd("SCAN")
                    .arg(cursor)
                    .arg("MATCH")
                    .arg(&pattern)
                    .arg("COUNT")
                    .arg(100)
                    .query(&mut conn);

            match page {
                Ok((next, items)) => {
                    keys.extend(
                        items
                            .into_iter()
                            .filter_map(|key| key.get(prefix_len..).map(|v| v.to_owned())),
                    );
                    cursor = next;

                    if cursor == 0 {
                        break;
                    }
                }
                Err(err) => {
                    log!("redis cache scan failed: {:?}", err);
                    break;
                }
            }
        }

        keys
    }
}

/// 安装 Redis 缓存：之后的 access_token / ticket / verify_ticket 都写进 Redis
pub fn install_redis_cache<U: AsRef<str>>(url: U) -> Result<()> {
    let store = RedisCacheStore::from_url(url)?;
    wechat_sdk::cache::set_cache_store(Arc::new(store));

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 极简 RESP2 服务器：让 `RedisCacheStore` 能连真实 socket 跑通 get/set/forget/keys
    mod fake_redis {
        use std::collections::HashMap;
        use std::io::{BufRead, BufReader, Read, Write};
        use std::net::{TcpListener, TcpStream};
        use std::sync::{Arc, Mutex};
        use std::thread;

        pub struct FakeRedis {
            pub port: u16,
            store: Arc<Mutex<HashMap<String, String>>>,
        }

        impl FakeRedis {
            pub fn start() -> FakeRedis {
                let listener = TcpListener::bind("127.0.0.1:0").unwrap();
                let port = listener.local_addr().unwrap().port();
                let store: Arc<Mutex<HashMap<String, String>>> = Arc::new(Mutex::new(HashMap::new()));
                let shared = store.clone();

                thread::spawn(move || {
                    for stream in listener.incoming().flatten() {
                        let store = shared.clone();
                        thread::spawn(move || handle(stream, store));
                    }
                });

                FakeRedis { port: port, store: store }
            }

            pub fn get(&self, key: &str) -> Option<String> {
                self.store.lock().unwrap().get(key).cloned()
            }
        }

        fn read_command(reader: &mut BufReader<TcpStream>) -> Option<Vec<String>> {
            let mut line = String::new();
            if reader.read_line(&mut line).ok()? == 0 {
                return None;
            }

            let count: usize = line.trim().trim_start_matches('*').parse().ok()?;
            let mut args = Vec::with_capacity(count);

            for _ in 0..count {
                let mut len_line = String::new();
                reader.read_line(&mut len_line).ok()?;
                let len: usize = len_line.trim().trim_start_matches('$').parse().ok()?;
                let mut buf = vec![0u8; len + 2];
                reader.read_exact(&mut buf).ok()?;
                args.push(String::from_utf8_lossy(&buf[..len]).to_string());
            }

            Some(args)
        }

        fn bulk(value: &str) -> String {
            format!("${}\r\n{}\r\n", value.len(), value)
        }

        fn handle(stream: TcpStream, store: Arc<Mutex<HashMap<String, String>>>) {
            let mut writer = stream.try_clone().unwrap();
            let mut reader = BufReader::new(stream);

            while let Some(args) = read_command(&mut reader) {
                if args.is_empty() {
                    continue;
                }

                let command = args[0].to_uppercase();
                let reply = match command.as_str() {
                    "SET" => {
                        store.lock().unwrap().insert(args[1].clone(), args[2].clone());
                        "+OK\r\n".to_owned()
                    }
                    "SETEX" => {
                        store.lock().unwrap().insert(args[1].clone(), args[3].clone());
                        "+OK\r\n".to_owned()
                    }
                    "GET" => match store.lock().unwrap().get(&args[1]) {
                        Some(value) => bulk(value),
                        None => "$-1\r\n".to_owned(),
                    },
                    "DEL" => {
                        let removed = store.lock().unwrap().remove(&args[1]).is_some() as i64;
                        format!(":{}\r\n", removed)
                    }
                    "SCAN" => {
                        let keys: Vec<String> = store.lock().unwrap().keys().cloned().collect();
                        let mut body = format!("*2\r\n{}*{}\r\n", bulk("0"), keys.len());

                        for key in keys {
                            body.push_str(&bulk(&key));
                        }

                        body
                    }
                    _ => "+OK\r\n".to_owned(),
                };

                if writer.write_all(reply.as_bytes()).is_err() {
                    return;
                }
                let _ = writer.flush();
            }
        }
    }

    /// 本地没有 Redis 时跳过，避免 CI 依赖外部服务
    fn store() -> Option<RedisCacheStore> {
        let store = RedisCacheStore::from_url("redis://127.0.0.1:6379/")
            .ok()?
            .with_namespace("wechat_rs_test");

        store.connection()?;

        Some(store)
    }

    #[test]
    fn redis_store_roundtrip() {
        let Some(store) = store() else {
            return;
        };

        store.set("k1", "v1", 30);
        assert_eq!(store.get("k1"), Some("v1".to_owned()));

        store.forget("k1");
        assert_eq!(store.get("k1"), None);
    }

    #[test]
    fn redis_store_talks_to_resp_server() {
        let fake = fake_redis::FakeRedis::start();
        let store = RedisCacheStore::from_url(format!("redis://127.0.0.1:{}/", fake.port))
            .unwrap()
            .with_namespace("t");

        store.set("a", "1", 60);
        assert_eq!(store.get("a"), Some("1".to_owned()));
        assert_eq!(fake.get("t:a"), Some("1".to_owned()));

        store.set("b", "2", 0);
        assert_eq!(store.get("b"), Some("2".to_owned()));

        let mut keys = store.keys();
        keys.sort();
        assert_eq!(keys, vec!["a".to_owned(), "b".to_owned()]);

        store.forget("a");
        assert_eq!(store.get("a"), None);
        assert_eq!(fake.get("t:a"), None);
    }

    #[test]
    fn install_redis_cache_swaps_global_store() {
        let fake = fake_redis::FakeRedis::start();
        install_redis_cache(format!("redis://127.0.0.1:{}/", fake.port)).unwrap();

        // 全局 store 换成 Redis 后，sdk 的缓存写入会落到 Redis
        wechat_sdk::cache::set_raw("install", "ok", 60);
        assert_eq!(fake.get("wechat:install"), Some("ok".to_owned()));

        wechat_sdk::cache::forget("install");
        wechat_sdk::cache::reset_cache_store();
    }

    #[test]
    fn redis_store_lists_keys() {
        let Some(store) = store() else {
            return;
        };

        store.set("prefix:a", "1", 30);
        let keys = store.keys();

        assert!(keys.iter().any(|key| key == "prefix:a"));

        store.forget("prefix:a");
    }
}
