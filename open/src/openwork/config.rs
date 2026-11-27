//! copyright © ecdata.cn 2021 - present
//! 企业微信服务商（开放平台）配置，对齐 EasyWeChat\OpenWork\Account / Config

/// 服务商账号信息，对齐 OpenWork\Account
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct Account {
    /// 服务商 corp_id
    pub corp_id: String,
    /// 服务商 provider_secret
    pub provider_secret: String,
    /// 第三方应用 suite_id
    pub suite_id: String,
    /// 第三方应用 suite_secret
    pub suite_secret: String,
    /// 消息推送 token
    pub token: String,
    /// 消息加解密 EncodingAESKey
    pub aes_key: String,
}

impl Account {
    /// 创建账号
    pub fn new<A, B, C, D, E, F>(
        corp_id: A,
        provider_secret: B,
        suite_id: C,
        suite_secret: D,
        token: E,
        aes_key: F,
    ) -> Account
    where
        A: Into<String>,
        B: Into<String>,
        C: Into<String>,
        D: Into<String>,
        E: Into<String>,
        F: Into<String>,
    {
        Account {
            corp_id: corp_id.into(),
            provider_secret: provider_secret.into(),
            suite_id: suite_id.into(),
            suite_secret: suite_secret.into(),
            token: token.into(),
            aes_key: aes_key.into(),
        }
    }

    /// 是否配置了消息加密
    pub fn has_encryption(&self) -> bool {
        !self.token.is_empty() && !self.aes_key.is_empty()
    }
}

/// 服务商配置，对齐 OpenWork\Config（requiredKeys）
#[derive(Debug, Clone, Default)]
pub struct OpenWorkConfig {
    /// 账号信息
    pub account: Account,
    /// 网页授权回调地址
    pub oauth_redirect_url: String,
    /// 网页授权 scope，默认 snsapi_base
    pub oauth_scopes: Vec<String>,
}

impl OpenWorkConfig {
    /// 创建配置
    pub fn new(account: Account) -> OpenWorkConfig {
        OpenWorkConfig {
            account,
            oauth_redirect_url: String::new(),
            oauth_scopes: vec!["snsapi_base".to_owned()],
        }
    }

    /// 从 JSON 构建，缺失必填项时报错（对齐 Config::requiredKeys）
    pub fn from_value(value: &serde_json::Value) -> wechat_sdk::WechatResult<OpenWorkConfig> {
        for key in ["corp_id", "suite_id", "provider_secret", "suite_secret", "token", "aes_key"] {
            if value[key].as_str().unwrap_or_default().is_empty() {
                return Err(error! {
                    code: -1,
                    msg: format!("Missing config: {}", key)
                });
            }
        }

        let scopes = match value["oauth"]["scopes"].as_array() {
            Some(items) => items
                .iter()
                .filter_map(|item| item.as_str().map(|s| s.to_owned()))
                .collect(),
            None => vec!["snsapi_base".to_owned()],
        };

        Ok(OpenWorkConfig {
            account: Account {
                corp_id: value["corp_id"].as_str().unwrap_or_default().to_owned(),
                provider_secret: value["provider_secret"].as_str().unwrap_or_default().to_owned(),
                suite_id: value["suite_id"].as_str().unwrap_or_default().to_owned(),
                suite_secret: value["suite_secret"].as_str().unwrap_or_default().to_owned(),
                token: value["token"].as_str().unwrap_or_default().to_owned(),
                aes_key: value["aes_key"].as_str().unwrap_or_default().to_owned(),
            },
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

    fn full() -> serde_json::Value {
        serde_json::json!({
            "corp_id": "corp",
            "provider_secret": "ps",
            "suite_id": "suite",
            "suite_secret": "ss",
            "token": "token",
            "aes_key": "aes",
            "oauth": {"redirect_url": "https://example.com/cb"}
        })
    }

    #[test]
    fn config_from_value() {
        let conf = OpenWorkConfig::from_value(&full()).unwrap();
        assert_eq!(conf.account.suite_id, "suite");
        assert!(conf.account.has_encryption());
        assert_eq!(conf.oauth_scopes, vec!["snsapi_base"]);
    }

    #[test]
    fn config_requires_all_keys() {
        let mut value = full();
        value["suite_secret"] = serde_json::json!("");
        let err = OpenWorkConfig::from_value(&value).unwrap_err();
        assert!(err.to_string().contains("suite_secret"));
    }
}
