//! copyright © ecdata.cn 2021 - present
//! 企业微信应用入口，对齐 EasyWeChat\Work\Application
//! created by shaipe 20210228

use crate::config::{Account, WorkConfig};
use crate::encryptor::Encryptor;
use crate::oauth::OAuth;
use crate::server::Server;
use crate::ticket::JsApiTicket;
use crate::token::AccessToken;
use wechat_sdk::constant::WORK_API_DOMAIN;
use crate::utils::Utils;
use serde::Serialize;
use serde_json::Value;
use wechat_sdk::{Client, WechatResult as Result};

/// 企业微信主动请求的查询参数
pub type Query = Vec<(String, String)>;

/// 企业微信应用
#[derive(Debug, Clone)]
pub struct Application {
    config: WorkConfig,
    access_token: AccessToken,
    ticket: JsApiTicket,
    client: Client,
}

impl Application {
    /// 创建应用
    pub fn new(config: WorkConfig) -> Application {
        let access_token = AccessToken::new(&config.account.corp_id, &config.account.secret);
        let ticket = JsApiTicket::new(&config.account.corp_id, &config.account.secret);

        Application {
            config,
            access_token,
            ticket,
            client: Client::default(),
        }
    }

    /// 配置
    pub fn config(&self) -> &WorkConfig {
        &self.config
    }

    /// 账号信息，对齐 getAccount
    pub fn account(&self) -> &Account {
        &self.config.account
    }

    /// HTTP 客户端
    pub fn http_client(&self) -> &Client {
        &self.client
    }

    /// access_token 管理器
    pub fn access_token(&self) -> &AccessToken {
        &self.access_token
    }

    /// 获取 access_token
    pub async fn get_access_token(&self) -> Result<String> {
        self.access_token.get_token().await
    }

    /// jsapi_ticket 管理器，对齐 getTicket
    pub fn ticket(&self) -> &JsApiTicket {
        &self.ticket
    }

    /// 获取 jsapi_ticket
    pub async fn get_jsapi_ticket(&self) -> Result<String> {
        self.ticket.get_ticket().await
    }

    /// 消息加解密器，对齐 getEncryptor
    pub fn encryptor(&self) -> Result<Encryptor> {
        let account = &self.config.account;
        if !account.has_encryption() {
            return Err(error! {code: -1, msg: "token or aes_key cannot be empty."});
        }

        Encryptor::new(
            account.corp_id.as_str(),
            account.token.as_str(),
            account.aes_key.as_str(),
        )
    }

    /// 消息服务器，对齐 getServer
    pub fn server(&self) -> Result<Server> {
        let encryptor = if self.config.account.has_encryption() {
            Some(self.encryptor()?)
        } else {
            None
        };

        Ok(Server::new(encryptor))
    }

    /// 工具，对齐 getUtils
    pub fn utils(&self) -> Utils<'_> {
        Utils::new(self)
    }

    /// 网页授权组件，对齐 getOAuth()
    ///
    /// 默认带上应用的 token 管理器，`user_id()` / `user()` 会自动取用（必要时刷新）
    /// access_token；也可用 [`OAuth::with_api_access_token`] 显式指定，
    /// 对标 PHP 的 `withApiAccessToken($this->getAccessToken()->getToken())`。
    pub fn oauth(&self) -> OAuth {
        OAuth::from_config(&self.config).with_access_token(self.access_token.clone())
    }

    /// 刷新 access_token（供 token 过期重试用）
    async fn refresh_access_token(&self) -> Result<String> {
        self.access_token.refresh().await
    }

    /// 以 access_token 发起 GET（token 过期时刷新后重试一次）
    pub async fn get(&self, path: &str, query: &[(&str, &str)]) -> Result<Value> {
        let token = self.get_access_token().await?;

        let res = self
            .client
            .request_with_access_token(
                &token,
                |access_token| {
                    let mut params: Vec<(&str, &str)> = vec![("access_token", access_token)];
                    params.extend(query.iter().cloned());
                    Client::build_url(WORK_API_DOMAIN, path, &params)
                },
                || self.refresh_access_token(),
            )
            .await?;

        wechat_sdk::json_decode(&res)
    }

    /// 以 access_token 发起 POST（token 过期时刷新后重试一次）
    pub async fn post<T: Serialize + ?Sized>(&self, path: &str, body: &T) -> Result<Value> {
        let token = self.get_access_token().await?;

        let res = self
            .client
            .post_with_access_token(
                &token,
                body,
                |access_token| {
                    Client::build_url(WORK_API_DOMAIN, path, &[("access_token", access_token)])
                },
                || self.refresh_access_token(),
            )
            .await?;

        wechat_sdk::json_decode(&res)
    }

    /// 无需 access_token 的 GET
    pub async fn get_without_token(&self, path: &str, query: &[(&str, &str)]) -> Result<Value> {
        let url = Client::build_url(WORK_API_DOMAIN, path, query);
        self.client.get_json(&url).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> WorkConfig {
        WorkConfig::new(
            Account::new("corp", "secret").with_message("token", "kdjCGGJKSRjjhESfPO5lTSWtYS0v5pQX47skCkZczio"),
        )
    }

    #[test]
    fn server_has_encryptor_when_configured() {
        let app = Application::new(config());
        assert!(app.server().unwrap().has_encryptor());
        assert!(app.encryptor().is_ok());
    }

    #[test]
    fn server_without_message_config() {
        let app = Application::new(WorkConfig::new(Account::new("corp", "secret")));
        assert!(!app.server().unwrap().has_encryptor());
        assert!(app.encryptor().is_err());
    }

    #[test]
    fn oauth_uses_corp_and_agent() {
        let app = Application::new(
            WorkConfig::new(Account::new("corp", "secret").with_agent_id(100001)),
        );
        let oauth = app.oauth();

        assert_eq!(oauth.client_id(), "corp");
        assert_eq!(oauth.agent_id(), Some(100001));
        assert!(oauth
            .authorize_url_with("https://example.com/cb", "STATE")
            .contains("appid=corp"));
    }

    #[test]
    fn access_token_key_defaults() {
        let app = Application::new(config());
        assert_eq!(app.access_token().get_key(), "work.access_token.corp.secret");
        assert_eq!(app.ticket().get_key(), "work.jsapi_ticket.corp");
    }
}
