//! copyright © ecdata.cn 2021 - present
//! 企业微信配置，对齐 EasyWeChat\Work\Account 与 Config
//! created by shaipe 20210228

use serde::Serialize;

/// 企业微信账号信息
#[derive(Debug, Clone, Default, Serialize)]
pub struct Account {
    /// 企业 corp_id
    pub corp_id: String,
    /// 应用 secret
    pub secret: String,
    /// 消息推送 token
    pub token: String,
    /// 消息加解密 EncodingAESKey
    pub aes_key: String,
    /// 应用 agent_id（非必需）
    pub agent_id: Option<i64>,
}

impl Account {
    /// 创建账号
    pub fn new<A, B>(corp_id: A, secret: B) -> Account
    where
        A: Into<String>,
        B: Into<String>,
    {
        Account {
            corp_id: corp_id.into(),
            secret: secret.into(),
            ..Default::default()
        }
    }

    /// 设置消息 token/aes_key
    pub fn with_message<T, K>(mut self, token: T, aes_key: K) -> Account
    where
        T: Into<String>,
        K: Into<String>,
    {
        self.token = token.into();
        self.aes_key = aes_key.into();
        self
    }

    /// 设置应用 agent_id
    pub fn with_agent_id(mut self, agent_id: i64) -> Account {
        self.agent_id = Some(agent_id);
        self
    }

    /// 是否配置了消息加密
    pub fn has_encryption(&self) -> bool {
        !self.token.is_empty() && !self.aes_key.is_empty()
    }
}

/// 企业微信应用配置，对齐 EasyWeChat\Work\Config
#[derive(Debug, Clone, Default, Serialize)]
pub struct WorkConfig {
    /// 账号信息
    pub account: Account,
    /// 消息回调是否需要加密（企业微信默认加密）
    pub require_encryption: bool,
    /// 网页授权回调地址
    pub oauth_redirect_url: String,
    /// 网页授权 scope，默认 snsapi_base
    pub oauth_scopes: Vec<String>,
}

impl WorkConfig {
    /// 创建配置
    pub fn new(account: Account) -> WorkConfig {
        WorkConfig {
            account,
            require_encryption: true,
            oauth_redirect_url: String::new(),
            oauth_scopes: vec!["snsapi_base".to_owned()],
        }
    }

    /// 从 JSON 配置构建，对齐 easywechat 配置键名
    pub fn from_value(value: &serde_json::Value) -> wechat_sdk::WechatResult<WorkConfig> {
        let corp_id = value["corp_id"].as_str().unwrap_or_default();
        if corp_id.is_empty() {
            return Err(error! {code: -1, msg: "Missing config: corp_id"});
        }

        let scopes = match value["oauth"]["scopes"].as_array() {
            Some(items) => items
                .iter()
                .filter_map(|item| item.as_str().map(|s| s.to_owned()))
                .collect(),
            None => vec!["snsapi_base".to_owned()],
        };

        Ok(WorkConfig {
            account: Account {
                corp_id: corp_id.to_owned(),
                secret: value["secret"].as_str().unwrap_or_default().to_owned(),
                token: value["token"].as_str().unwrap_or_default().to_owned(),
                aes_key: value["aes_key"].as_str().unwrap_or_default().to_owned(),
                agent_id: value["agent_id"].as_i64(),
            },
            require_encryption: value["require_encryption"].as_bool().unwrap_or(true),
            oauth_redirect_url: value["oauth"]["redirect_url"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
            oauth_scopes: scopes,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_account() {
        let account = Account::new("corp", "secret")
            .with_message("token", "aes")
            .with_agent_id(100001);

        assert!(account.has_encryption());
        assert_eq!(account.agent_id, Some(100001));
    }

    #[test]
    fn config_from_value() {
        let conf = WorkConfig::from_value(&serde_json::json!({
            "corp_id": "corp",
            "secret": "secret",
            "token": "token",
            "aes_key": "aes",
            "agent_id": 5,
            "oauth": {"redirect_url": "https://example.com/cb", "scopes": ["snsapi_privateinfo"]}
        }))
        .unwrap();

        assert_eq!(conf.account.corp_id, "corp");
        assert_eq!(conf.account.agent_id, Some(5));
        assert_eq!(conf.oauth_scopes, vec!["snsapi_privateinfo"]);
        assert_eq!(conf.oauth_redirect_url, "https://example.com/cb");
    }

    #[test]
    fn config_requires_corp_id() {
        assert!(WorkConfig::from_value(&serde_json::json!({})).is_err());
    }
}
