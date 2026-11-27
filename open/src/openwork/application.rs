//! copyright © ecdata.cn 2021 - present
//! 企业微信服务商应用入口，对齐 EasyWeChat\OpenWork\Application

use crate::openwork::config::{Account, OpenWorkConfig};
use crate::openwork::oauth::SuiteOAuth;
use crate::openwork::server::Server;
use crate::openwork::ticket::JsApiTicket;
use crate::openwork::token::{AuthorizerAccessToken, ProviderAccessToken, SuiteAccessToken, SuiteTicket};
use serde::Serialize;
use serde_json::{json, Value};
use wechat_sdk::{constant::WORK_API_DOMAIN, Client, Encryptor, WechatResult as Result};

/// 授权信息，对齐 OpenWork\Authorization 的 getAppId
pub fn authorization_app_id(authorization: &Value) -> String {
    authorization["auth_corp_info"]["corpid"]
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

/// 企业微信服务商应用
#[derive(Debug, Clone)]
pub struct Application {
    config: OpenWorkConfig,
    provider_access_token: ProviderAccessToken,
    suite_ticket: SuiteTicket,
    suite_access_token: SuiteAccessToken,
    client: Client,
}

impl Application {
    /// 创建应用
    pub fn new(config: OpenWorkConfig) -> Application {
        let account = config.account.clone();
        let provider_access_token =
            ProviderAccessToken::new(&account.corp_id, &account.provider_secret);
        let suite_ticket = SuiteTicket::new(&account.suite_id);
        let suite_access_token = SuiteAccessToken::new(
            &account.suite_id,
            &account.suite_secret,
            suite_ticket.clone(),
        );

        Application {
            config,
            provider_access_token,
            suite_ticket,
            suite_access_token,
            client: Client::default(),
        }
    }

    /// 配置
    pub fn config(&self) -> &OpenWorkConfig {
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

    /// 服务商 access_token，对齐 getProviderAccessToken
    pub fn provider_access_token(&self) -> &ProviderAccessToken {
        &self.provider_access_token
    }

    /// 获取服务商 access_token
    pub async fn get_provider_access_token(&self) -> Result<String> {
        self.provider_access_token.get_token().await
    }

    /// 第三方应用 access_token，对齐 getSuiteAccessToken
    pub fn suite_access_token(&self) -> &SuiteAccessToken {
        &self.suite_access_token
    }

    /// 获取第三方应用 access_token
    pub async fn get_suite_access_token(&self) -> Result<String> {
        self.suite_access_token.get_token().await
    }

    /// suite_ticket 存取器，对齐 getSuiteTicket
    pub fn suite_ticket(&self) -> &SuiteTicket {
        &self.suite_ticket
    }

    /// 服务商加解密器（receive_id 为空），对齐 getEncryptor
    pub fn encryptor(&self) -> Result<Encryptor> {
        let account = &self.config.account;
        if account.token.is_empty() || account.aes_key.is_empty() {
            return Err(error! {code: -1, msg: "token or aes_key cannot be empty."});
        }

        Encryptor::new(&account.corp_id, &account.token, &account.aes_key, None)
    }

    /// 第三方应用加解密器，对齐 getSuiteEncryptor
    pub fn suite_encryptor(&self) -> Result<Encryptor> {
        let account = &self.config.account;
        if account.token.is_empty() || account.aes_key.is_empty() {
            return Err(error! {code: -1, msg: "token or aes_key cannot be empty."});
        }

        Encryptor::new(&account.suite_id, &account.token, &account.aes_key, None)
    }

    /// 消息服务器，对齐 getServer
    ///
    /// 默认挂载 suite_ticket 处理器：收到 suite_ticket 事件时写入缓存
    pub fn server(&self) -> Result<Server> {
        let suite_encryptor = self.suite_encryptor().ok();
        let provider_encryptor = self.encryptor().ok();
        let suite_id = self.config.account.suite_id.clone();
        let ticket_store = self.suite_ticket.clone();

        let mut server = Server::new(suite_encryptor, provider_encryptor);
        server.handle_suite_ticket_refreshed(move |message| {
            if message["SuiteId"].as_str().unwrap_or_default() == suite_id {
                if let Some(ticket) = message["SuiteTicket"].as_str() {
                    ticket_store.set_ticket(ticket);
                }
            }
            None
        });

        Ok(server)
    }

    /// 授权信息查询，对齐 getAuthorization
    pub async fn get_authorization(&self, corp_id: &str, permanent_code: &str) -> Result<Value> {
        let suite_access_token = self.get_suite_access_token().await?;
        let url = Client::build_url(
            WORK_API_DOMAIN,
            "/cgi-bin/service/get_auth_info",
            &[("suite_access_token", suite_access_token.as_str())],
        );

        let res: Value = self
            .client
            .post_json(
                &url,
                &json!({
                    "auth_corpid": corp_id,
                    "permanent_code": permanent_code,
                }),
            )
            .await?;

        if res["auth_corp_info"].is_null() {
            return Err(error! {
                code: res["errcode"].as_i64().unwrap_or(-1) as i32,
                msg: format!("Failed to get auth_corp_info: {}", res)
            });
        }

        Ok(res)
    }

    /// 授权企业 access_token，对齐 getAuthorizerAccessToken
    pub fn authorizer_access_token(
        &self,
        corp_id: &str,
        permanent_code: &str,
    ) -> AuthorizerAccessToken {
        AuthorizerAccessToken::new(
            corp_id,
            permanent_code,
            Some(self.suite_access_token.clone()),
        )
    }

    /// 授权企业 jsapi_ticket，对齐 getJsApiTicket
    pub fn jsapi_ticket(&self, corp_id: &str, permanent_code: &str) -> JsApiTicket {
        JsApiTicket::new(
            corp_id,
            Some(self.authorizer_access_token(corp_id, permanent_code)),
        )
    }

    /// 以服务商 access_token 发起 GET（token 过期时刷新后重试一次）
    pub async fn get(&self, path: &str, query: &[(&str, &str)]) -> Result<Value> {
        let token = self.get_provider_access_token().await?;

        let res = self
            .client
            .request_with_access_token(
                &token,
                |access_token| {
                    let mut all: Vec<(&str, &str)> = vec![("provider_access_token", access_token)];
                    all.extend(query.iter().cloned());
                    Client::build_url(WORK_API_DOMAIN, path, &all)
                },
                || async move { self.provider_access_token().refresh().await },
            )
            .await?;

        wechat_sdk::json_decode(&res)
    }

    /// 以服务商 access_token 发起 POST（token 过期时刷新后重试一次）
    pub async fn post<T: Serialize + ?Sized>(&self, path: &str, body: &T) -> Result<Value> {
        let token = self.get_provider_access_token().await?;

        let res = self
            .client
            .post_with_access_token(
                &token,
                body,
                |access_token| {
                    Client::build_url(
                        WORK_API_DOMAIN,
                        path,
                        &[("provider_access_token", access_token)],
                    )
                },
                || async move { self.provider_access_token().refresh().await },
            )
            .await?;

        wechat_sdk::json_decode(&res)
    }

    /// 以授权企业 access_token 发起 GET，对齐 getAuthorizerClient
    pub async fn authorizer_get(
        &self,
        corp_id: &str,
        permanent_code: &str,
        path: &str,
        query: &[(&str, &str)],
    ) -> Result<Value> {
        let authorizer = self.authorizer_access_token(corp_id, permanent_code);
        let token = authorizer.get_token().await?;

        let res = self
            .client
            .request_with_access_token(
                &token,
                |access_token| {
                    let mut all: Vec<(&str, &str)> = vec![("access_token", access_token)];
                    all.extend(query.iter().cloned());
                    Client::build_url(WORK_API_DOMAIN, path, &all)
                },
                || {
                    let authorizer = authorizer.clone();
                    async move { authorizer.refresh().await }
                },
            )
            .await?;

        wechat_sdk::json_decode(&res)
    }

    /// 以授权企业 access_token 发起 POST
    pub async fn authorizer_post<T: Serialize + ?Sized>(
        &self,
        corp_id: &str,
        permanent_code: &str,
        path: &str,
        body: &T,
    ) -> Result<Value> {
        let authorizer = self.authorizer_access_token(corp_id, permanent_code);
        let token = authorizer.get_token().await?;

        let res = self
            .client
            .post_with_access_token(
                &token,
                body,
                |access_token| {
                    Client::build_url(WORK_API_DOMAIN, path, &[("access_token", access_token)])
                },
                || {
                    let authorizer = authorizer.clone();
                    async move { authorizer.refresh().await }
                },
            )
            .await?;

        wechat_sdk::json_decode(&res)
    }

    /// 服务商自身第三方登录，对齐 PHP `OpenWork\Application::getOAuth(string $suiteId)`
    ///
    /// PHP 返回 socialite 的 OpenWeWork provider，携带 suite ticket 与 suite access token：
    /// 授权页使用 `appid={suiteId}`，换身份/取详情使用 `cgi-bin/service/getuserinfo3rd`
    /// 与 `getuserdetail3rd`。
    pub fn oauth<S: Into<String>>(&self, suite_id: S) -> SuiteOAuth {
        self.build_oauth(suite_id.into())
    }

    /// 授权企业（代开发应用）登录，对齐 PHP `OpenWork\Application::getCorpOAuth(string $corpId)`
    ///
    /// 与 [`Application::oauth`] 相同，只是授权页的 `appid` 换成授权企业的 `corp_id`。
    pub fn corp_oauth<S: Into<String>>(&self, corp_id: S) -> SuiteOAuth {
        self.build_oauth(corp_id.into())
    }

    fn build_oauth(&self, client_id: String) -> SuiteOAuth {
        SuiteOAuth::new(client_id, self.config.account.suite_id.clone())
            .with_redirect_url(self.config.oauth_redirect_url.clone())
            .with_scopes(self.config.oauth_scopes.clone())
            .with_ticket(self.suite_ticket().clone())
            .with_access_token(self.suite_access_token().clone())
    }

    /// 第三方登录授权地址，对齐 getOAuth
    pub fn oauth_url(&self, scope: Option<&str>, state: Option<&str>) -> String {
        let scope = scope
            .map(|v| v.to_owned())
            .unwrap_or_else(|| self.config.oauth_scopes.join(","));
        let state = state.unwrap_or("");

        format!(
            "https://open.weixin.qq.com/connect/oauth2/authorize?appid={}&redirect_uri={}&response_type=code&scope={}&state={}#wechat_redirect",
            self.config.account.suite_id,
            wechat_sdk::url_encoding::raw_url_encode(&self.config.oauth_redirect_url),
            scope,
            state
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> OpenWorkConfig {
        OpenWorkConfig::new(Account::new(
            "corp",
            "provider-secret",
            "suite",
            "suite-secret",
            "token",
            "kdjCGGJKSRjjhESfPO5lTSWtYS0v5pQX47skCkZczio",
        ))
    }

    #[test]
    fn oauth_carries_suite_credentials() {
        // 用独立的 suite_id，避免与其它用例共享全局缓存的 suite ticket
        let mut conf = OpenWorkConfig::new(Account::new(
            "corp-oauth",
            "provider-secret",
            "suite-oauth",
            "suite-secret",
            "token",
            "kdjCGGJKSRjjhESfPO5lTSWtYS0v5pQX47skCkZczio",
        ));
        conf.oauth_redirect_url = "https://open.example.com/cb".to_owned();
        conf.oauth_scopes = vec!["snsapi_base".to_owned()];
        let app = Application::new(conf);
        app.suite_ticket().set_ticket("SUITE-TICKET");

        let suite_oauth = app.oauth("suite-oauth");
        assert_eq!(suite_oauth.client_id(), "suite-oauth");
        assert_eq!(suite_oauth.component_app_id(), "suite-oauth");
        assert_eq!(suite_oauth.suite_ticket().unwrap(), "SUITE-TICKET");

        let auth_url = suite_oauth.authorize_url("st");
        assert!(auth_url.contains("appid=suite-oauth"));
        assert!(auth_url.contains("component_appid=suite-oauth"));
        assert!(auth_url.contains("redirect_uri=https%3A%2F%2Fopen.example.com%2Fcb"));

        // 授权企业：appid 换成 corp_id，component_appid 仍是 suite_id
        let corp_oauth = app.corp_oauth("ww-corp");
        let corp_url = corp_oauth.authorize_url("st");
        assert!(corp_url.contains("appid=ww-corp"));
        assert!(corp_url.contains("component_appid=suite-oauth"));
        assert!(corp_url.contains("scope=snsapi_base"));
    }

    #[test]
    fn keys_and_encryptors() {
        let app = Application::new(config());
        assert_eq!(
            app.provider_access_token().get_key(),
            "open_work.access_token.corp.provider-secret"
        );
        assert!(app.suite_encryptor().is_ok());
        assert!(app.encryptor().is_ok());
        assert!(app.server().is_ok());
    }

    #[test]
    fn authorizer_token_created() {
        let app = Application::new(config());
        let token = app.authorizer_access_token("auth-corp", "permanent");
        assert_eq!(
            token.get_key(),
            "open_work.authorizer.access_token.auth-corp.permanent"
        );
        assert_eq!(app.jsapi_ticket("auth-corp", "permanent").get_key(), "open_work.jsapi_ticket.auth-corp");
    }

    #[test]
    fn oauth_url_contains_suite_id() {
        let mut conf = config();
        conf.oauth_redirect_url = "https://example.com/cb".to_owned();
        let app = Application::new(conf);
        let url = app.oauth_url(None, Some("state1"));

        assert!(url.starts_with("https://open.weixin.qq.com/connect/oauth2/authorize?appid=suite"));
        assert!(url.contains("scope=snsapi_base"));
        assert!(url.ends_with("#wechat_redirect"));
    }

    #[test]
    fn authorization_app_id_reads_corp() {
        let value = json!({"auth_corp_info": {"corpid": "wx123"}});
        assert_eq!(authorization_app_id(&value), "wx123");
    }
}
