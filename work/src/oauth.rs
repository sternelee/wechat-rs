//! copyright © ecdata.cn 2021 - present
//! 企业微信网页授权，对齐 EasyWeChat\Work\Application::getOAuth()
//!
//! PHP 侧返回 overtrue/socialite 的 WeWork provider：
//!
//! ```php
//! $socialite = (new WeWork([
//!     'client_id' => $this->config->get('corp_id'),
//!     'client_secret' => $this->config->get('secret'),
//!     'redirect_url' => $this->config->get('oauth.redirect_url'),
//! ]))->withApiAccessToken($this->getAccessToken()->getToken())
//!     ->scopes((array) $this->config->get('oauth.scopes') ?: ['snsapi_base'])
//!     ->withAgentId((int) $this->config->get('agent_id'));
//! ```
//!
//! 本项目直接按企业微信文档实现等价的授权页、code 换身份与用户详情接口。

use crate::config::{Account, WorkConfig};
use crate::token::AccessToken;
use serde_json::Value;
use wechat_sdk::{constant::WORK_API_DOMAIN, Client, WechatResult as Result};

/// 微信内网页授权页
pub const OAUTH_AUTHORIZE_URL: &str = "https://open.weixin.qq.com/connect/oauth2/authorize";
/// 企业微信扫码登录页（非微信内场景）
pub const QR_CONNECT_URL: &str = "https://open.work.weixin.qq.com/wwopen/sso/qrConnect";

/// 网页授权，对齐 getOAuth()
#[derive(Debug, Clone)]
pub struct OAuth {
    /// 企业 corpid，对齐 client_id
    client_id: String,
    /// 应用 secret，对齐 client_secret
    client_secret: String,
    redirect_url: String,
    scopes: Vec<String>,
    agent_id: Option<i64>,
    /// 显式指定的 api access_token，对齐 withApiAccessToken()
    api_access_token: Option<String>,
    /// 应用 token 管理器，用于自动获取/刷新 api access_token
    access_token: Option<AccessToken>,
    /// 接口域名（默认 `https://qyapi.weixin.qq.com`，私有化部署可覆盖）
    base_domain: String,
    client: Client,
}

impl OAuth {
    /// 创建授权组件
    pub fn new<C, S>(corp_id: C, secret: S) -> OAuth
    where
        C: Into<String>,
        S: Into<String>,
    {
        OAuth {
            client_id: corp_id.into(),
            client_secret: secret.into(),
            redirect_url: String::new(),
            scopes: vec!["snsapi_base".to_owned()],
            agent_id: None,
            api_access_token: None,
            access_token: None,
            base_domain: WORK_API_DOMAIN.to_owned(),
            client: Client::default(),
        }
    }

    /// 从应用配置创建，对齐 `$this->config->get('oauth.*')`
    pub fn from_config(config: &WorkConfig) -> OAuth {
        OAuth::new(
            config.account.corp_id.as_str(),
            config.account.secret.as_str(),
        )
        .with_redirect_url(config.oauth_redirect_url.as_str())
        .with_scopes(config.oauth_scopes.clone())
        .with_agent_id(config.account.agent_id)
    }

    /// 从账号信息创建
    pub fn from_account(account: &Account) -> OAuth {
        OAuth::new(account.corp_id.as_str(), account.secret.as_str())
            .with_agent_id(account.agent_id)
    }

    /// 设置回调地址
    pub fn with_redirect_url<S: Into<String>>(mut self, url: S) -> Self {
        self.redirect_url = url.into();
        self
    }

    /// 设置 scope，对齐 scopes()
    pub fn with_scopes<S: Into<String>>(mut self, scopes: Vec<S>) -> Self {
        self.scopes = scopes.into_iter().map(|s| s.into()).collect();
        self
    }

    /// 设置 agent_id，对齐 withAgentId()
    pub fn with_agent_id(mut self, agent_id: Option<i64>) -> Self {
        self.agent_id = agent_id;
        self
    }

    /// 显式指定 api access_token，对齐 withApiAccessToken()
    pub fn with_api_access_token<S: Into<String>>(mut self, token: S) -> Self {
        self.api_access_token = Some(token.into());
        self
    }

    /// 使用应用 token 管理器自动获取 api access_token
    pub fn with_access_token(mut self, token: AccessToken) -> Self {
        self.access_token = Some(token);
        self
    }

    /// 覆盖接口域名（私有化部署代理）
    pub fn with_base_domain<S: Into<String>>(mut self, domain: S) -> Self {
        self.base_domain = domain.into();
        self
    }

    /// corp_id，对齐 client_id
    pub fn client_id(&self) -> &str {
        &self.client_id
    }

    /// secret，对齐 client_secret
    pub fn client_secret(&self) -> &str {
        &self.client_secret
    }

    /// 回调地址
    pub fn redirect_url(&self) -> &str {
        &self.redirect_url
    }

    /// scope 列表
    pub fn scopes(&self) -> &[String] {
        &self.scopes
    }

    /// agent_id
    pub fn agent_id(&self) -> Option<i64> {
        self.agent_id
    }

    /// 微信内授权页地址，对齐 getAuthUrl()
    pub fn authorize_url(&self, state: &str) -> Result<String> {
        if self.redirect_url.is_empty() {
            return Err(error! {code: -1, msg: "Missing config `oauth.redirect_url`."});
        }

        Ok(self.authorize_url_with(self.redirect_url.as_str(), state))
    }

    /// 指定回调地址生成授权页
    pub fn authorize_url_with(&self, redirect_uri: &str, state: &str) -> String {
        let scope = if self.scopes.is_empty() {
            "snsapi_base".to_owned()
        } else {
            self.scopes.join(",")
        };

        let mut url = format!(
            "{}?appid={}&redirect_uri={}&response_type=code&scope={}&state={}",
            OAUTH_AUTHORIZE_URL,
            self.client_id,
            encode(redirect_uri),
            scope,
            encode(state),
        );

        if let Some(agent_id) = self.agent_id {
            url.push_str(&format!("&agentid={}", agent_id));
        }

        url.push_str("#wechat_redirect");
        url
    }

    /// 扫码登录地址，对齐 WeWork provider 的 qrConnect 场景
    pub fn qr_connect_url(&self, state: &str) -> Result<String> {
        if self.redirect_url.is_empty() {
            return Err(error! {code: -1, msg: "Missing config `oauth.redirect_url`."});
        }

        let mut url = format!(
            "{}?appid={}&redirect_uri={}&state={}&usertype=member",
            QR_CONNECT_URL,
            self.client_id,
            encode(self.redirect_url.as_str()),
            encode(state),
        );

        if let Some(agent_id) = self.agent_id {
            url.push_str(&format!("&agentid={}", agent_id));
        }

        Ok(url)
    }

    /// api access_token：优先使用显式指定的，否则向应用 token 管理器取
    pub async fn get_api_access_token(&self) -> Result<String> {
        if let Some(token) = &self.api_access_token {
            return Ok(token.clone());
        }

        match &self.access_token {
            Some(token) => token.get_token().await,
            None => Err(error! {
                code: -1,
                msg: "Missing api access_token: call `with_api_access_token()` or `with_access_token()`."
            }),
        }
    }

    /// code 换身份，对齐 getUserByCode() → `/cgi-bin/auth/getuserinfo`
    ///
    /// 返回 `{"userid": ...}`（企业成员）或 `{"openid": ...}`（外部联系人），
    /// 敏感信息场景还会带 `user_ticket`。
    pub async fn user_id(&self, code: &str) -> Result<Value> {
        let token = self.get_api_access_token().await?;
        let url = Client::build_url(
            self.base_domain.as_str(),
            "/cgi-bin/auth/getuserinfo",
            &[("access_token", token.as_str()), ("code", code)],
        );

        self.client.get_json(&url).await
    }

    /// 用 `user_ticket` 获取敏感信息，对齐 `/cgi-bin/user/getuserdetail`
    pub async fn user_detail(&self, user_ticket: &str) -> Result<Value> {
        let token = self.get_api_access_token().await?;
        let url = Client::build_url(
            self.base_domain.as_str(),
            "/cgi-bin/user/getuserdetail",
            &[("access_token", token.as_str())],
        );

        self.client
            .post_json(&url, &serde_json::json!({ "user_ticket": user_ticket }))
            .await
    }

    /// 读取成员详情，对齐 `/cgi-bin/user/get`
    pub async fn user(&self, user_id: &str) -> Result<Value> {
        let token = self.get_api_access_token().await?;
        let url = Client::build_url(
            self.base_domain.as_str(),
            "/cgi-bin/user/get",
            &[("access_token", token.as_str()), ("userid", user_id)],
        );

        self.client.get_json(&url).await
    }

    /// code 换完整用户信息：先用 code 换到 userid，再取成员详情
    ///
    /// 对齐 socialite `user()`：`getUserByCode()` 拿到 userid 后调用 `getUser()`。
    pub async fn user_from_code(&self, code: &str) -> Result<Value> {
        let identity = self.user_id(code).await?;

        let user_id = match identity["userid"].as_str() {
            Some(user_id) if !user_id.is_empty() => user_id.to_owned(),
            _ => return Ok(identity),
        };

        let mut user = self.user(&user_id).await?;
        if let Some(object) = user.as_object_mut() {
            object.insert("userid".to_owned(), Value::String(user_id));
        }

        Ok(user)
    }
}

/// 对 query 值做 percent-encode，对齐 PHP `rawurlencode()` / `http_build_query()`
fn encode(value: &str) -> String {
    wechat_sdk::url_encoding::raw_url_encode(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn oauth() -> OAuth {
        OAuth::new("corp-1", "secret-1")
            .with_redirect_url("https://example.com/cb")
            .with_agent_id(Some(100001))
    }

    #[test]
    fn authorize_url_carries_corp_and_agent() {
        let url = oauth().authorize_url("STATE").unwrap();

        assert!(url.starts_with(OAUTH_AUTHORIZE_URL));
        assert!(url.contains("appid=corp-1"));
        assert!(url.contains("response_type=code"));
        assert!(url.contains("scope=snsapi_base"));
        assert!(url.contains("state=STATE"));
        assert!(url.contains("agentid=100001"));
        assert!(url.ends_with("#wechat_redirect"));
        // 回调地址被 percent-encode
        assert!(url.contains("redirect_uri=https%3A%2F%2Fexample.com%2Fcb"));
    }

    #[test]
    fn authorize_url_requires_redirect_url() {
        let err = OAuth::new("corp", "secret").authorize_url("STATE").unwrap_err();
        assert!(format!("{}", err).contains("oauth.redirect_url"));
    }

    #[test]
    fn qr_connect_url_targets_wwopen() {
        let url = oauth().qr_connect_url("STATE").unwrap();
        assert!(url.starts_with(QR_CONNECT_URL));
        assert!(url.contains("usertype=member"));
        assert!(url.contains("agentid=100001"));
    }

    #[test]
    fn from_config_reads_oauth_settings() {
        let conf = WorkConfig::from_value(&serde_json::json!({
            "corp_id": "corp-1",
            "secret": "secret-1",
            "agent_id": 100001,
            "oauth": {"redirect_url": "https://example.com/cb", "scopes": ["snsapi_privateinfo"]}
        }))
        .unwrap();

        let oauth = OAuth::from_config(&conf);
        assert_eq!(oauth.client_id(), "corp-1");
        assert_eq!(oauth.client_secret(), "secret-1");
        assert_eq!(oauth.redirect_url(), "https://example.com/cb");
        assert_eq!(oauth.scopes(), ["snsapi_privateinfo"]);
        assert_eq!(oauth.agent_id(), Some(100001));
        assert!(oauth
            .authorize_url("STATE")
            .unwrap()
            .contains("scope=snsapi_privateinfo"));
    }

    #[tokio::test]
    async fn api_token_is_required_for_user_calls() {
        let err = OAuth::new("corp", "secret").user_id("CODE").await.unwrap_err();
        assert!(format!("{}", err).contains("api access_token"));
    }

    #[tokio::test]
    async fn user_id_reads_identity_from_wecom() {
        let (port, captured) = serve_once("{\"userid\":\"zhangsan\"}");

        let oauth = OAuth::new("corp", "secret")
            .with_api_access_token("TOKEN")
            .with_base_domain(format!("http://127.0.0.1:{}", port));

        let identity = oauth.user_id("CODE-1").await.unwrap();
        assert_eq!(identity["userid"], "zhangsan");

        let request = captured.lock().unwrap()[0].clone();
        assert!(request.contains("/cgi-bin/auth/getuserinfo"));
        assert!(request.contains("access_token=TOKEN"));
        assert!(request.contains("code=CODE-1"));
    }

    #[tokio::test]
    async fn user_from_code_falls_back_to_openid_identity() {
        let (port, _) = serve_once("{\"openid\":\"oExternal\"}");

        let oauth = OAuth::new("corp", "secret")
            .with_api_access_token("TOKEN")
            .with_base_domain(format!("http://127.0.0.1:{}", port));

        let user = oauth.user_from_code("CODE-2").await.unwrap();
        assert_eq!(user["openid"], "oExternal");
    }

    fn serve_once(response: &'static str) -> (u16, std::sync::Arc<std::sync::Mutex<Vec<String>>>) {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::sync::{Arc, Mutex};

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let captured: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = captured.clone();

        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let mut stream = match stream {
                    Ok(stream) => stream,
                    Err(_) => continue,
                };

                let mut buf = [0u8; 4096];
                let read = stream.read(&mut buf).unwrap_or(0);
                sink.lock()
                    .unwrap()
                    .push(String::from_utf8_lossy(&buf[..read]).to_string());

                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    response.len(),
                    response
                );
            }
        });

        (port, captured)
    }
}
