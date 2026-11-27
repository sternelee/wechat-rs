//! copyright © ecdata.cn 2021 - present
//! 企业微信服务商各类 token，对齐 EasyWeChat\OpenWork\
//! ProviderAccessToken / SuiteAccessToken / SuiteTicket / AuthorizerAccessToken

use serde_json::{json, Value};
use wechat_sdk::{cache, constant::WORK_API_DOMAIN, Client, WechatResult as Result};

/// access_token 默认缓存时间（秒），对齐 expires_in
pub const TOKEN_TTL: u64 = 7200;

/// suite_ticket 缓存时间（秒），对齐 SuiteTicket::setTicket 的 6000
pub const SUITE_TICKET_TTL: u64 = 6000;

fn cache_get(key: &str) -> Option<String> {
    cache::get_raw(key)
}

fn cache_set(key: &str, value: &str, ttl: u64) {
    cache::set_raw(key, value, ttl);
}

/// 服务商 access_token，对齐 ProviderAccessToken
#[derive(Debug, Clone)]
pub struct ProviderAccessToken {
    corp_id: String,
    provider_secret: String,
    key: Option<String>,
    client: Client,
}

impl ProviderAccessToken {
    /// 创建 token 管理器
    pub fn new<S: Into<String>>(corp_id: S, provider_secret: S) -> ProviderAccessToken {
        ProviderAccessToken {
            corp_id: corp_id.into(),
            provider_secret: provider_secret.into(),
            key: None,
            client: Client::default(),
        }
    }

    /// 缓存 key，对齐 getKey
    pub fn get_key(&self) -> String {
        match &self.key {
            Some(key) => key.clone(),
            None => format!(
                "open_work.access_token.{}.{}",
                self.corp_id, self.provider_secret
            ),
        }
    }

    /// 自定义缓存 key
    pub fn set_key<S: Into<String>>(mut self, key: S) -> Self {
        self.key = Some(key.into());
        self
    }

    /// 获取 token
    pub async fn get_token(&self) -> Result<String> {
        match cache_get(&self.get_key()) {
            Some(token) => Ok(token),
            None => self.refresh().await,
        }
    }

    /// 刷新 token：POST /cgi-bin/service/get_provider_token
    pub async fn refresh(&self) -> Result<String> {
        let url = format!("{}/cgi-bin/service/get_provider_token", WORK_API_DOMAIN);
        let res: Value = self
            .client
            .post_json(
                &url,
                &json!({
                    "corpid": self.corp_id,
                    "provider_secret": self.provider_secret,
                }),
            )
            .await?;

        let token = res["provider_access_token"].as_str().unwrap_or_default();

        if token.is_empty() {
            return Err(error! {
                code: res["errcode"].as_i64().unwrap_or(-1) as i32,
                msg: format!("Failed to get provider_access_token: {}", res)
            });
        }

        cache_set(&self.get_key(), token, TOKEN_TTL);

        Ok(token.to_owned())
    }

    /// 请求参数，对齐 toQuery
    pub async fn to_query(&self) -> Result<Vec<(String, String)>> {
        Ok(vec![(
            "provider_access_token".to_owned(),
            self.get_token().await?,
        )])
    }
}

/// suite_ticket 存取，对齐 SuiteTicket
///
/// 由服务商回调（suite_ticket 事件）写入
#[derive(Debug, Clone)]
pub struct SuiteTicket {
    suite_id: String,
    key: Option<String>,
}

impl SuiteTicket {
    /// 创建 suite_ticket 存取器
    pub fn new<S: Into<String>>(suite_id: S) -> SuiteTicket {
        SuiteTicket {
            suite_id: suite_id.into(),
            key: None,
        }
    }

    /// 缓存 key，对齐 getKey
    pub fn get_key(&self) -> String {
        match &self.key {
            Some(key) => key.clone(),
            None => format!("open_work.suite_ticket.{}", self.suite_id),
        }
    }

    /// 自定义缓存 key
    pub fn set_key<S: Into<String>>(mut self, key: S) -> Self {
        self.key = Some(key.into());
        self
    }

    /// 写入 ticket，对齐 setTicket
    pub fn set_ticket(&self, ticket: &str) -> &Self {
        cache_set(&self.get_key(), ticket, SUITE_TICKET_TTL);
        self
    }

    /// 读取 ticket，未设置时报错（对齐 RuntimeException('No suite_ticket found.')）
    pub fn get_ticket(&self) -> Result<String> {
        match cache_get(&self.get_key()) {
            Some(ticket) => Ok(ticket),
            None => Err(error! {code: -1, msg: "No suite_ticket found."}),
        }
    }
}

/// 第三方应用 access_token，对齐 SuiteAccessToken
#[derive(Debug, Clone)]
pub struct SuiteAccessToken {
    suite_id: String,
    suite_secret: String,
    suite_ticket: SuiteTicket,
    key: Option<String>,
    client: Client,
}

impl SuiteAccessToken {
    /// 创建 token 管理器
    pub fn new<S: Into<String>>(
        suite_id: S,
        suite_secret: S,
        suite_ticket: SuiteTicket,
    ) -> SuiteAccessToken {
        SuiteAccessToken {
            suite_id: suite_id.into(),
            suite_secret: suite_secret.into(),
            suite_ticket,
            key: None,
            client: Client::default(),
        }
    }

    /// 缓存 key，对齐 getKey
    pub fn get_key(&self) -> String {
        match &self.key {
            Some(key) => key.clone(),
            None => format!(
                "open_work.suite_access_token.{}.{}",
                self.suite_id, self.suite_secret
            ),
        }
    }

    /// 自定义缓存 key
    pub fn set_key<S: Into<String>>(mut self, key: S) -> Self {
        self.key = Some(key.into());
        self
    }

    /// suite_ticket 存取器
    pub fn suite_ticket(&self) -> &SuiteTicket {
        &self.suite_ticket
    }

    /// 获取 token
    pub async fn get_token(&self) -> Result<String> {
        match cache_get(&self.get_key()) {
            Some(token) => Ok(token),
            None => self.refresh().await,
        }
    }

    /// 刷新 token：POST /cgi-bin/service/get_suite_token
    pub async fn refresh(&self) -> Result<String> {
        let url = format!("{}/cgi-bin/service/get_suite_token", WORK_API_DOMAIN);
        let ticket = self.suite_ticket.get_ticket()?;
        let res: Value = self
            .client
            .post_json(
                &url,
                &json!({
                    "suite_id": self.suite_id,
                    "suite_secret": self.suite_secret,
                    "suite_ticket": ticket,
                }),
            )
            .await?;

        let token = res["suite_access_token"].as_str().unwrap_or_default();

        if token.is_empty() {
            return Err(error! {
                code: res["errcode"].as_i64().unwrap_or(-1) as i32,
                msg: format!("Failed to get suite_access_token: {}", res)
            });
        }

        cache_set(&self.get_key(), token, TOKEN_TTL);

        Ok(token.to_owned())
    }

    /// 请求参数，对齐 toQuery
    pub async fn to_query(&self) -> Result<Vec<(String, String)>> {
        Ok(vec![(
            "suite_access_token".to_owned(),
            self.get_token().await?,
        )])
    }
}

/// 授权企业 access_token，对齐 AuthorizerAccessToken
#[derive(Debug, Clone)]
pub struct AuthorizerAccessToken {
    corp_id: String,
    /// 永久授权码或直接给定的 access_token
    permanent_code_or_access_token: String,
    suite_access_token: Option<SuiteAccessToken>,
    key: Option<String>,
    client: Client,
}

impl AuthorizerAccessToken {
    /// 创建 token 管理器
    pub fn new<S: Into<String>>(
        corp_id: S,
        permanent_code_or_access_token: S,
        suite_access_token: Option<SuiteAccessToken>,
    ) -> AuthorizerAccessToken {
        AuthorizerAccessToken {
            corp_id: corp_id.into(),
            permanent_code_or_access_token: permanent_code_or_access_token.into(),
            suite_access_token,
            key: None,
            client: Client::default(),
        }
    }

    /// 授权企业 corp_id
    pub fn corp_id(&self) -> &str {
        &self.corp_id
    }

    /// 缓存 key，对齐 getKey
    pub fn get_key(&self) -> String {
        match &self.key {
            Some(key) => key.clone(),
            None => format!(
                "open_work.authorizer.access_token.{}.{}",
                self.corp_id, self.permanent_code_or_access_token
            ),
        }
    }

    /// 自定义缓存 key
    pub fn set_key<S: Into<String>>(mut self, key: S) -> Self {
        self.key = Some(key.into());
        self
    }

    /// 获取 token：未配置 suite_access_token 时直接返回传入值
    pub async fn get_token(&self) -> Result<String> {
        if self.suite_access_token.is_none() {
            return Ok(self.permanent_code_or_access_token.clone());
        }

        match cache_get(&self.get_key()) {
            Some(token) => Ok(token),
            None => self.refresh().await,
        }
    }

    /// 刷新 token：POST /cgi-bin/service/get_corp_token
    pub async fn refresh(&self) -> Result<String> {
        let suite_access_token = match &self.suite_access_token {
            Some(token) => token.get_token().await?,
            None => return Ok(String::new()),
        };

        let url = wechat_sdk::Client::build_url(
            WORK_API_DOMAIN,
            "/cgi-bin/service/get_corp_token",
            &[("suite_access_token", suite_access_token.as_str())],
        );
        let res: Value = self
            .client
            .post_json(
                &url,
                &json!({
                    "auth_corpid": self.corp_id,
                    "permanent_code": self.permanent_code_or_access_token,
                }),
            )
            .await?;

        let token = res["access_token"].as_str().unwrap_or_default();

        if token.is_empty() {
            return Err(error! {
                code: res["errcode"].as_i64().unwrap_or(-1) as i32,
                msg: format!("Failed to get access_token: {}", res)
            });
        }

        cache_set(&self.get_key(), token, TOKEN_TTL);

        Ok(token.to_owned())
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
    fn provider_token_key() {
        let token = ProviderAccessToken::new("corp", "secret");
        assert_eq!(token.get_key(), "open_work.access_token.corp.secret");
    }

    #[test]
    fn suite_ticket_roundtrip() {
        let ticket = SuiteTicket::new("suite");
        assert!(ticket.get_ticket().is_err());

        ticket.set_ticket("ticket-value");
        assert_eq!(ticket.get_ticket().unwrap(), "ticket-value");
        assert_eq!(ticket.get_key(), "open_work.suite_ticket.suite");
    }

    #[test]
    fn authorizer_token_without_suite_token_returns_input() {
        let token = AuthorizerAccessToken::new("corp", "direct-token", None);
        assert_eq!(
            tokio::runtime::Runtime::new()
                .unwrap()
                .block_on(token.get_token())
                .unwrap(),
            "direct-token"
        );
    }
}
