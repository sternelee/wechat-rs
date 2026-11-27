//! copyright © ecdata.cn 2021 - present
//! 公众号应用入口，对齐 EasyWeChat\OfficialAccount\Application
//! created by shaipe 20210302

use crate::config::{Account, MpConfig};
use crate::oauth::OAuth;
use crate::server::MpServer;
use crate::utils::Utils;
use serde::Serialize;
use serde_json::Value;
use wechat_sdk::{
    constant::API_DOMAIN, AccessToken, Client, ComponentOAuth, Encryptor, JsApiTicket, Server,
    WechatResult as Result,
};

/// 公众号应用，聚合账号、token、ticket、消息加解密与 HTTP 调用
///
/// ```
/// use wechat_mp::{Application, MpConfig};
/// let app = Application::new(MpConfig::new("wx123", "secret"));
/// assert_eq!(app.account().app_id(), "wx123");
/// ```
#[derive(Debug, Clone)]
pub struct Application {
    config: MpConfig,
    access_token: AccessToken,
    ticket: JsApiTicket,
    client: Client,
    oauth_component: Option<ComponentOAuth>,
    encryptor: Option<Encryptor>,
}

impl Application {
    /// 创建应用
    pub fn new(config: MpConfig) -> Application {
        let account = config.account.clone();
        let access_token = AccessToken::new("official_account", account.app_id(), account.secret())
            .stable(config.use_stable_access_token);
        let ticket = JsApiTicket::new("official_account", account.app_id(), account.secret());

        Application {
            config,
            access_token,
            ticket,
            client: Client::default(),
            oauth_component: None,
            encryptor: None,
        }
    }

    /// 注入消息加解密器（第三方平台代公众号时使用组件侧 Encryptor）
    pub fn with_encryptor(mut self, encryptor: Encryptor) -> Self {
        self.encryptor = Some(encryptor);
        self
    }

    /// 第三方平台代公众号授权，注入组件上下文（component_appid + 组件 token）
    pub fn with_oauth_component(mut self, component: ComponentOAuth) -> Self {
        self.oauth_component = Some(component);
        self
    }

    /// 使用外部注入的 access_token（第三方平台代公众号调用时使用）
    pub fn with_access_token(mut self, access_token: AccessToken) -> Self {
        self.access_token = access_token;
        self.ticket = JsApiTicket::new(
            "official_account",
            self.config.account.app_id(),
            self.config.account.secret(),
        );
        self
    }

    /// 配置
    pub fn config(&self) -> &MpConfig {
        &self.config
    }

    /// 账号信息
    pub fn account(&self) -> &Account {
        &self.config.account
    }

    /// HTTP 客户端
    pub fn http_client(&self) -> &Client {
        &self.client
    }

    /// 消息加解密器（未配置 token/aes_key 时返回错误，对齐 getEncryptor）
    pub fn encryptor(&self) -> Result<Encryptor> {
        let account = self.account();
        if account.token().is_empty() || account.aes_key().is_empty() {
            return Err(error! {code: -1, msg: "token or aes_key cannot be empty."});
        }

        Encryptor::new(
            account.app_id(),
            account.token(),
            account.aes_key(),
            Some(account.app_id()),
        )
    }

    /// 消息服务器，对齐 OfficialAccount\Application::getServer
    pub fn server(&self) -> Result<Server> {
        let account = self.account();
        let aes_key = if account.aes_key().is_empty() {
            None
        } else {
            Some(account.aes_key())
        };

        let server = Server::new(
            account.app_id(),
            account.token(),
            aes_key,
            self.config.require_encryption,
        )?;

        Ok(match &self.encryptor {
            Some(encryptor) => server.with_encryptor(encryptor.clone()),
            None => server,
        })
    }

    /// access_token 管理器
    pub fn access_token(&self) -> &AccessToken {
        &self.access_token
    }

    /// 获取 access_token
    pub async fn get_access_token(&self) -> Result<String> {
        self.access_token.get_token().await
    }

    /// JsApiTicket 管理器
    pub fn ticket(&self) -> &JsApiTicket {
        &self.ticket
    }

    /// 获取 jsapi_ticket
    pub async fn get_jsapi_ticket(&self) -> Result<String> {
        self.ticket.get_ticket().await
    }

    /// JS-SDK 工具
    pub fn utils(&self) -> Utils<'_> {
        Utils::new(self)
    }

    /// OAuth 授权
    pub fn oauth(&self) -> OAuth {
        OAuth::new(
            self.account().app_id(),
            self.account().secret(),
            self.config
                .oauth_redirect_url
                .clone()
                .unwrap_or_default()
                .as_str(),
        )
        .with_scopes(self.config.oauth_scopes.clone())
        .pipe_component(self.oauth_component.clone())
    }

    /// 消息服务器
    pub fn message_server(&self) -> Result<MpServer> {
        MpServer::new(self)
    }

    /// 以 access_token 发起 GET 请求，对齐 createClient()->get()
    ///
    /// 42001/40001 等 token 过期错误会先刷新 access_token 再重试一次
    /// （对齐 AccessTokenAwareClient + AccessTokenExpiredRetryStrategy）。
    pub async fn get(&self, path: &str, query: &[(&str, &str)]) -> Result<Value> {
        let token = self.get_access_token().await?;

        let res = self
            .client
            .request_with_access_token(
                &token,
                |access_token| {
                    let mut all: Vec<(&str, &str)> = vec![("access_token", access_token)];
                    all.extend(query.iter().cloned());
                    Client::build_url(API_DOMAIN, path, &all)
                },
                || self.refresh_access_token(),
            )
            .await?;

        wechat_sdk::json_decode(&res)
    }

    /// 以 access_token 发起 POST 请求，对齐 createClient()->post()
    pub async fn post<T: Serialize + ?Sized>(&self, path: &str, body: &T) -> Result<Value> {
        let token = self.get_access_token().await?;

        let res = self
            .client
            .post_with_access_token(
                &token,
                body,
                |access_token| {
                    Client::build_url(API_DOMAIN, path, &[("access_token", access_token)])
                },
                || self.refresh_access_token(),
            )
            .await?;

        wechat_sdk::json_decode(&res)
    }

    /// 强制刷新 access_token（供 token 过期重试使用）
    async fn refresh_access_token(&self) -> Result<String> {
        Ok(self.access_token.refresh_forced().await?.access_token)
    }

    /// 无需 access_token 的 GET 请求（如 sns/oauth2/access_token）
    pub async fn get_without_token(&self, path: &str, query: &[(&str, &str)]) -> Result<Value> {
        let url = Client::build_url(API_DOMAIN, path, query);
        self.client.get_json(&url).await
    }

    /// 无需 access_token 的 POST 请求
    pub async fn post_without_token<T: Serialize + ?Sized>(
        &self,
        path: &str,
        body: &T,
    ) -> Result<Value> {
        let url = Client::build_url(API_DOMAIN, path, &[]);
        self.client.post_json(&url, body).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_and_oauth() {
        let app = Application::new(
            MpConfig::new("wx123", "secret").oauth("https://example.com/cb", vec!["snsapi_userinfo".into()]),
        );
        assert_eq!(app.account().app_id(), "wx123");
        assert_eq!(app.oauth().scopes(), &vec!["snsapi_userinfo".to_owned()]);

        let url = app.oauth().authorize_url("https://example.com/cb", "state1");
        assert!(url.contains("appid=wx123"));
        assert!(url.contains("scope=snsapi_userinfo"));
    }

    #[test]
    fn encryptor_requires_token_and_aes_key() {
        let app = Application::new(MpConfig::new("wx123", "secret"));
        assert!(app.encryptor().is_err());
    }

    #[test]
    fn server_only_uses_encryptor_when_aes_key_present() {
        let app = Application::new(MpConfig::new("wx123", "secret").with_message("token", ""));
        assert!(!app.server().unwrap().has_encryptor());

        let app = Application::new(
            MpConfig::new("wx123", "secret").with_message("token", "kdjCGGJKSRjjhESfPO5lTSWtYS0v5pQX47skCkZczio"),
        );
        assert!(app.server().unwrap().has_encryptor());
    }
}
