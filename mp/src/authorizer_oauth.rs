//! copyright © ecdata.cn 2021 - present
//! 第三方平台代公众号的网页授权，对齐 EasyWeChat\OpenPlatform\Application::createAuthorizerOAuthFactory()
//!
//! PHP 侧：
//!
//! ```php
//! new WeChat([
//!     'client_id' => $authorizerAppId,
//!     'component' => [
//!         'component_app_id' => $this->getComponentAppId(),
//!         'component_access_token' => fn () => $this->getComponentAccessToken()->getToken(),
//!     ],
//!     'redirect_url' => $this->getConfig()['oauth']['redirect_url'] ?? null,
//! ])->scopes($this->getConfig()['oauth']['scopes'] ?? ['snsapi_userinfo'])
//! ```
//!
//! 因此授权页要带 `component_appid`，code 换 token 走 `/sns/oauth2/component/access_token`。

use serde_json::Value;
use wechat_sdk::{Client, ComponentOAuth, WechatResult as Result};

use crate::oauth::{encode_uri, OAUTH_AUTHORIZE_URL};

/// 代公众号网页授权
#[derive(Debug, Clone)]
pub struct AuthorizerOAuth {
    /// 授权方公众号 app_id，对齐 client_id
    authorizer_app_id: String,
    /// 组件（第三方平台）上下文
    component: ComponentOAuth,
    redirect_url: String,
    scopes: Vec<String>,
    base_domain: String,
    client: Client,
}

impl AuthorizerOAuth {
    /// 创建代公众号授权组件
    pub fn new<S: Into<String>>(authorizer_app_id: S, component: ComponentOAuth) -> AuthorizerOAuth {
        AuthorizerOAuth {
            authorizer_app_id: authorizer_app_id.into(),
            component,
            redirect_url: String::new(),
            scopes: vec!["snsapi_userinfo".to_owned()],
            base_domain: wechat_sdk::constant::API_DOMAIN.to_owned(),
            client: Client::default(),
        }
    }

    /// 设置回调地址
    pub fn with_redirect_url<S: Into<String>>(mut self, url: S) -> Self {
        self.redirect_url = url.into();
        self
    }

    /// 设置授权作用域
    pub fn with_scopes(mut self, scopes: Vec<String>) -> Self {
        if !scopes.is_empty() {
            self.scopes = scopes;
        }
        self
    }

    /// 覆盖 API 域名（测试或代理场景）
    pub fn with_base_domain<S: Into<String>>(mut self, domain: S) -> Self {
        self.base_domain = domain.into();
        self
    }

    /// 授权方 app_id
    pub fn authorizer_app_id(&self) -> &str {
        &self.authorizer_app_id
    }

    /// 组件 app_id
    pub fn component_app_id(&self) -> &str {
        self.component.component_app_id()
    }

    /// 作用域
    pub fn scopes(&self) -> &[String] {
        &self.scopes
    }

    /// 生成授权跳转地址（带 component_appid）
    pub fn authorize_url<S: AsRef<str>>(&self, redirect_uri: S, state: S) -> String {
        let redirect = if redirect_uri.as_ref().is_empty() {
            self.redirect_url.as_str()
        } else {
            redirect_uri.as_ref()
        };

        format!(
            "{}?appid={}&redirect_uri={}&response_type=code&scope={}&state={}&component_appid={}#wechat_redirect",
            OAUTH_AUTHORIZE_URL,
            self.authorizer_app_id,
            encode_uri(redirect),
            self.scopes.join(","),
            state.as_ref(),
            self.component.component_app_id(),
        )
    }

    /// 通过 code 换取网页授权 access_token（组件代换）
    pub async fn access_token(&self, code: &str) -> Result<Value> {
        let component_token = self.component.get_token().await?;

        let url = Client::build_url(
            &self.base_domain,
            "/sns/oauth2/component/access_token",
            &[
                ("appid", self.authorizer_app_id.as_str()),
                ("code", code),
                ("grant_type", "authorization_code"),
                (
                    "component_appid",
                    self.component.component_app_id(),
                ),
                ("component_access_token", component_token.as_str()),
            ],
        );

        self.client.get_json(&url).await
    }

    /// 刷新网页授权 access_token（组件代换）
    pub async fn refresh_token(&self, refresh_token: &str) -> Result<Value> {
        let component_token = self.component.get_token().await?;

        let url = Client::build_url(
            &self.base_domain,
            "/sns/oauth2/component/refresh_token",
            &[
                ("appid", self.authorizer_app_id.as_str()),
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh_token),
                (
                    "component_appid",
                    self.component.component_app_id(),
                ),
                ("component_access_token", component_token.as_str()),
            ],
        );

        self.client.get_json(&url).await
    }

    /// 拉取用户信息（scope 需为 snsapi_userinfo）
    pub async fn user(&self, open_id: &str, access_token: &str, lang: Option<&str>) -> Result<Value> {
        let url = Client::build_url(
            &self.base_domain,
            "/sns/userinfo",
            &[
                ("access_token", access_token),
                ("openid", open_id),
                ("lang", lang.unwrap_or("zh_CN")),
            ],
        );

        self.client.get_json(&url).await
    }

    /// code → 网页授权 token → 用户信息，对齐 socialite `user()`
    pub async fn user_from_code(&self, code: &str, lang: Option<&str>) -> Result<Value> {
        let token = self.access_token(code).await?;
        let open_id = token["openid"].as_str().unwrap_or_default();
        let access_token = token["access_token"].as_str().unwrap_or_default();

        if open_id.is_empty() || access_token.is_empty() {
            return Ok(token);
        }

        self.user(open_id, access_token, lang).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    fn component() -> ComponentOAuth {
        ComponentOAuth::new("wx-component", || async { Ok("COMPONENT-TOKEN".to_owned()) })
    }

    #[test]
    fn authorize_url_carries_component_app_id() {
        let oauth = AuthorizerOAuth::new("wx-authorizer", component())
            .with_redirect_url("https://example.com/cb");

        let url = oauth.authorize_url("", "xyz");

        assert!(url.starts_with(OAUTH_AUTHORIZE_URL));
        assert!(url.contains("appid=wx-authorizer"));
        assert!(url.contains("component_appid=wx-component"));
        assert!(url.contains("scope=snsapi_userinfo"));
        assert_eq!(oauth.component_app_id(), "wx-component");
        assert_eq!(oauth.authorizer_app_id(), "wx-authorizer");
    }

    #[tokio::test]
    async fn access_token_uses_component_exchange() {
        let (port, captured) = serve_once("{\"openid\":\"o1\",\"access_token\":\"AT\"}");

        let oauth = AuthorizerOAuth::new("wx-authorizer", component())
            .with_base_domain(format!("http://127.0.0.1:{}", port));

        let token = oauth.access_token("CODE-1").await.unwrap();
        assert_eq!(token["access_token"], "AT");

        let request = captured.lock().unwrap()[0].clone();
        assert!(request.contains("/sns/oauth2/component/access_token"));
        assert!(request.contains("appid=wx-authorizer"));
        assert!(request.contains("component_appid=wx-component"));
        assert!(request.contains("component_access_token=COMPONENT-TOKEN"));
    }

    #[tokio::test]
    async fn user_from_code_short_circuits_without_identity() {
        let (port, _) = serve_once("{\"errcode\":0}");

        let oauth = AuthorizerOAuth::new("wx-authorizer", component())
            .with_base_domain(format!("http://127.0.0.1:{}", port));

        let user = oauth.user_from_code("CODE-2", None).await.unwrap();
        assert_eq!(user["errcode"], 0);
    }

    fn serve_once(response: &'static str) -> (u16, Arc<Mutex<Vec<String>>>) {
        use std::io::{Read, Write};
        use std::net::TcpListener;

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
