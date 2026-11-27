//! copyright © ecdata.cn 2021 - present
//! 开放平台配置，对齐 EasyWeChat\OpenPlatform\Config 与 Account

use serde_json::Value;
use wechat_sdk::{WechatError, WechatResult as Result};

/// 开放平台账号
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Account {
    /// 第三方平台 appid
    pub app_id: String,
    /// 第三方平台 app secret
    pub secret: String,
    /// 消息校验 token
    pub token: String,
    /// 消息加解密 key
    pub aes_key: String,
}

impl Account {
    /// 创建账号
    pub fn new<A, B, T, K>(app_id: A, secret: B, token: T, aes_key: K) -> Account
    where
        A: Into<String>,
        B: Into<String>,
        T: Into<String>,
        K: Into<String>,
    {
        Account {
            app_id: app_id.into(),
            secret: secret.into(),
            token: token.into(),
            aes_key: aes_key.into(),
        }
    }

    /// 是否配置了消息加密
    pub fn has_encryption(&self) -> bool {
        !self.token.is_empty() && !self.aes_key.is_empty()
    }
}

/// 开放平台配置
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OpenPlatformConfig {
    /// 账号
    pub account: Account,
    /// 回调地址，用于生成授权页
    pub redirect_url: String,
    /// 代公众号/小程序 OAuth 回调地址，对齐 oauth.redirect_url
    pub oauth_redirect_url: String,
    /// 代公众号/小程序 OAuth 作用域，对齐 oauth.scopes（默认 snsapi_userinfo）
    pub oauth_scopes: Vec<String>,
    /// 是否强制要求加密消息，透传给代公众号/小程序应用
    pub require_encryption: bool,
}

impl OpenPlatformConfig {
    /// 创建配置
    pub fn new(account: Account) -> OpenPlatformConfig {
        OpenPlatformConfig {
            account,
            redirect_url: String::new(),
            oauth_redirect_url: String::new(),
            oauth_scopes: vec!["snsapi_userinfo".to_owned()],
            require_encryption: false,
        }
    }

    /// 设置回调地址
    pub fn with_redirect_url<S: Into<String>>(mut self, url: S) -> Self {
        self.redirect_url = url.into();
        self
    }

    /// 设置代公众号/小程序 OAuth 回调与作用域，对齐 EasyWeChat `oauth` 配置段
    pub fn with_oauth<S: Into<String>>(mut self, redirect_url: S, scopes: Vec<String>) -> Self {
        self.oauth_redirect_url = redirect_url.into();
        if !scopes.is_empty() {
            self.oauth_scopes = scopes;
        }
        self
    }

    /// 是否强制要求加密消息
    pub fn require_encryption(mut self, require: bool) -> Self {
        self.require_encryption = require;
        self
    }

    /// 代公众号 OAuth 回调地址
    pub fn oauth_redirect_url(&self) -> &str {
        &self.oauth_redirect_url
    }

    /// 代公众号 OAuth 作用域
    pub fn oauth_scopes(&self) -> &[String] {
        &self.oauth_scopes
    }

    /// 从 json 读取配置，对齐 requiredKeys = [app_id, secret, aes_key]
    pub fn from_value(conf: &Value) -> Result<OpenPlatformConfig> {
        let app_id = conf
            .get("app_id")
            .and_then(Value::as_str)
            .ok_or_else(|| missing("app_id"))?;
        let secret = conf
            .get("secret")
            .and_then(Value::as_str)
            .ok_or_else(|| missing("secret"))?;
        let aes_key = conf
            .get("aes_key")
            .and_then(Value::as_str)
            .ok_or_else(|| missing("aes_key"))?;

        let token = conf
            .get("token")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let redirect_url = conf
            .get("redirect_url")
            .and_then(Value::as_str)
            .unwrap_or_default();

        let oauth_redirect_url = conf
            .get("oauth")
            .and_then(|oauth| oauth.get("redirect_url"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        let oauth_scopes: Vec<String> = conf
            .get("oauth")
            .and_then(|oauth| oauth.get("scopes"))
            .and_then(Value::as_array)
            .map(|scopes| {
                scopes
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        let require_encryption = conf
            .get("require_encryption")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        Ok(OpenPlatformConfig {
            account: Account::new(app_id, secret, token, aes_key),
            redirect_url: redirect_url.to_owned(),
            oauth_redirect_url: oauth_redirect_url.to_owned(),
            oauth_scopes: if oauth_scopes.is_empty() {
                vec!["snsapi_userinfo".to_owned()]
            } else {
                oauth_scopes
            },
            require_encryption,
        })
    }
}

fn missing(key: &str) -> WechatError {
    error! {
        code: -1,
        msg: format!("Missing required config key: {}", key),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn account_from_value() {
        let conf = OpenPlatformConfig::from_value(&json!({
            "app_id": "wx123",
            "secret": "secret",
            "token": "token",
            "aes_key": "aes",
            "redirect_url": "https://example.com/callback",
        }))
        .unwrap();

        assert_eq!(conf.account.app_id, "wx123");
        assert_eq!(conf.redirect_url, "https://example.com/callback");
        assert!(conf.account.has_encryption());
        assert_eq!(conf.oauth_scopes(), ["snsapi_userinfo".to_owned()]);
        assert!(conf.oauth_redirect_url().is_empty());
        assert!(!conf.require_encryption);
    }

    #[test]
    fn oauth_section_is_parsed() {
        let conf = OpenPlatformConfig::from_value(&json!({
            "app_id": "wx123",
            "secret": "secret",
            "aes_key": "aes",
            "require_encryption": true,
            "oauth": {
                "redirect_url": "https://example.com/oauth",
                "scopes": ["snsapi_base"],
            },
        }))
        .unwrap();

        assert_eq!(conf.oauth_redirect_url(), "https://example.com/oauth");
        assert_eq!(conf.oauth_scopes(), ["snsapi_base".to_owned()]);
        assert!(conf.require_encryption);
    }

    #[test]
    fn account_requires_keys() {
        assert!(OpenPlatformConfig::from_value(&json!({"secret": "s"})).is_err());
    }
}
