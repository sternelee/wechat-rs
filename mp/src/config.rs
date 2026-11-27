//! copyright © ecdata.cn 2021 - present
//! 公众号配置，对齐 EasyWeChat\OfficialAccount\Config 与 Account
//! created by shaipe 20210302

use serde_json::Value;
use wechat_sdk::WechatResult as Result;

/// 公众号账号信息，对齐 EasyWeChat\OfficialAccount\Account
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Account {
    app_id: String,
    secret: String,
    token: String,
    aes_key: String,
}

impl Account {
    /// 创建账号
    pub fn new<A: Into<String>, B: Into<String>, T: Into<String>, K: Into<String>>(
        app_id: A,
        secret: B,
        token: T,
        aes_key: K,
    ) -> Account {
        Account {
            app_id: app_id.into(),
            secret: secret.into(),
            token: token.into(),
            aes_key: aes_key.into(),
        }
    }

    /// 公众号 AppId
    pub fn app_id(&self) -> &str {
        &self.app_id
    }

    /// 公众号 AppSecret
    pub fn secret(&self) -> &str {
        &self.secret
    }

    /// 消息校验 Token
    pub fn token(&self) -> &str {
        &self.token
    }

    /// 消息加解密 EncodingAESKey
    pub fn aes_key(&self) -> &str {
        &self.aes_key
    }

    /// 是否配置了消息加密
    pub fn has_encryption(&self) -> bool {
        !self.token.is_empty() && !self.aes_key.is_empty()
    }
}

/// 公众号配置，对齐 EasyWeChat\OfficialAccount\Config
#[derive(Debug, Clone)]
pub struct MpConfig {
    /// 账号信息
    pub account: Account,
    /// 是否强制要求加密消息
    pub require_encryption: bool,
    /// 是否使用稳定版 access_token 接口
    pub use_stable_access_token: bool,
    /// OAuth 回调地址
    pub oauth_redirect_url: Option<String>,
    /// OAuth 授权作用域
    pub oauth_scopes: Vec<String>,
}

impl Default for MpConfig {
    fn default() -> Self {
        MpConfig {
            account: Account::default(),
            require_encryption: false,
            use_stable_access_token: false,
            oauth_redirect_url: None,
            oauth_scopes: vec!["snsapi_userinfo".to_owned()],
        }
    }
}

impl MpConfig {
    /// 使用 app_id / secret 创建配置
    pub fn new<S: Into<String>>(app_id: S, secret: S) -> MpConfig {
        MpConfig {
            account: Account::new(app_id, secret, "".to_owned(), "".to_owned()),
            ..Default::default()
        }
    }

    /// 设置消息 token / aes_key
    pub fn with_message<S: Into<String>>(mut self, token: S, aes_key: S) -> Self {
        self.account.token = token.into();
        self.account.aes_key = aes_key.into();
        self
    }

    /// 设置消息 token
    pub fn with_token<S: Into<String>>(mut self, token: S) -> Self {
        self.account.token = token.into();
        self
    }

    /// 设置 EncodingAESKey
    pub fn with_aes_key<S: Into<String>>(mut self, aes_key: S) -> Self {
        self.account.aes_key = aes_key.into();
        self
    }

    /// 是否强制加密
    pub fn require_encryption(mut self, require: bool) -> Self {
        self.require_encryption = require;
        self
    }

    /// 使用稳定版 access_token
    pub fn stable_access_token(mut self, stable: bool) -> Self {
        self.use_stable_access_token = stable;
        self
    }

    /// 设置 OAuth 回调与作用域
    pub fn oauth<S: Into<String>>(mut self, redirect_url: S, scopes: Vec<String>) -> Self {
        self.oauth_redirect_url = Some(redirect_url.into());
        if !scopes.is_empty() {
            self.oauth_scopes = scopes;
        }
        self
    }

    /// 从 JSON/YAML 转换后的配置构建，对齐 easywechat 配置键名
    ///
    /// 支持键：app_id、secret、token、aes_key、require_encryption、
    /// use_stable_access_token、oauth.redirect_url、oauth.scopes
    pub fn from_value(value: &Value) -> Result<MpConfig> {
        let app_id = value["app_id"].as_str().unwrap_or_default();
        if app_id.is_empty() {
            return Err(error! {code: -1, msg: "Missing config: app_id"});
        }

        let mut conf = MpConfig::new(app_id, value["secret"].as_str().unwrap_or_default());

        if let Some(v) = value["token"].as_str() {
            conf.account.token = v.to_owned();
        }
        if let Some(v) = value["aes_key"].as_str() {
            conf.account.aes_key = v.to_owned();
        }
        if let Some(v) = value["require_encryption"].as_bool() {
            conf.require_encryption = v;
        }
        if let Some(v) = value["use_stable_access_token"].as_bool() {
            conf.use_stable_access_token = v;
        }
        if let Some(v) = value["oauth"]["redirect_url"].as_str() {
            conf.oauth_redirect_url = Some(v.to_owned());
        }
        if let Some(arr) = value["oauth"]["scopes"].as_array() {
            conf.oauth_scopes = arr
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_owned()))
                .collect();
        }

        Ok(conf)
    }

    /// 账号信息
    pub fn account(&self) -> &Account {
        &self.account
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_config_from_value() {
        let value = serde_json::json!({
            "app_id": "wx123",
            "secret": "secret",
            "token": "token",
            "aes_key": "aes",
            "require_encryption": true,
            "use_stable_access_token": true,
            "oauth": {"redirect_url": "https://example.com/cb", "scopes": ["snsapi_base"]}
        });
        let conf = MpConfig::from_value(&value).unwrap();
        assert_eq!(conf.account().app_id(), "wx123");
        assert_eq!(conf.account().aes_key(), "aes");
        assert!(conf.require_encryption);
        assert!(conf.use_stable_access_token);
        assert_eq!(conf.oauth_scopes, vec!["snsapi_base".to_owned()]);
    }

    #[test]
    fn missing_app_id_is_error() {
        assert!(MpConfig::from_value(&serde_json::json!({})).is_err());
    }
}
