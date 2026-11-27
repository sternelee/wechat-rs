//! copyright © ecdata.cn 2021 - present
//! 公众号网页授权（OAuth 2.0），对齐 EasyWeChat 的 Socialite WeChat Provider
//! DOC https://developers.weixin.qq.com/doc/offiaccount/OA_Web_Apps/Wechat_webpage_authorization.html
//! created by shaipe 20210302

use serde_json::Value;
use wechat_sdk::{constant::API_DOMAIN, Client, ComponentOAuth, WechatResult as Result};

/// 授权地址前缀，公众号网页授权固定走 open.weixin.qq.com
pub const OAUTH_AUTHORIZE_URL: &str = "https://open.weixin.qq.com/connect/oauth2/authorize";

/// 公众号网页授权
#[derive(Debug, Clone)]
pub struct OAuth {
    app_id: String,
    secret: String,
    redirect_url: String,
    scopes: Vec<String>,
    state: String,
    /// 第三方平台代公众号授权上下文（未设置时为普通公众号授权）
    component: Option<ComponentOAuth>,
}

impl OAuth {
    /// 创建授权对象
    pub fn new<S: Into<String>>(app_id: S, secret: S, redirect_url: S) -> OAuth {
        OAuth {
            app_id: app_id.into(),
            secret: secret.into(),
            redirect_url: redirect_url.into(),
            scopes: vec!["snsapi_userinfo".to_owned()],
            state: String::new(),
            component: None,
        }
    }

    /// 第三方平台代公众号授权，对齐 EasyWeChat `component` 选项
    pub fn with_component(mut self, component: ComponentOAuth) -> OAuth {
        self.component = Some(component);
        self
    }

    /// 按条件注入组件上下文（None 时保持普通公众号授权）
    pub fn pipe_component(mut self, component: Option<ComponentOAuth>) -> OAuth {
        if let Some(component) = component {
            self.component = Some(component);
        }
        self
    }

    /// 组件上下文
    pub fn component(&self) -> Option<&ComponentOAuth> {
        self.component.as_ref()
    }

    /// 设置授权作用域（snsapi_base / snsapi_userinfo）
    pub fn with_scopes(mut self, scopes: Vec<String>) -> OAuth {
        if !scopes.is_empty() {
            self.scopes = scopes;
        }
        self
    }

    /// 设置 state
    pub fn with_state<S: Into<String>>(mut self, state: S) -> OAuth {
        self.state = state.into();
        self
    }

    /// 作用域
    pub fn scopes(&self) -> &Vec<String> {
        &self.scopes
    }

    /// 生成授权跳转地址
    pub fn authorize_url<S: AsRef<str>>(&self, redirect_url: S, state: S) -> String {
        let redirect = if redirect_url.as_ref().is_empty() {
            self.redirect_url.as_str()
        } else {
            redirect_url.as_ref()
        };
        let state = if state.as_ref().is_empty() {
            self.state.as_str()
        } else {
            state.as_ref()
        };

        match &self.component {
            Some(component) => format!(
                "{}?appid={}&redirect_uri={}&response_type=code&scope={}&state={}&component_appid={}#wechat_redirect",
                OAUTH_AUTHORIZE_URL,
                self.app_id,
                encode_uri(redirect),
                self.scopes.join(","),
                state,
                component.component_app_id(),
            ),
            None => format!(
                "{}?appid={}&redirect_uri={}&response_type=code&scope={}&state={}#wechat_redirect",
                OAUTH_AUTHORIZE_URL,
                self.app_id,
                encode_uri(redirect),
                self.scopes.join(","),
                state
            ),
        }
    }

    /// 通过 code 换取网页授权 access_token
    pub async fn access_token(&self, client: &Client, code: &str) -> Result<Value> {
        if let Some(component) = &self.component {
            return self
                .component_access_token(client, component, "/sns/oauth2/component/access_token", &[
                    ("appid", self.app_id.as_str()),
                    ("code", code),
                    ("grant_type", "authorization_code"),
                ])
                .await;
        }

        let url = Client::build_url(
            API_DOMAIN,
            "/sns/oauth2/access_token",
            &[
                ("appid", self.app_id.as_str()),
                ("secret", self.secret.as_str()),
                ("code", code),
                ("grant_type", "authorization_code"),
            ],
        );

        client.get_json(&url).await
    }

    /// 组件代换请求：追加 component_appid / component_access_token
    async fn component_access_token(
        &self,
        client: &Client,
        component: &ComponentOAuth,
        path: &str,
        params: &[(&str, &str)],
    ) -> Result<Value> {
        let component_token = component.get_token().await?;

        let mut query: Vec<(&str, &str)> = params.to_vec();
        query.push(("component_appid", component.component_app_id()));
        query.push(("component_access_token", component_token.as_str()));

        let url = Client::build_url(API_DOMAIN, path, &query);
        client.get_json(&url).await
    }

    /// code → token → 用户信息，对齐 socialite `user()`
    pub async fn user_from_code(&self, client: &Client, code: &str, lang: Option<&str>) -> Result<Value> {
        let token = self.access_token(client, code).await?;
        let open_id = token["openid"].as_str().unwrap_or_default();
        let access_token = token["access_token"].as_str().unwrap_or_default();

        if open_id.is_empty() || access_token.is_empty() {
            return Ok(token);
        }

        self.user(client, open_id, access_token, lang).await
    }

    /// 刷新网页授权 access_token
    pub async fn refresh_token(&self, client: &Client, refresh_token: &str) -> Result<Value> {
        let url = Client::build_url(
            API_DOMAIN,
            "/sns/oauth2/refresh_token",
            &[
                ("appid", self.app_id.as_str()),
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh_token),
            ],
        );

        client.get_json(&url).await
    }

    /// 拉取用户信息（scope 需为 snsapi_userinfo）
    pub async fn user(
        &self,
        client: &Client,
        open_id: &str,
        access_token: &str,
        lang: Option<&str>,
    ) -> Result<Value> {
        let url = Client::build_url(
            API_DOMAIN,
            "/sns/userinfo",
            &[
                ("access_token", access_token),
                ("openid", open_id),
                ("lang", lang.unwrap_or("zh_CN")),
            ],
        );

        client.get_json(&url).await
    }

    /// 校验网页授权 access_token 是否有效
    pub async fn check_token(
        &self,
        client: &Client,
        open_id: &str,
        access_token: &str,
    ) -> Result<Value> {
        let url = Client::build_url(
            API_DOMAIN,
            "/sns/auth",
            &[("access_token", access_token), ("openid", open_id)],
        );

        client.get_json(&url).await
    }
}

/// 按 RFC3986 对 URL 编码（保留 unreserved 字符，对齐 PHP urlencode）
pub fn encode_uri(value: &str) -> String {
    use percent_encoding::{utf8_percent_encode, AsciiSet, CONTROLS};

    const UNRESERVED_EXCLUDED: &AsciiSet = &CONTROLS
        .add(b' ')
        .add(b'"')
        .add(b'#')
        .add(b'$')
        .add(b'%')
        .add(b'&')
        .add(b'\'')
        .add(b'(')
        .add(b')')
        .add(b'*')
        .add(b'+')
        .add(b',')
        .add(b'/')
        .add(b':')
        .add(b';')
        .add(b'<')
        .add(b'=')
        .add(b'>')
        .add(b'?')
        .add(b'@')
        .add(b'[')
        .add(b'\\')
        .add(b']')
        .add(b'^')
        .add(b'`')
        .add(b'{')
        .add(b'|')
        .add(b'}');

    utf8_percent_encode(value, UNRESERVED_EXCLUDED).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authorize_url_contains_required_params() {
        let oauth = OAuth::new("wx123", "secret", "https://example.com/cb");
        let url = oauth.authorize_url("https://example.com/cb", "xyz");
        assert!(url.starts_with(OAUTH_AUTHORIZE_URL));
        assert!(url.contains("appid=wx123"));
        assert!(url.contains("scope=snsapi_userinfo"));
        assert!(url.contains("state=xyz"));
        assert!(url.ends_with("#wechat_redirect"));
    }

    #[test]
    fn authorize_url_falls_back_to_config() {
        let oauth = OAuth::new("wx123", "secret", "https://example.com/cb").with_state("cfg");
        let url = oauth.authorize_url("", "");
        assert!(url.contains("redirect_uri=https%3A%2F%2Fexample.com%2Fcb"));
        assert!(url.contains("state=cfg"));
    }
}
