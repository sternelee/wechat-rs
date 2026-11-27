//! copyright © ecdata.cn 2021 - present
//! 小程序应用入口，对齐 EasyWeChat\MiniApp\Application
//! created by shaipe 20210228

use crate::config::WeappConfig;
use crate::qrcode::QRCode;
use crate::subscribe::SubscribeMessage;
use crate::utils::Utils;
use serde::Serialize;
use serde_json::Value;
#[cfg(test)]
use std::collections::HashMap;
use wechat_sdk::{
    constant::API_DOMAIN, AccessToken, Client, Encryptor, Server, WechatResult as Result,
};

/// 小程序应用
///
/// ```
/// use wechat_weapp::{Application, WeappConfig};
/// let app = Application::new(WeappConfig::new("wx1", "secret"));
/// assert_eq!(app.account().app_id, "wx1");
/// ```
#[derive(Debug, Clone)]
pub struct Application {
    config: WeappConfig,
    access_token: AccessToken,
    client: Client,
    encryptor: Option<Encryptor>,
}

impl Application {
    /// 创建应用
    pub fn new(config: WeappConfig) -> Application {
        let access_token = AccessToken::new("mini_app", &config.app_id, &config.secret)
            .stable(config.use_stable_access_token);

        Application {
            config,
            access_token,
            client: Client::default(),
            encryptor: None,
        }
    }

    /// 注入消息加解密器（第三方平台代小程序时使用组件侧 Encryptor）
    pub fn with_encryptor(mut self, encryptor: Encryptor) -> Self {
        self.encryptor = Some(encryptor);
        self
    }

    /// 使用外部注入的 access_token（第三方平台代小程序调用时使用）
    pub fn with_access_token(mut self, access_token: AccessToken) -> Self {
        self.access_token = access_token;
        self
    }

    /// 配置
    pub fn config(&self) -> &WeappConfig {
        &self.config
    }

    /// 账号信息（对齐 getAccount）
    pub fn account(&self) -> &WeappConfig {
        &self.config
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

    /// 消息加解密器（对齐 getEncryptor）
    pub fn encryptor(&self) -> Result<Encryptor> {
        if !self.config.has_encryption() {
            return Err(error! {code: -1, msg: "token or aes_key cannot be empty."});
        }

        Encryptor::new(
            &self.config.app_id,
            &self.config.token,
            &self.config.aes_key,
            Some(&self.config.app_id),
        )
    }

    /// 消息服务器（对齐 getServer，小程序复用公众号 Server）
    ///
    /// 返回的 `sdk::Server` 支持 `serve()` / `with_handler()` / `on_message_type()` / `on_event()`，
    /// 回复会自动反转 To/From 并在安全模式下用 aes_key 加密。
    pub fn server(&self) -> Result<Server> {
        let aes_key = if self.config.aes_key.is_empty() {
            None
        } else {
            Some(self.config.aes_key.as_str())
        };

        let server = Server::new(
            &self.config.app_id,
            &self.config.token,
            aes_key,
            self.config.require_encryption,
        )?;

        Ok(match &self.encryptor {
            Some(encryptor) => server.with_encryptor(encryptor.clone()),
            None => server,
        })
    }

    /// 工具（对齐 getUtils）
    pub fn utils(&self) -> Utils<'_> {
        Utils::new(self)
    }

    /// 小程序码服务（复用应用的 token 配置，对齐 PHP 共享同一个 AccessToken 实例）
    pub fn qrcode(&self) -> QRCode {
        QRCode::new(&self.config.app_id, &self.config.secret)
            .with_stable(self.config.use_stable_access_token)
    }

    /// 订阅消息服务（复用应用的 token 配置）
    pub fn subscribe_message(&self) -> SubscribeMessage {
        SubscribeMessage::new(&self.config.app_id, &self.config.secret)
            .with_stable(self.config.use_stable_access_token)
    }

    /// 以 access_token 发起 GET（token 过期时刷新后重试一次）
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

    /// 以 access_token 发起 POST（token 过期时刷新后重试一次）
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

    /// 强制刷新 access_token（token 过期重试使用）
    async fn refresh_access_token(&self) -> Result<String> {
        Ok(self.access_token.refresh_forced().await?.access_token)
    }

    /// 无需 access_token 的 GET
    pub async fn get_without_token(&self, path: &str, query: &[(&str, &str)]) -> Result<Value> {
        let url = Client::build_url(API_DOMAIN, path, query);
        self.client.get_json(&url).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encryptor_requires_config() {
        let app = Application::new(WeappConfig::new("wx1", "secret"));
        assert!(app.encryptor().is_err());
        assert!(!app.server().unwrap().has_encryptor());
    }

    /// 小程序码/订阅消息复用应用的 token 配置（缓存 key 与 stable 模式一致）
    #[test]
    fn qrcode_and_subscribe_share_app_token_config() {
        let app = Application::new(WeappConfig::new("wx1", "secret").stable_access_token(true));
        assert!(app.qrcode().stable);
        assert!(app.subscribe_message().stable);
        assert_eq!(app.qrcode().app_id, "wx1");

        let plain = Application::new(WeappConfig::new("wx2", "secret"));
        assert!(!plain.qrcode().stable);
    }

    /// 生成合法的 43 位 EncodingAESKey（32 字节 base64 去掉 '='）
    fn test_aes_key() -> String {
        let bytes: Vec<u8> = (0u8..32).map(|i| i.wrapping_mul(7).wrapping_add(13)).collect();

        base64::encode(bytes).trim_end_matches('=').to_owned()
    }

    fn signed_query() -> HashMap<String, String> {
        let timestamp = "1587121403";
        let nonce = "1661748508";
        let signature = wechat_sdk::signature::create_signature(&["token", timestamp, nonce]);

        let mut query: HashMap<String, String> = HashMap::new();
        query.insert("timestamp".to_owned(), timestamp.to_owned());
        query.insert("nonce".to_owned(), nonce.to_owned());
        query.insert("signature".to_owned(), signature);

        query
    }

    #[test]
    fn encryptor_ready_when_configured() {
        let aes_key = test_aes_key();
        let app = Application::new(
            WeappConfig::new("wx1", "secret").with_message("token", &aes_key),
        );

        assert!(app.encryptor().is_ok());
        assert!(app.server().unwrap().has_encryptor());
    }

    #[test]
    fn server_serves_callback_with_handlers() {
        // MiniApp\Server 直接继承 OfficialAccount\Server，回调面完全一致
        let aes_key = test_aes_key();
        let app = Application::new(
            WeappConfig::new("wx1", "secret").with_message("token", &aes_key),
        );
        let server = app.server().unwrap().on_message_type("text", |message| {
            Some(serde_json::json!(format!(
                "mini:{}",
                message["Content"].as_str().unwrap_or_default()
            )))
        });

        let body = "<xml><ToUserName><![CDATA[gh_1]]></ToUserName>\
<FromUserName><![CDATA[oUser]]></FromUserName>\
<MsgType><![CDATA[text]]></MsgType>\
<Content><![CDATA[hi]]></Content></xml>";

        let response = server.serve(&signed_query(), body).unwrap();
        assert_eq!(response.status(), 200);
        // 配了 aes_key 即安全模式，回复必须是密文
        assert!(response.body().contains("<Encrypt>"));

        let encryptor = app.encryptor().unwrap();
        let package = wechat_sdk::xmlutil::parse(response.body());
        let doc = package.as_document();
        let ciphertext = wechat_sdk::xmlutil::evaluate(&doc, "//xml/Encrypt/text()").string();
        let msg_signature = wechat_sdk::xmlutil::evaluate(&doc, "//xml/MsgSignature/text()").string();
        let nonce = wechat_sdk::xmlutil::evaluate(&doc, "//xml/Nonce/text()").string();
        let timestamp = wechat_sdk::xmlutil::evaluate(&doc, "//xml/TimeStamp/text()").string();
        let plain = encryptor
            .decrypt(&ciphertext, &msg_signature, &nonce, &timestamp)
            .unwrap();
        assert!(plain.contains("<![CDATA[mini:hi]]>"));
        assert!(plain.contains("<ToUserName><![CDATA[oUser]]></ToUserName>"));
    }

    #[test]
    fn server_echoes_echostr() {
        let aes_key = test_aes_key();
        let app = Application::new(
            WeappConfig::new("wx1", "secret").with_message("token", &aes_key),
        );
        let server = app.server().unwrap();

        let mut query = signed_query();
        query.insert("echostr".to_owned(), "mini-echo".to_owned());

        assert_eq!(server.serve(&query, "").unwrap().body(), "mini-echo");
    }
}
