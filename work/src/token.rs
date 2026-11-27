//! copyright © ecdata.cn 2021 - present
//! 企业微信 access_token，对齐 EasyWeChat\Work\AccessToken
//! created by shaipe 20210228

use serde_json::Value;
use wechat_sdk::{cache, constant::WORK_API_DOMAIN, Client, WechatResult as Result};

/// 企业微信 access_token 管理器
#[derive(Debug, Clone)]
pub struct AccessToken {
    corp_id: String,
    secret: String,
    key: Option<String>,
    client: Client,
}

impl AccessToken {
    /// 创建 manager
    pub fn new<S: Into<String>>(corp_id: S, secret: S) -> AccessToken {
        AccessToken {
            corp_id: corp_id.into(),
            secret: secret.into(),
            key: None,
            client: Client::default(),
        }
    }

    /// 自定义缓存 key
    pub fn set_key<S: Into<String>>(mut self, key: S) -> Self {
        self.key = Some(key.into());
        self
    }

    /// 获取缓存 key，对齐 getKey
    pub fn get_key(&self) -> String {
        match &self.key {
            Some(key) => key.clone(),
            // 对齐 PHP：work.access_token.{corpId}.{secret}
            None => format!("work.access_token.{}.{}", self.corp_id, self.secret),
        }
    }

    /// 获取 access_token，缓存有效时直接返回，对齐 getToken
    pub async fn get_token(&self) -> Result<String> {
        if let Some(token) = Self::get_cached(&self.get_key()) {
            return Ok(token);
        }

        self.refresh().await
    }

    /// 刷新 access_token，对齐 refresh
    pub async fn refresh(&self) -> Result<String> {
        let url = Client::build_url(
            WORK_API_DOMAIN,
            "/cgi-bin/gettoken",
            &[
                ("corpid", self.corp_id.as_str()),
                ("corpsecret", self.secret.as_str()),
            ],
        );

        let res: Value = self.client.get_json(&url).await?;
        let token = res["access_token"].as_str().unwrap_or_default();

        if token.is_empty() {
            return Err(error! {
                code: res["errcode"].as_i64().unwrap_or(-1) as i32,
                msg: format!("Failed to get access_token: {}", res)
            });
        }

        Self::set_cached(&self.get_key(), token);

        Ok(token.to_owned())
    }

    /// 读取缓存
    pub fn get_cached(key: &str) -> Option<String> {
        cache::get_raw(key)
    }

    /// 写入缓存
    pub fn set_cached(key: &str, token: &str) {
        cache::set_raw(key, token, 7200);
    }

    /// 清空缓存
    pub fn clear_cache() {
        cache::forget_prefix("work.access_token");
    }

    /// 请求参数，对齐 toQuery
    pub async fn to_query(&self) -> Result<Vec<(String, String)>> {
        Ok(vec![("access_token".to_owned(), self.get_token().await?)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_key() {
        let token = AccessToken::new("corp", "secret");
        assert_eq!(token.get_key(), "work.access_token.corp.secret");
    }

    #[test]
    fn custom_key_and_cache() {
        let token = AccessToken::new("corp", "secret").set_key("my_key");
        assert_eq!(token.get_key(), "my_key");

        AccessToken::set_cached("my_key", "token_value");
        assert_eq!(AccessToken::get_cached("my_key").as_deref(), Some("token_value"));

        // 默认 key 走 clear_cache 前缀清理；自定义 key 需单独 forget
        AccessToken::clear_cache();
        assert!(AccessToken::get_cached("work.access_token.corp.secret").is_none());
        cache::forget("my_key");
        assert!(AccessToken::get_cached("my_key").is_none());
    }
}
