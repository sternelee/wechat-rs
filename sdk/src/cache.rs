//! copyright © ecdata.cn 2021 - present
//! 通用 KV 缓存抽象
//!
//! EasyWeChat 6.x 的 AccessToken / Ticket / VerifyTicket 都通过注入的 cache 存储
//! （PSR-16）缓存，这里提供等价的可替换实现：
//! - 默认是进程内 `MemoryCache`（带 TTL）
//! - 通过 [`set_cache_store`] 注入 Redis 等外部存储（如 `wechat_redis::RedisCacheStore`）
//!
//! created by shaipe

use serde::de::DeserializeOwned;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

/// 键值缓存接口，对齐 PSR-16 的 get/set/delete
pub trait CacheStore: Send + Sync {
    /// 读取原始字符串
    fn get(&self, key: &str) -> Option<String>;
    /// 写入并设置过期时间（秒），ttl 为 0 表示不过期
    fn set(&self, key: &str, value: &str, ttl_seconds: u64);
    /// 删除
    fn forget(&self, key: &str);

    /// 列出所有 key（可选实现，用于按前缀清理）
    fn keys(&self) -> Vec<String> {
        Vec::new()
    }
}

/// 进程内缓存，带 TTL
#[derive(Debug, Default)]
pub struct MemoryCache {
    items: Mutex<HashMap<String, (String, u64)>>,
}

impl MemoryCache {
    /// 新建空缓存
    pub fn new() -> MemoryCache {
        MemoryCache::default()
    }

    fn now() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or_default()
    }
}

impl CacheStore for MemoryCache {
    fn get(&self, key: &str) -> Option<String> {
        let mut items = self.items.lock().unwrap();
        let now = MemoryCache::now();

        match items.get(key) {
            Some((value, expire_at)) => {
                if *expire_at != 0 && *expire_at <= now {
                    items.remove(key);
                    None
                } else {
                    Some(value.clone())
                }
            }
            None => None,
        }
    }

    fn set(&self, key: &str, value: &str, ttl_seconds: u64) {
        let expire_at = if ttl_seconds == 0 {
            0
        } else {
            MemoryCache::now() + ttl_seconds
        };

        self.items
            .lock()
            .unwrap()
            .insert(key.to_owned(), (value.to_owned(), expire_at));
    }

    fn forget(&self, key: &str) {
        self.items.lock().unwrap().remove(key);
    }

    fn keys(&self) -> Vec<String> {
        self.items.lock().unwrap().keys().cloned().collect()
    }
}

lazy_static! {
    static ref CACHE_STORE: Mutex<Arc<dyn CacheStore>> = Mutex::new(Arc::new(MemoryCache::new()));
}

/// 替换全局缓存实现（例如注入 Redis）
pub fn set_cache_store(store: Arc<dyn CacheStore>) {
    *CACHE_STORE.lock().unwrap() = store;
}

/// 当前缓存实现
pub fn cache_store() -> Arc<dyn CacheStore> {
    CACHE_STORE.lock().unwrap().clone()
}

/// 恢复为进程内缓存（测试用）
pub fn reset_cache_store() {
    set_cache_store(Arc::new(MemoryCache::new()));
}

/// 读取并反序列化
pub fn get<T: DeserializeOwned>(key: &str) -> Option<T> {
    cache_store()
        .get(key)
        .and_then(|raw| serde_json::from_str(&raw).ok())
}

/// 序列化写入
pub fn set<T: Serialize>(key: &str, value: &T, ttl_seconds: u64) {
    if let Ok(raw) = serde_json::to_string(value) {
        cache_store().set(key, &raw, ttl_seconds);
    }
}

/// 读取原始字符串
pub fn get_raw(key: &str) -> Option<String> {
    cache_store().get(key)
}

/// 写入原始字符串
pub fn set_raw(key: &str, value: &str, ttl_seconds: u64) {
    cache_store().set(key, value, ttl_seconds);
}

/// 删除
pub fn forget(key: &str) {
    cache_store().forget(key);
}

/// 按前缀清理（内存缓存支持；外部存储若不实现 keys() 则忽略）
pub fn forget_prefix(prefix: &str) {
    let store = cache_store();

    for key in store.keys() {
        if key.starts_with(prefix) {
            store.forget(&key);
        }
    }
}

/// 全局缓存是进程级状态，涉及它的测试需要串行执行
#[cfg(test)]
pub(crate) static TEST_GUARD: Mutex<()> = Mutex::new(());

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_cache_stores_values() {
        let cache = MemoryCache::new();
        cache.set("k", "v", 60);

        assert_eq!(cache.get("k"), Some("v".to_owned()));

        cache.forget("k");
        assert_eq!(cache.get("k"), None);
    }

    #[test]
    fn memory_cache_expires() {
        let cache = MemoryCache::new();
        // 1 秒前过期
        cache
            .items
            .lock()
            .unwrap()
            .insert("k".to_owned(), ("v".to_owned(), MemoryCache::now() - 1));

        assert_eq!(cache.get("k"), None);
    }

    /// 记录调用的假存储，用于验证外部 store 注入链路
    #[derive(Debug, Default)]
    struct RecordingStore {
        items: Mutex<HashMap<String, String>>,
        writes: Mutex<u32>,
    }

    impl CacheStore for RecordingStore {
        fn get(&self, key: &str) -> Option<String> {
            self.items.lock().unwrap().get(key).cloned()
        }

        fn set(&self, key: &str, value: &str, _ttl_seconds: u64) {
            *self.writes.lock().unwrap() += 1;
            self.items
                .lock()
                .unwrap()
                .insert(key.to_owned(), value.to_owned());
        }

        fn forget(&self, key: &str) {
            self.items.lock().unwrap().remove(key);
        }

        fn keys(&self) -> Vec<String> {
            self.items.lock().unwrap().keys().cloned().collect()
        }
    }

    #[test]
    fn access_token_uses_injected_store() {
        let _guard = TEST_GUARD.lock().unwrap_or_else(|e| e.into_inner());
        let store = Arc::new(RecordingStore::default());
        set_cache_store(store.clone());

        let token = crate::AccessToken::new("official_account", "wx123", "secret");

        token.set(crate::Token {
            access_token: "from-external-store".to_owned(),
            create_time: 1,
            expires: 7200,
        });

        // 数据落在注入的存储里，而不是进程内默认缓存
        assert_eq!(*store.writes.lock().unwrap(), 1);
        assert_eq!(
            store.get(&token.get_key()),
            Some(
                serde_json::to_string(&crate::Token {
                    access_token: "from-external-store".to_owned(),
                    create_time: 1,
                    expires: 7200,
                })
                .unwrap()
            )
        );
        assert_eq!(
            token.get_cached().map(|t| t.access_token),
            Some("from-external-store".to_owned())
        );

        token.forget();
        assert!(token.get_cached().is_none());

        reset_cache_store();
    }

    #[test]
    fn global_store_roundtrip() {
        let _guard = TEST_GUARD.lock().unwrap_or_else(|e| e.into_inner());
        reset_cache_store();
        set("token", &"access-token-value", 7200);

        assert_eq!(get::<String>("token"), Some("access-token-value".to_owned()));

        forget("token");
        assert_eq!(get::<String>("token"), None);
    }
}
