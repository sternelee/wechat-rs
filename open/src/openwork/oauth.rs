//! copyright © ecdata.cn 2021 - present
//! 服务商（第三方应用）网页授权，对齐 EasyWeChat\OpenWork\Application::{getOAuth, getCorpOAuth}
//!
//! PHP 侧返回 socialite 的 OpenWeWork provider，构造参数为 suite ticket + suite access token：
//!
//! ```php
//! (new OpenWeWork(['client_id' => $suiteId /* 或 $corpId */, 'redirect_url' => oauth.redirect_url]))
//!     ->withSuiteTicket($this->getSuiteTicket()->getTicket())
//!     ->withSuiteAccessToken($suiteAccessToken->getToken())
//!     ->scopes($this->config->get('oauth.scopes', ['snsapi_base']));
//! ```
//!
//! 授权页带 `component_appid`（应用/服务商的 suite_id），code 换身份走服务商接口
//! `cgi-bin/service/getuserinfo3rd`（需 suite_access_token），用户详情走
//! `cgi-bin/service/getuserdetail3rd`（需 user_ticket）。

use serde_json::Value;
use wechat_sdk::{constant::WORK_API_DOMAIN, url_encoding::raw_url_encode, Client, WechatResult as Result};

use crate::openwork::token::{SuiteAccessToken, SuiteTicket};

/// 第三方应用授权页前缀
pub const SUITE_AUTHORIZE_URL: &str = "https://open.weixin.qq.com/connect/oauth2/authorize";

/// 服务商扫码登录前缀
pub const SUITE_QR_CONNECT_URL: &str = "https://open.work.weixin.qq.com/wwopen/sso/3rd_qrConnect";

/// 授权对象：`client_id` 为 suite_id（服务商自身）或 corp_id（授权企业），对齐 PHP getOAuth/getCorpOAuth
#[derive(Debug, Clone)]
pub struct SuiteOAuth {
    client_id: String,
    component_app_id: String,
    redirect_url: String,
    scopes: Vec<String>,
    suite_ticket: Option<String>,
    suite_access_token: Option<String>,
    access_token: Option<SuiteAccessToken>,
    ticket: Option<SuiteTicket>,
    base_domain: String,
    client: Client,
}

impl SuiteOAuth {
    /// 创建授权对象
    pub fn new<C, S>(client_id: C, component_app_id: S) -> SuiteOAuth
    where
        C: Into<String>,
        S: Into<String>,
    {
        SuiteOAuth {
            client_id: client_id.into(),
            component_app_id: component_app_id.into(),
            redirect_url: String::new(),
            scopes: vec!["snsapi_base".to_owned()],
            suite_ticket: None,
            suite_access_token: None,
            access_token: None,
            ticket: None,
            base_domain: WORK_API_DOMAIN.to_owned(),
            client: Client::default(),
        }
    }

    /// 设置回调地址，对齐 `redirect_url`
    pub fn with_redirect_url<S: Into<String>>(mut self, url: S) -> Self {
        self.redirect_url = url.into();
        self
    }

    /// 设置作用域（默认 snsapi_base）
    pub fn with_scopes(mut self, scopes: Vec<String>) -> Self {
        if !scopes.is_empty() {
            self.scopes = scopes;
        }
        self
    }

    /// 显式指定 suite ticket，对齐 `withSuiteTicket()`
    pub fn with_suite_ticket<S: Into<String>>(mut self, ticket: S) -> Self {
        self.suite_ticket = Some(ticket.into());
        self
    }

    /// 显式指定 suite access token，对齐 `withSuiteAccessToken()`
    pub fn with_suite_access_token<S: Into<String>>(mut self, token: S) -> Self {
        self.suite_access_token = Some(token.into());
        self
    }

    /// 使用 token 管理器按需刷新
    pub fn with_access_token(mut self, access_token: SuiteAccessToken) -> Self {
        self.access_token = Some(access_token);
        self
    }

    /// 使用 suite ticket 管理器
    pub fn with_ticket(mut self, ticket: SuiteTicket) -> Self {
        self.ticket = Some(ticket);
        self
    }

    /// 覆盖 API 域名（测试或代理场景）
    pub fn with_base_domain<S: Into<String>>(mut self, domain: S) -> Self {
        self.base_domain = domain.into();
        self
    }

    /// client_id（suite_id 或 corp_id）
    pub fn client_id(&self) -> &str {
        &self.client_id
    }

    /// 组件（服务商）suite_id
    pub fn component_app_id(&self) -> &str {
        &self.component_app_id
    }

    /// suite ticket（显式值优先，否则由 SuiteTicket 取）
    pub fn suite_ticket(&self) -> Result<String> {
        if let Some(ticket) = &self.suite_ticket {
            return Ok(ticket.clone());
        }

        match &self.ticket {
            Some(ticket) => ticket.get_ticket(),
            None => Err(error! { code: -1, msg: "No suite_ticket available for SuiteOAuth." }),
        }
    }

    /// 授权跳转地址（带 component_appid）
    pub fn authorize_url<S: AsRef<str>>(&self, state: S) -> String {
        format!(
            "{}?appid={}&redirect_uri={}&response_type=code&scope={}&state={}&component_appid={}#wechat_redirect",
            SUITE_AUTHORIZE_URL,
            self.client_id,
            raw_url_encode(&self.redirect_url),
            self.scopes.join(","),
            state.as_ref(),
            self.component_app_id,
        )
    }

    /// 服务商扫码登录地址，对齐 `qr_connect_url`
    pub fn qr_connect_url<S: AsRef<str>>(&self, state: S) -> String {
        format!(
            "{}?appid={}&redirect_uri={}&state={}&usertype=member&lang=zh_CN",
            SUITE_QR_CONNECT_URL,
            self.component_app_id,
            raw_url_encode(&self.redirect_url),
            state.as_ref()
        )
    }

    /// 取 suite access token（显式值优先，否则由管理器刷新）
    pub async fn suite_access_token(&self) -> Result<String> {
        if let Some(token) = &self.suite_access_token {
            return Ok(token.clone());
        }

        match &self.access_token {
            Some(token) => token.get_token().await,
            None => Err(error! { code: -1, msg: "No suite_access_token available for SuiteOAuth." }),
        }
    }

    /// code 换用户身份，对齐 `cgi-bin/service/getuserinfo3rd`
    pub async fn user_info(&self, code: &str) -> Result<Value> {
        let token = self.suite_access_token().await?;
        let url = Client::build_url(
            &self.base_domain,
            "/cgi-bin/service/getuserinfo3rd",
            &[("suite_access_token", token.as_str()), ("code", code)],
        );

        self.client.get_json(&url).await
    }

    /// 拉取用户详情，对齐 `cgi-bin/service/getuserdetail3rd`
    pub async fn user_detail(&self, user_ticket: &str) -> Result<Value> {
        let token = self.suite_access_token().await?;
        let url = Client::build_url(
            &self.base_domain,
            "/cgi-bin/service/getuserdetail3rd",
            &[("suite_access_token", token.as_str())],
        );

        self.client
            .post_json(&url, &serde_json::json!({ "user_ticket": user_ticket }))
            .await
    }

    /// code → 身份 → 详情，对齐 socialite `user()`/`userFromCode()`
    pub async fn user_from_code(&self, code: &str) -> Result<Value> {
        let info = self.user_info(code).await?;
        let user_ticket = info["user_ticket"].as_str().unwrap_or_default();

        if user_ticket.is_empty() {
            return Ok(info);
        }

        let detail = self.user_detail(user_ticket).await?;

        Ok(match (info.as_object(), detail.as_object()) {
            (Some(info), Some(detail)) => {
                let mut merged = info.clone();
                for (key, value) in detail {
                    merged.insert(key.clone(), value.clone());
                }
                Value::Object(merged)
            }
            _ => info,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn authorize_url_uses_client_id_and_component_app_id() {
        let oauth = SuiteOAuth::new("ww-corp", "ww-suite").with_redirect_url("https://a.b/cb");

        let url = oauth.authorize_url("st");
        assert!(url.starts_with(SUITE_AUTHORIZE_URL));
        assert!(url.contains("appid=ww-corp"));
        assert!(url.contains("component_appid=ww-suite"));
        assert!(url.contains("scope=snsapi_base"));
        assert!(url.contains("redirect_uri=https%3A%2F%2Fa.b%2Fcb"));

        let qr = oauth.qr_connect_url("st");
        assert!(qr.starts_with(SUITE_QR_CONNECT_URL));
        assert!(qr.contains("appid=ww-suite"));
        assert!(qr.contains("usertype=member"));
    }

    #[test]
    fn default_scopes_can_be_overridden() {
        let oauth = SuiteOAuth::new("ww-corp", "ww-suite").with_scopes(vec!["snsapi_privateinfo".into()]);
        assert!(oauth.authorize_url("st").contains("scope=snsapi_privateinfo"));
        assert_eq!(oauth.client_id(), "ww-corp");
        assert_eq!(oauth.component_app_id(), "ww-suite");
    }

    #[test]
    fn suite_ticket_requires_source() {
        let oauth = SuiteOAuth::new("ww-corp", "ww-suite");
        assert!(oauth.suite_ticket().is_err());

        let with_ticket = oauth.with_suite_ticket("TICKET");
        assert_eq!(with_ticket.suite_ticket().unwrap(), "TICKET");
    }

    #[tokio::test]
    async fn user_info_and_detail_hit_suite_endpoints() {
        let (port, captured) = serve(&[
            json!({"errcode": 0, "userid": "zhangsan", "user_ticket": "UT-1"}).to_string(),
            json!({"errcode": 0, "userid": "zhangsan", "mobile": "13800000000"}).to_string(),
        ]);

        let oauth = SuiteOAuth::new("ww-corp", "ww-suite")
            .with_suite_access_token("SUITE-TOKEN")
            .with_base_domain(format!("http://127.0.0.1:{}", port));

        let user = oauth.user_from_code("CODE-1").await.unwrap();
        assert_eq!(user["userid"], "zhangsan");
        assert_eq!(user["mobile"], "13800000000");

        let requests = captured.lock().unwrap().clone();
        assert!(requests[0].contains("/cgi-bin/service/getuserinfo3rd"));
        assert!(requests[0].contains("suite_access_token=SUITE-TOKEN"));
        assert!(requests[1].contains("/cgi-bin/service/getuserdetail3rd"));
        assert!(requests[1].contains("UT-1"));
    }

    fn serve(responses: &[String]) -> (u16, std::sync::Arc<std::sync::Mutex<Vec<String>>>) {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::sync::{Arc, Mutex};

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let captured: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = captured.clone();
        let responses: Vec<String> = responses.to_vec();

        std::thread::spawn(move || {
            for (index, stream) in listener.incoming().enumerate() {
                let mut stream = match stream {
                    Ok(stream) => stream,
                    Err(_) => continue,
                };

                let mut buf = [0u8; 8192];
                let read = stream.read(&mut buf).unwrap_or(0);
                sink.lock()
                    .unwrap()
                    .push(String::from_utf8_lossy(&buf[..read]).to_string());

                let body = responses
                    .get(index)
                    .cloned()
                    .unwrap_or_else(|| "{\"errcode\":0}".to_owned());

                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
            }
        });

        (port, captured)
    }
}
