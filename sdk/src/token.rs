//! copyright © ecdata.cn 2021 - present
//! 接口凭证管理，对齐 EasyWeChat\OfficialAccount\AccessToken / MiniApp\AccessToken / Work\AccessToken
//! created by shaipe 20211012
//!
//! 与 easywechat 保持一致：
//! - 缓存 key 包含 app_type/app_id/stable，不同应用不会互相覆盖
//! - 支持 stable_token（POST /cgi-bin/stable_token）
//! - 取缓存时会校验有效期，过期自动刷新

use crate::{cache, constant::API_DOMAIN, Client, WechatResult as Result};

// 默认缓存前缀

// 默认提前刷新时间（秒）
const REFRESH_AHEAD: u64 = 200;

/// Access Token对象
#[derive(Debug, Clone, Default)]
pub struct AccessToken {
    // 应用类型
    pub app_type: String,
    // 应用id
    pub app_id: String,
    // 密钥
    pub secret: String,
    // 是否使用稳定版 access_token
    pub stable: bool,
    // 自定义缓存 key
    pub key: Option<String>,
}

/// Token信息
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Token {
    // 访问token
    pub access_token: String,
    // access_token获取时间
    pub create_time: u64,
    // 有效期
    pub expires: i64,
}

impl AccessToken {
    /// 创建一个 token 配置实例
    pub fn new(app_type: &str, app_id: &str, secret: &str) -> AccessToken {
        AccessToken {
            app_type: app_type.to_owned(),
            app_id: app_id.to_owned(),
            secret: secret.to_owned(),
            stable: false,
            key: None,
        }
    }

    /// 使用稳定版 access_token（对齐 easywechat 的 use_stable_access_token）
    pub fn stable(mut self, stable: bool) -> Self {
        self.stable = stable;
        self
    }

    /// 自定义缓存 key
    pub fn set_key(mut self, key: &str) -> Self {
        self.key = Some(key.to_owned());
        self
    }

    /// 缓存 key，对齐 easywechat `{prefix}.access_token.{appId}.{secret}.{stable}`
    pub fn get_key(&self) -> String {
        if let Some(key) = &self.key {
            return key.clone();
        }
        // 对齐 PHP：{app_type}.access_token.{appId}.{secret}.{stable}
        format!(
            "{}.access_token.{}.{}.{}",
            self.app_type,
            self.app_id,
            self.secret,
            if self.stable { "1" } else { "0" }
        )
    }

    /// 获取微信授权的accessToken
    pub async fn get_access_token(&self, grant_type: &str) -> Result<Token> {
        // 组装请求地址
        let url = format!(
            "{domain}/cgi-bin/token?grant_type={grant_type}&appid={app_id}&secret={secret}",
            domain = API_DOMAIN,
            grant_type = if grant_type.is_empty() {
                "client_credential"
            } else {
                grant_type
            },
            app_id = self.app_id,
            secret = self.secret
        );

        let data = Client::new().get_json(&url).await?;
        self.parse_token(&data)
    }

    /// 获取稳定版 access_token，POST /cgi-bin/stable_token
    pub async fn get_stable_access_token(&self, force_refresh: bool) -> Result<Token> {
        let url = format!("{}/cgi-bin/stable_token", API_DOMAIN);
        let params = serde_json::json!({
            "grant_type": "client_credential",
            "appid": self.app_id,
            "secret": self.secret,
            "force_refresh": force_refresh,
        });
        let data = Client::new().post_json(&url, &params).await?;
        self.parse_token(&data)
    }

    /// 从返回结果中解析 token
    fn parse_token(&self, data: &serde_json::Value) -> Result<Token> {
        let token = match data["access_token"].as_str() {
            Some(s) => s.to_owned(),
            None => {
                return Err(error! {
                    code: -1,
                    msg: format!("Failed to get access_token: {}", data)
                })
            }
        };
        let expires = data["expires_in"].as_i64().unwrap_or(7200);
        Ok(Token {
            access_token: token,
            create_time: crate::current_timestamp(),
            expires,
        })
    }

    /// 拉取新的 token（不写缓存）
    async fn fetch_token(&self, force: bool) -> Result<Token> {
        if self.stable {
            self.get_stable_access_token(force).await
        } else {
            self.get_access_token("client_credential").await
        }
    }

    /// 刷新 token 并写入缓存
    ///
    /// 对齐 PHP `AccessToken::refresh()`：拿到新 token 后立即 `$this->cache->set($this->getKey(), ...)`，
    /// 否则缓存里仍是过期 token，后续每次调用都会重复「刷新 + 重试」。
    pub async fn refresh(&self) -> Result<Token> {
        self.refresh_with(|| self.fetch_token(false)).await
    }

    /// 强制刷新（忽略缓存；stable 模式带 force_refresh，对齐 AccessTokenExpiredRetryStrategy）
    pub async fn refresh_forced(&self) -> Result<Token> {
        self.refresh_with(|| self.fetch_token(true)).await
    }

    /// 用给定的抓取逻辑刷新并持久化，便于在测试中注入
    pub(crate) async fn refresh_with<F, Fut>(&self, fetch: F) -> Result<Token>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<Token>>,
    {
        let token = fetch().await?;
        self.set(token.clone());
        Ok(token)
    }

    /// 把 Token 写入缓存
    pub fn set(&self, val: Token) {
        let ttl = if val.expires > 0 { val.expires as u64 } else { 7200 };

        cache::set(&self.get_key(), &val, ttl);
    }

    /// 直接读取缓存，不校验有效期
    pub fn get_cached(&self) -> Option<Token> {
        cache::get(&self.get_key())
    }

    /// 清除缓存
    pub fn forget(&self) {
        cache::forget(&self.get_key());
    }

    /// 判断 token 是否已过期（提前 REFRESH_AHEAD 秒视为过期）
    pub fn is_expired(&self, token: &Token) -> bool {
        let now = crate::current_timestamp();
        let expires = if token.expires <= 0 { 7200 } else { token.expires as u64 };
        if now < token.create_time {
            return true;
        }
        now - token.create_time + REFRESH_AHEAD >= expires
    }

    /// 获取缓存中的缓存数据，不存在或已过期时自动刷新
    pub async fn get(&self) -> Option<Token> {
        if let Some(token) = self.get_cached() {
            if !token.access_token.is_empty() && !self.is_expired(&token) {
                return Some(token);
            }
        }

        match self.refresh().await {
            Ok(access) => Some(access),
            Err(err) => {
                log!("get access token error{:?}", err);
                None
            }
        }
    }

    /// 获取 access_token 字符串
    pub async fn get_token(&self) -> Result<String> {
        match self.get().await {
            Some(token) => Ok(token.access_token),
            None => Err(error!("access token is invalid")),
        }
    }

    /// 兼容旧接口：返回 access_token 请求参数
    pub fn to_query(&self, token: &str) -> Vec<(String, String)> {
        vec![("access_token".to_owned(), token.to_owned())]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_key_is_unique_per_app() {
        let a = AccessToken::new("mp", "wx1", "secret");
        let b = AccessToken::new("mp", "wx2", "secret");
        let c = AccessToken::new("weapp", "wx1", "secret");
        assert_ne!(a.get_key(), b.get_key());
        assert_ne!(a.get_key(), c.get_key());
        assert!(a.get_key().contains("wx1"));
    }

    #[test]
    fn stable_changes_key() {
        let a = AccessToken::new("mp", "wx1", "secret");
        let b = AccessToken::new("mp", "wx1", "secret").stable(true);
        assert_ne!(a.get_key(), b.get_key());
    }

    #[test]
    fn expired_detection() {
        let token = AccessToken::new("mp", "wx1", "secret");
        let fresh = Token {
            access_token: "abc".to_owned(),
            create_time: crate::current_timestamp(),
            expires: 7200,
        };
        assert!(!token.is_expired(&fresh));

        let old = Token {
            access_token: "abc".to_owned(),
            create_time: 1,
            expires: 7200,
        };
        assert!(token.is_expired(&old));
    }

    #[test]
    fn set_and_get_cache() {
        let _guard = crate::cache::TEST_GUARD
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let token = AccessToken::new("test", "wx_cache", "secret");
        token.set(Token {
            access_token: "token-value".to_owned(),
            create_time: crate::current_timestamp(),
            expires: 7200,
        });
        assert_eq!(token.get_cached().unwrap().access_token, "token-value");
    }

    #[tokio::test]
    async fn refresh_persists_new_token() {
        let _guard = crate::cache::TEST_GUARD
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let token = AccessToken::new("test", "wx_refresh", "secret");
        token.forget();
        assert!(token.get_cached().is_none());

        let refreshed = token
            .refresh_with(|| async {
                Ok(Token {
                    access_token: "fresh-token".to_owned(),
                    create_time: crate::current_timestamp(),
                    expires: 7200,
                })
            })
            .await
            .expect("refresh should succeed");

        assert_eq!(refreshed.access_token, "fresh-token");
        assert_eq!(
            token
                .get_cached()
                .expect("refreshed token must be cached")
                .access_token,
            "fresh-token"
        );
    }
}
