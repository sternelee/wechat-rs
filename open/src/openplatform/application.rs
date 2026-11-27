//! copyright © ecdata.cn 2021 - present
//! 开放平台应用，对齐 EasyWeChat\OpenPlatform\Application

use serde_json::{json, Value};
use wechat_sdk::{constant::API_DOMAIN, AccessToken, Client, Encryptor, Token, WechatResult as Result};

use super::authorization::Authorization;
use super::config::OpenPlatformConfig;
use super::server::Server;
use super::token::{
    authorizer_cache_key, get_cached_authorizer_token, set_cached_authorizer_token,
    AuthorizerAccessToken, ComponentAccessToken, VerifyTicket, DEFAULT_AUTHORIZER_TTL,
};

/// 授权页地址
pub const AUTHORIZE_URL: &str = "https://mp.weixin.qq.com/cgi-bin/componentloginpage";

/// 开放平台应用
#[derive(Debug, Clone)]
pub struct Application {
    config: OpenPlatformConfig,
    verify_ticket: VerifyTicket,
    client: Client,
}

impl Application {
    /// 创建应用
    pub fn new(config: OpenPlatformConfig) -> Application {
        let verify_ticket = VerifyTicket::new(config.account.app_id.clone());

        Application {
            config,
            verify_ticket,
            client: Client::default(),
        }
    }

    /// 从 json 创建应用
    pub fn from_value(conf: &Value) -> Result<Application> {
        Ok(Application::new(OpenPlatformConfig::from_value(conf)?))
    }

    /// 配置
    pub fn config(&self) -> &OpenPlatformConfig {
        &self.config
    }

    /// 账号
    pub fn account(&self) -> &super::config::Account {
        &self.config.account
    }

    /// HTTP 客户端
    pub fn http_client(&self) -> &Client {
        &self.client
    }

    /// verify_ticket 管理器
    pub fn verify_ticket(&self) -> &VerifyTicket {
        &self.verify_ticket
    }

    /// 写入 verify_ticket
    pub fn set_verify_ticket<S: Into<String>>(&self, ticket: S) {
        self.verify_ticket.set_ticket(ticket);
    }

    /// 消息加解密器
    pub fn encryptor(&self) -> Result<Encryptor> {
        let account = &self.config.account;

        Encryptor::new(
            &account.app_id,
            &account.token,
            &account.aes_key,
            Some(&account.app_id),
        )
    }

    /// 消息服务端，默认挂载 verify_ticket 刷新处理器
    pub fn server(&self) -> Result<Server> {
        let mut server = Server::new(&self.config.account.token).with_encryptor(self.encryptor()?);
        let verify_ticket = self.verify_ticket.clone();

        server = server.on_info_type("component_verify_ticket", move |message| {
            if let Some(ticket) = message.get("ComponentVerifyTicket").and_then(Value::as_str) {
                verify_ticket.set_ticket(ticket);
            }

            Some(Value::from("success"))
        });

        Ok(server)
    }

    /// component_access_token 管理器
    pub fn component_access_token(&self) -> Result<ComponentAccessToken> {
        let account = &self.config.account;

        Ok(ComponentAccessToken::new(
            &account.app_id,
            &account.secret,
            self.verify_ticket.get_ticket()?,
        ))
    }

    /// 获取 component_access_token
    pub async fn get_component_token(&self) -> Result<String> {
        self.component_access_token()?.get_token().await
    }

    /// 查询授权信息，POST cgi-bin/component/api_query_auth
    pub async fn authorization(&self, authorization_code: &str) -> Result<Authorization> {
        let url = format!("{}/cgi-bin/component/api_query_auth", API_DOMAIN);
        let query = self.component_access_token()?.to_query().await?;
        let body = json!(
            {
                "component_appid": self.config.account.app_id,
                "authorization_code": authorization_code
            }
        );

        let data = self.client.post_json_with_query(&url, &query, &body).await?;

        if data.get("authorization_info").is_none() {
            return Err(error! {
                code: -1,
                msg: format!("Failed to get authorization_info: {}", data),
            });
        }

        Ok(Authorization::new(data))
    }

    /// 刷新授权方 token，POST cgi-bin/component/api_authorizer_token
    pub async fn refresh_authorizer_token(
        &self,
        app_id: &str,
        refresh_token: &str,
    ) -> Result<AuthorizerAccessToken> {
        let url = format!("{}/cgi-bin/component/api_authorizer_token", API_DOMAIN);
        let query = self.component_access_token()?.to_query().await?;
        let body = json!(
            {
                "component_appid": self.config.account.app_id,
                "authorizer_appid": app_id,
                "authorizer_refresh_token": refresh_token
            }
        );

        let data = self.client.post_json_with_query(&url, &query, &body).await?;
        let token = data
            .get("authorizer_access_token")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                error! {
                    code: -1,
                    msg: format!("Failed to get authorizer_access_token: {}", data),
                }
            })?;
        let expires_in = data
            .get("expires_in")
            .and_then(Value::as_u64)
            .unwrap_or(7200);

        let key = authorizer_cache_key(app_id, refresh_token);
        set_cached_authorizer_token(&key, token, expires_in.saturating_sub(500));

        Ok(AuthorizerAccessToken::new(app_id, token))
    }

    /// 获取授权方 token，带缓存
    pub async fn get_authorizer_access_token(
        &self,
        app_id: &str,
        refresh_token: &str,
    ) -> Result<AuthorizerAccessToken> {
        let key = authorizer_cache_key(app_id, refresh_token);

        if let Some(token) = get_cached_authorizer_token(&key) {
            return Ok(AuthorizerAccessToken::new(app_id, token));
        }

        self.refresh_authorizer_token(app_id, refresh_token).await
    }

    /// 创建预授权码，POST cgi-bin/component/api_create_preauthcode
    pub async fn create_pre_authorization_code(&self) -> Result<String> {
        let url = format!("{}/cgi-bin/component/api_create_preauthcode", API_DOMAIN);
        let query = self.component_access_token()?.to_query().await?;
        let body = json!( { "component_appid": self.config.account.app_id } );

        let data = self.client.post_json_with_query(&url, &query, &body).await?;
        let code = data
            .get("pre_auth_code")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                error! {
                    code: -1,
                    msg: format!("Failed to get pre_auth_code: {}", data),
                }
            })?;

        Ok(code.to_owned())
    }

    /// 生成预授权页地址
    pub fn create_pre_authorization_url(
        &self,
        pre_auth_code: &str,
        redirect_url: Option<&str>,
        extra: &[(&str, &str)],
    ) -> String {
        let redirect = encode_query_value(redirect_url.unwrap_or(&self.config.redirect_url));

        let mut query: Vec<(String, String)> = vec![
            ("component_appid".to_owned(), self.config.account.app_id.clone()),
            ("pre_auth_code".to_owned(), pre_auth_code.to_owned()),
            ("redirect_uri".to_owned(), redirect),
            ("auth_type".to_owned(), "3".to_owned()),
        ];
        query.extend(
            extra
                .iter()
                .map(|(key, value)| ((*key).to_owned(), (*value).to_owned())),
        );

        format!("{}?{}", AUTHORIZE_URL, wechat_sdk::signature::query_string(&query))
    }

    /// 授权方 access_token 包装，便于直接发起调用
    pub fn authorizer_token(app_id: &str, token: &str) -> AuthorizerAccessToken {
        AuthorizerAccessToken::new(app_id, token)
    }

    /// 组件自身网页授权，对齐 OpenPlatform\Application::getOAuth()
    ///
    /// PHP: `new WeChat(['client_id' => component_app_id, 'client_secret' => secret,
    /// 'redirect_url' => oauth.redirect_url])->scopes(oauth.scopes ?: ['snsapi_userinfo'])`
    pub fn oauth(&self) -> wechat_mp::OAuth {
        let account = &self.config.account;

        wechat_mp::OAuth::new(
            account.app_id.clone(),
            account.secret.clone(),
            self.config.oauth_redirect_url.clone(),
        )
        .with_scopes(self.config.oauth_scopes.clone())
    }

    /// 组件授权上下文（component_appid + 惰性组件 token 提供者）
    pub fn component_oauth(&self) -> Result<wechat_sdk::ComponentOAuth> {
        let token = self.component_access_token()?;

        Ok(wechat_sdk::ComponentOAuth::new(
            self.config.account.app_id.clone(),
            move || {
                let token = token.clone();
                async move { token.get_token().await }
            },
        ))
    }

    /// 代公众号网页授权工厂，对齐 createAuthorizerOAuthFactory()
    pub fn authorizer_oauth(&self, app_id: &str) -> Result<wechat_mp::AuthorizerOAuth> {
        Ok(wechat_mp::AuthorizerOAuth::new(app_id.to_owned(), self.component_oauth()?)
            .with_redirect_url(self.config.oauth_redirect_url.clone())
            .with_scopes(self.config.oauth_scopes.clone()))
    }

    /// 代公众号应用的配置：授权方 appid + 组件 token/aes_key + require_encryption + oauth
    fn authorizer_mp_config(&self, app_id: &str) -> wechat_mp::MpConfig {
        let account = &self.config.account;

        wechat_mp::MpConfig::new(app_id, "")
            .with_message(account.token.clone(), account.aes_key.clone())
            .require_encryption(self.config.require_encryption)
            .oauth(
                self.config.oauth_redirect_url.clone(),
                self.config.oauth_scopes.clone(),
            )
    }

    /// 代小程序应用的配置：授权方 appid + 组件 token/aes_key + require_encryption
    fn authorizer_weapp_config(&self, app_id: &str) -> wechat_weapp::WeappConfig {
        let account = &self.config.account;

        wechat_weapp::WeappConfig::new(app_id, "")
            .with_message(account.token.clone(), account.aes_key.clone())
            .require_encryption(self.config.require_encryption)
    }

    /// 代公众号调用：注入授权方 token、组件加解密器与组件授权的公众号应用
    ///
    /// 对齐 PHP getOfficialAccount()：配置里 app_id 换成授权方 appid，
    /// token/aes_key 沿用第三方平台，并注入 encryptor 与 OAuth factory。
    pub async fn official_account(
        &self,
        app_id: &str,
        refresh_token: &str,
    ) -> Result<wechat_mp::Application> {
        let token = self.get_authorizer_access_token(app_id, refresh_token).await?;
        let access_token = seed_access_token("official_account", app_id, token.get_token());
        // PHP 无条件注入组件 encryptor（`setEncryptor($this->getEncryptor())`）；
        // aes_key 缺失时这里会返回 Err，与 PHP 的 "Invalid AESKey." 一致。
        Ok(wechat_mp::Application::new(self.authorizer_mp_config(app_id))
            .with_access_token(access_token)
            .with_oauth_component(self.component_oauth()?)
            .with_encryptor(self.encryptor()?))
    }

    /// 代小程序调用：注入授权方 token 的小程序应用
    pub async fn mini_app(
        &self,
        app_id: &str,
        refresh_token: &str,
    ) -> Result<wechat_weapp::Application> {
        let token = self.get_authorizer_access_token(app_id, refresh_token).await?;
        let access_token = seed_access_token("mini_app", app_id, token.get_token());

        // 同上：无条件注入组件 encryptor
        Ok(wechat_weapp::Application::new(self.authorizer_weapp_config(app_id))
            .with_access_token(access_token)
            .with_encryptor(self.encryptor()?))
    }
}

/// 查询参数值编码，等价 PHP 的 urlencode
fn encode_query_value(value: &str) -> String {
    use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};

    utf8_percent_encode(value, NON_ALPHANUMERIC).to_string()
}

/// 把已有 token 写入 sdk 缓存，供外部 Application 直接使用
fn seed_access_token(app_type: &str, app_id: &str, token: &str) -> AccessToken {
    let access_token = AccessToken::new(app_type, app_id, "");

    access_token.set(Token {
        access_token: token.to_owned(),
        create_time: wechat_sdk::current_timestamp(),
        expires: DEFAULT_AUTHORIZER_TTL as i64,
    });

    access_token
}

#[cfg(test)]
mod tests {
    use crate::openplatform::config::Account;
    use super::*;
    use serde_json::json;

    /// 生成合法的 43 位 EncodingAESKey（避免使用固定字面量）
    fn test_aes_key() -> String {
        let mut key: String = (0..42).map(|i| char::from(b'a' + (i % 26) as u8)).collect();
        key.push('A');
        key
    }

    fn application() -> Application {
        let conf = OpenPlatformConfig::from_value(&json!(
            {
                "app_id": "wxcomponent",
                "secret": "secret",
                "token": "token",
                "aes_key": test_aes_key(),
                "redirect_url": "https://example.com/cb"
            }
        ))
        .unwrap();

        Application::new(conf)
    }

    #[test]
    fn builds_components() {
        let app = application();
        assert_eq!(app.account().app_id, "wxcomponent");
        assert!(app.encryptor().is_ok());
        assert!(app.server().is_ok());
    }

    #[test]
    fn component_token_requires_verify_ticket() {
        // 使用独立 app_id，避免与其他用例共享的全局 ticket 缓存相互影响
        let conf = OpenPlatformConfig::from_value(&json!(
            {
                "app_id": "wxcomponent-isolated",
                "secret": "secret",
                "token": "token",
                "aes_key": test_aes_key()
            }
        ))
        .unwrap();
        let app = Application::new(conf);

        assert!(app.component_access_token().is_err());

        app.set_verify_ticket("ticket-value");
        let token = app.component_access_token().unwrap();
        assert_eq!(
            token.get_key(),
            "open_platform.component_access_token.wxcomponent-isolated"
        );
    }

    #[test]
    fn pre_authorization_url_contains_required_query() {
        let app = application();
        let url = app.create_pre_authorization_url("pre-auth", None, &[("biz_appid", "wx1")]);

        assert!(url.starts_with(AUTHORIZE_URL));
        assert!(url.contains("component_appid=wxcomponent"));
        assert!(url.contains("pre_auth_code=pre-auth"));
        assert!(url.contains("redirect_uri=https%3A%2F%2Fexample"));
        assert!(url.contains("biz_appid=wx1"));
    }

    #[test]
    fn server_registers_verify_ticket_handler() {
        let app = application();
        let server = app.server().unwrap();
        let message = json!(
            {
                "InfoType": "component_verify_ticket",
                "ComponentVerifyTicket": "new-ticket"
            }
        );

        assert_eq!(
            server.handle(&message).and_then(|v| v.as_str().map(str::to_owned)),
            Some("success".to_owned())
        );
        assert_eq!(app.verify_ticket().get_ticket().unwrap(), "new-ticket");
    }

    fn authorized_platform() -> Application {
        let config = OpenPlatformConfig::new(Account::new(
            "wx-component",
            "component-secret",
            "component-token",
            "aes",
        ))
        .with_oauth("https://example.com/oauth", vec!["snsapi_base".to_owned()])
        .require_encryption(true);

        let app = Application::new(config);
        app.set_verify_ticket("verify-ticket");
        app
    }

    #[test]
    fn oauth_uses_component_credentials() {
        let app = authorized_platform();
        let oauth = app.oauth();

        let url = oauth.authorize_url("", "state-1");
        assert!(url.contains("appid=wx-component"));
        assert!(url.contains("scope=snsapi_base"));
        assert!(!url.contains("component_appid"));
        assert_eq!(app.config().oauth_redirect_url(), "https://example.com/oauth");
    }

    #[test]
    fn authorizer_apps_inherit_component_message_settings() {
        let app = authorized_platform();

        let mp_config = app.authorizer_mp_config("wx-authorizer");
        assert_eq!(mp_config.account.app_id(), "wx-authorizer");
        assert_eq!(mp_config.account.token(), "component-token");
        assert_eq!(mp_config.account.aes_key(), "aes");
        assert!(mp_config.require_encryption);
        assert_eq!(mp_config.oauth_redirect_url.as_deref(), Some("https://example.com/oauth"));
        assert_eq!(mp_config.oauth_scopes, vec!["snsapi_base".to_owned()]);

        let weapp_config = app.authorizer_weapp_config("wx-mini");
        assert_eq!(weapp_config.app_id, "wx-mini");
        assert_eq!(weapp_config.token, "component-token");
        assert_eq!(weapp_config.aes_key, "aes");
        assert!(weapp_config.require_encryption);
    }

    #[test]
    fn component_encryptor_requires_valid_aes_key() {
        // PHP `getEncryptor()` 在 aes_key 非法时抛 "Invalid AESKey."；
        // 代子应用注入加密器时也走同一约束。
        let config = OpenPlatformConfig::new(Account::new(
            "wx-component",
            "component-secret",
            "component-token",
            "not base64!!",
        ));
        let app = Application::new(config);

        assert!(app.encryptor().is_err(), "非法 aes_key 应报 Invalid AESKey");
    }

    #[test]
    fn authorizer_oauth_targets_authorizer_with_component_app_id() {
        let app = authorized_platform();
        let oauth = app.authorizer_oauth("wx-authorizer").unwrap();

        let url = oauth.authorize_url("", "state-1");
        assert!(url.contains("appid=wx-authorizer"));
        assert!(url.contains("component_appid=wx-component"));
        assert!(url.contains("scope=snsapi_base"));
        assert_eq!(oauth.component_app_id(), "wx-component");
    }

    #[tokio::test]
    async fn seed_access_token_is_cached() {
        let access_token = seed_access_token("official_account", "wxauth", "token-value");
        assert_eq!(access_token.get_token().await.unwrap(), "token-value");
    }
}
