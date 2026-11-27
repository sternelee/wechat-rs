//! copyright © ecdata.cn 2021 - present
//! 开放平台各类 token，对齐 EasyWeChat\OpenPlatform\
//! ComponentAccessToken / VerifyTicket / AuthorizerAccessToken

use serde_json::{json, Value};
use wechat_sdk::{cache, constant::API_DOMAIN, Client, WechatResult as Result};

/// verify_ticket 缓存时间（秒），对齐 VerifyTicket::setTicket 的 6000 秒
pub const VERIFY_TICKET_TTL: u64 = 6000;
/// authorizer access_token 默认有效期（秒），对齐 7200 - 500
pub const DEFAULT_AUTHORIZER_TTL: u64 = 7200 - 500;
/// component access_token 提前刷新时间（秒）
pub const COMPONENT_REFRESH_AHEAD: u64 = 100;
/// 缓存 key 前缀，对齐各 getKey 实现
pub const COMPONENT_TOKEN_PREFIX: &str = "open_platform.component_access_token";
/// verify_ticket 缓存 key 前缀
pub const VERIFY_TICKET_PREFIX: &str = "open_platform.verify_ticket";
/// 授权方 token 缓存 key 前缀
pub const AUTHORIZER_TOKEN_PREFIX: &str = "open-platform.authorizer_access_token";

fn now() -> u64 {
    wechat_sdk::current_timestamp()
}

/// 第三方平台 component_access_token，对齐 ComponentAccessToken
#[derive(Debug, Clone)]
pub struct ComponentAccessToken {
    app_id: String,
    secret: String,
    verify_ticket: String,
    key: Option<String>,
    client: Client,
}

impl ComponentAccessToken {
    /// 创建 token 管理器
    pub fn new<A, B, T>(app_id: A, secret: B, verify_ticket: T) -> ComponentAccessToken
    where
        A: Into<String>,
        B: Into<String>,
        T: Into<String>,
    {
        ComponentAccessToken {
            app_id: app_id.into(),
            secret: secret.into(),
            verify_ticket: verify_ticket.into(),
            key: None,
            client: Client::default(),
        }
    }

    /// 覆写缓存 key
    pub fn set_key<S: Into<String>>(mut self, key: S) -> Self {
        self.key = Some(key.into());
        self
    }

    /// 缓存 key
    pub fn get_key(&self) -> String {
        match &self.key {
            Some(key) => key.clone(),
            None => format!("{}.{}", COMPONENT_TOKEN_PREFIX, self.app_id),
        }
    }

    /// 缓存中的 token
    pub fn get_cached(&self) -> Option<String> {
        cache::get_raw(&self.get_key())
    }

    /// 写入缓存（expires_at 为绝对时间戳）
    pub fn set_cached(&self, token: &str, expires_at: u64) {
        let key = self.get_key();
        let ttl = expires_at.saturating_sub(now());

        if ttl == 0 {
            cache::forget(&key);
            return;
        }

        cache::set_raw(&key, token, ttl);
    }

    /// 刷新 token，POST cgi-bin/component/api_component_token
    pub async fn refresh(&self) -> Result<String> {
        let url = format!("{}/cgi-bin/component/api_component_token", API_DOMAIN);
        let body = json!({
            "component_appid": self.app_id,
            "component_appsecret": self.secret,
            "component_verify_ticket": self.verify_ticket,
        });

        let data = self.client.post_json(&url, &body).await?;
        let token = data
            .get("component_access_token")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                error! {
                    code: -1,
                    msg: format!("Failed to get component_access_token: {}", data),
                }
            })?;

        let expires_in = data
            .get("expires_in")
            .and_then(Value::as_u64)
            .unwrap_or(7200)
            .saturating_sub(COMPONENT_REFRESH_AHEAD);

        self.set_cached(token, now() + expires_in);

        Ok(token.to_owned())
    }

    /// 获取 token，优先缓存
    pub async fn get_token(&self) -> Result<String> {
        if let Some(token) = self.get_cached() {
            return Ok(token);
        }

        self.refresh().await
    }

    /// 查询参数
    pub async fn to_query(&self) -> Result<Vec<(String, String)>> {
        Ok(vec![(
            "component_access_token".to_owned(),
            self.get_token().await?,
        )])
    }
}

/// 第三方平台 verify_ticket，对齐 VerifyTicket
#[derive(Debug, Clone)]
pub struct VerifyTicket {
    app_id: String,
    key: Option<String>,
}

impl VerifyTicket {
    /// 创建 ticket 管理器
    pub fn new<S: Into<String>>(app_id: S) -> VerifyTicket {
        VerifyTicket {
            app_id: app_id.into(),
            key: None,
        }
    }

    /// 覆写缓存 key
    pub fn set_key<S: Into<String>>(mut self, key: S) -> Self {
        self.key = Some(key.into());
        self
    }

    /// 缓存 key
    pub fn get_key(&self) -> String {
        match &self.key {
            Some(key) => key.clone(),
            None => format!("{}.{}", VERIFY_TICKET_PREFIX, self.app_id),
        }
    }

    /// 写入 ticket，有效期 6000 秒，对齐 VerifyTicket::setTicket
    pub fn set_ticket<S: Into<String>>(&self, ticket: S) {
        cache::set_raw(&self.get_key(), &ticket.into(), VERIFY_TICKET_TTL);
    }

    /// 读取 ticket，缺失时报错，对齐 'No component_verify_ticket found.'
    pub fn get_ticket(&self) -> Result<String> {
        cache::get_raw(&self.get_key()).ok_or_else(|| {
            error! {
                code: -1,
                msg: "No component_verify_ticket found.",
            }
        })
    }
}

/// 授权方 access_token，对齐 AuthorizerAccessToken
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AuthorizerAccessToken {
    app_id: String,
    access_token: String,
}

impl AuthorizerAccessToken {
    /// 创建 token
    pub fn new<A, T>(app_id: A, access_token: T) -> AuthorizerAccessToken
    where
        A: Into<String>,
        T: Into<String>,
    {
        AuthorizerAccessToken {
            app_id: app_id.into(),
            access_token: access_token.into(),
        }
    }

    /// 授权方 appid
    pub fn get_app_id(&self) -> &str {
        &self.app_id
    }

    /// token
    pub fn get_token(&self) -> &str {
        &self.access_token
    }

    /// 查询参数
    pub fn to_query(&self) -> Vec<(String, String)> {
        vec![("access_token".to_owned(), self.access_token.clone())]
    }
}

impl std::fmt::Display for AuthorizerAccessToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.access_token)
    }
}

/// 授权方 token 缓存 key，对齐 getAuthorizerAccessToken 的 `appId.md5(refresh_token)`
pub fn authorizer_cache_key(app_id: &str, refresh_token: &str) -> String {
    format!(
        "{}.{}.{}",
        AUTHORIZER_TOKEN_PREFIX,
        app_id,
        md5_hex(refresh_token)
    )
}

/// md5 十六进制
pub fn md5_hex(value: &str) -> String {
    format!("{:x}", md5::compute(value.as_bytes()))
}

/// 读取缓存的授权方 token
pub fn get_cached_authorizer_token(key: &str) -> Option<String> {
    cache::get_raw(key)
}

/// 写入授权方 token 缓存
pub fn set_cached_authorizer_token(key: &str, token: &str, ttl: u64) {
    cache::set_raw(key, token, ttl);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn component_token_keys() {
        let token = ComponentAccessToken::new("wx123", "secret", "ticket");
        assert_eq!(
            token.get_key(),
            "open_platform.component_access_token.wx123"
        );

        let custom = ComponentAccessToken::new("wx123", "secret", "ticket").set_key("custom");
        assert_eq!(custom.get_key(), "custom");
    }

    #[test]
    fn component_token_cache_roundtrip() {
        let token = ComponentAccessToken::new("wx-cache", "secret", "ticket");
        token.set_cached("token-value", now() + 100);
        assert_eq!(token.get_cached().as_deref(), Some("token-value"));

        token.set_cached("expired", now() - 1);
        assert!(token.get_cached().is_none());
    }

    #[test]
    fn verify_ticket_roundtrip() {
        let ticket = VerifyTicket::new("wx-ticket");
        assert!(ticket.get_ticket().is_err());

        ticket.set_ticket("ticket-value");
        assert_eq!(ticket.get_ticket().unwrap(), "ticket-value");
        assert_eq!(ticket.get_key(), "open_platform.verify_ticket.wx-ticket");
    }

    #[test]
    fn authorizer_token_helpers() {
        let token = AuthorizerAccessToken::new("wxauth", "token");
        assert_eq!(token.get_app_id(), "wxauth");
        assert_eq!(token.to_string(), "token");
        assert_eq!(token.to_query()[0].0, "access_token");

        let key = authorizer_cache_key("wxauth", "refresh");
        assert!(key.starts_with("open-platform.authorizer_access_token.wxauth."));

        set_cached_authorizer_token(&key, "cached", 60);
        assert_eq!(get_cached_authorizer_token(&key).as_deref(), Some("cached"));
        assert_eq!(md5_hex("abc"), "900150983cd24fb0d6963f7d28e17f72");
    }
}
