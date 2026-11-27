//! copyright © ecdata.cn 2021 - present
//! 开放平台消息服务端，对齐 EasyWeChat\OpenPlatform\Server

use std::collections::HashMap;
use serde_json::Value;
use wechat_sdk::{constant::MSG_TYPE_XML, signature, Encryptor, WechatResult as Result};

/// 默认回复，对齐 Server 的 'success'
pub const DEFAULT_REPLY: &str = "success";

/// 处理器匹配类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandlerKind {
    /// InfoType
    InfoType,
    /// InfoType + AuthorizerAppid
    Authorizer,
}

/// 处理器
type Handler = Box<dyn Fn(&Value) -> Option<Value> + Send + Sync>;

/// 开放平台服务端
#[derive(Default)]
pub struct Server {
    token: String,
    encryptor: Option<Encryptor>,
    handlers: Vec<(HandlerKind, String, String, Handler)>,
}

impl Server {
    /// 创建服务端，token 用于校验明文请求签名
    pub fn new(token: &str) -> Server {
        Server {
            token: token.to_owned(),
            ..Server::default()
        }
    }

    /// 启用消息加解密
    pub fn with_encryptor(mut self, encryptor: Encryptor) -> Self {
        self.encryptor = Some(encryptor);
        self
    }

    /// 是否启用加密
    pub fn has_encryptor(&self) -> bool {
        self.encryptor.is_some()
    }

    /// token
    pub fn get_token(&self) -> &str {
        &self.token
    }

    /// 校验明文请求签名
    pub fn validate_plain_request(&self, query: &HashMap<String, String>) -> bool {
        let (timestamp, nonce, signature_value) = match (
            query.get("timestamp"),
            query.get("nonce"),
            query.get("signature"),
        ) {
            (Some(timestamp), Some(nonce), Some(signature_value)) => {
                (timestamp, nonce, signature_value)
            }
            _ => return false,
        };

        signature::verify_signature(&self.token, timestamp, nonce, signature_value)
    }

    /// 授权事件 InfoType
    pub fn info_type(message: &Value) -> Option<&str> {
        message.get("InfoType").and_then(Value::as_str)
    }

    /// 注册按 InfoType 匹配的处理器
    pub fn on_info_type<F>(mut self, info_type: &str, handler: F) -> Self
    where
        F: Fn(&Value) -> Option<Value> + Send + Sync + 'static,
    {
        self.handlers.push((
            HandlerKind::InfoType,
            info_type.to_owned(),
            String::new(),
            Box::new(handler),
        ));
        self
    }

    /// 注册按 InfoType + AuthorizerAppid 匹配的处理器
    pub fn on_authorizer<F>(mut self, info_type: &str, app_id: &str, handler: F) -> Self
    where
        F: Fn(&Value) -> Option<Value> + Send + Sync + 'static,
    {
        self.handlers.push((
            HandlerKind::Authorizer,
            info_type.to_owned(),
            app_id.to_owned(),
            Box::new(handler),
        ));
        self
    }

    /// 处理推送，命中处理器返回其响应，否则返回默认值
    pub fn handle(&self, message: &Value) -> Option<Value> {
        let info_type = match Server::info_type(message) {
            Some(info_type) => info_type,
            None => return None,
        };
        let app_id = message
            .get("AuthorizerAppid")
            .and_then(Value::as_str)
            .unwrap_or_default();

        for (kind, expected, expected_app, handler) in &self.handlers {
            let matched = match kind {
                HandlerKind::InfoType => expected == info_type,
                HandlerKind::Authorizer => {
                    expected == info_type && expected_app == app_id
                }
            };

            if matched {
                if let Some(reply) = handler(message) {
                    return Some(reply);
                }
            }
        }

        None
    }

    /// 收到授权事件，对齐 handleAuthorized
    pub fn handle_authorized(&self, message: &Value) -> Option<Value> {
        self.handle(message)
    }

    /// 消息解密：加密推送使用 Encryptor 解密，明文直接返回解析结果
    pub fn decrypt_message(&self, query: &HashMap<String, String>, body: &str) -> Result<Value> {
        if let Some(encryptor) = &self.encryptor {
            if body.contains("<Encrypt>") {
                return wechat_sdk::xml::to_json(&encryptor.decrypt_xml(body, query)?);
            }
        }

        wechat_sdk::xml::to_json(body)
    }

    /// 处理推送请求，返回响应体
    pub async fn serve(
        &self,
        query: &HashMap<String, String>,
        body: &str,
    ) -> Result<(u16, String)> {
        if let Some(echostr) = query.get("echostr") {
            if self.validate_plain_request(query) {
                return Ok((200, echostr.clone()));
            }

            if let Some(encryptor) = &self.encryptor {
                let nonce = query.get("nonce").map(String::as_str).unwrap_or_default();
                let timestamp = query.get("timestamp").map(String::as_str).unwrap_or_default();
                let msg_signature = query
                    .get("msg_signature")
                    .map(String::as_str)
                    .unwrap_or_default();

                return Ok((
                    200,
                    encryptor.decrypt(echostr, msg_signature, timestamp, nonce)?,
                ));
            }

            return Err(error! {code: -40001, msg: "Invalid Signature"});
        }

        let message = self.decrypt_message(query, body)?;
        let reply = self.handle(&message);

        self.transform_to_reply(reply, &message)
    }

    /// 处理器结果 → 响应，对齐 RespondXmlMessage::transformToReply
    ///
    /// 字符串结果视为文本消息，重写 To/From、补 CreateTime，
    /// 并在配置了 Encryptor 时加密（第三方平台安全模式必需）。
    pub fn transform_to_reply(
        &self,
        reply: Option<Value>,
        message: &Value,
    ) -> Result<(u16, String)> {
        let reply = match reply {
            Some(reply) => reply,
            None => return Ok((200, DEFAULT_REPLY.to_owned())),
        };

        // 对齐 RespondXmlMessage：字符串视为 text，对象保留全部属性
        let mut attributes = wechat_sdk::server::normalize_reply(&reply)?;

        if attributes
            .get("Content")
            .and_then(Value::as_str)
            .map(|c| c.is_empty())
            .unwrap_or(false)
        {
            return Ok((200, DEFAULT_REPLY.to_owned()));
        }

        attributes.insert(
            "ToUserName".to_owned(),
            Value::String(message["FromUserName"].as_str().unwrap_or_default().to_owned()),
        );
        attributes.insert(
            "FromUserName".to_owned(),
            Value::String(message["ToUserName"].as_str().unwrap_or_default().to_owned()),
        );
        attributes.insert(
            "CreateTime".to_owned(),
            Value::from(wechat_sdk::current_timestamp()),
        );

        let plain = wechat_sdk::xml::build(&Value::Object(attributes));

        match &self.encryptor {
            Some(encryptor) => Ok((
                200,
                encryptor.encrypt(&plain, None, None, MSG_TYPE_XML)?,
            )),
            None => Ok((200, plain)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn query(signature_value: &str) -> HashMap<String, String> {
        let mut query = HashMap::new();
        query.insert("timestamp".to_owned(), "1234".to_owned());
        query.insert("nonce".to_owned(), "nonce".to_owned());
        query.insert("signature".to_owned(), signature_value.to_owned());
        query
    }

    #[test]
    fn verify_url_rejects_bad_signature() {
        let server = Server::new("token");
        assert!(!server.validate_plain_request(&query("bad")));
    }

    #[test]
    fn verify_url_accepts_valid_signature() {
        let expected = signature::create_signature(&["token", "1234", "nonce"]);
        let server = Server::new("token");
        assert!(server.validate_plain_request(&query(&expected)));
    }

    #[tokio::test]
    async fn serve_echoes_echostr() {
        let expected = signature::create_signature(&["token", "1234", "nonce"]);
        let mut query = query(&expected);
        query.insert("echostr".to_owned(), "hello".to_owned());

        let (status, body) = Server::new("token").serve(&query, "").await.unwrap();
        assert_eq!(status, 200);
        assert_eq!(body, "hello");
    }

    #[tokio::test]
    async fn serve_handles_info_type() {
        let server = Server::new("token").on_info_type("authorized", |_| Some(Value::from("ok")));
        let body = "<xml><ToUserName>suite</ToUserName><FromUserName>sys</FromUserName>\
<InfoType>authorized</InfoType><AuthorizerAppid>wx1</AuthorizerAppid></xml>";

        let (status, reply) = server.serve(&HashMap::new(), body).await.unwrap();
        assert_eq!(status, 200);
        // 对齐 transformToReply：字符串结果包成文本消息并反转 To/From
        assert!(reply.contains("<Content><![CDATA[ok]]></Content>"));
        assert!(reply.contains("<ToUserName><![CDATA[sys]]></ToUserName>"));
        assert!(reply.contains("<FromUserName><![CDATA[suite]]></FromUserName>"));
    }

    #[test]
    fn transform_to_reply_encrypts_with_suite_encryptor() {
        let aes_key = "gFSml+n+BQ61hZqvRCxl/1SA8rpZFrgICtcfvF7PGcQ";
        let encryptor = Encryptor::new("suite", "token", aes_key, None).unwrap();
        let server = Server::new("token").with_encryptor(encryptor.clone());
        let message = json!({"ToUserName": "suite", "FromUserName": "sys"});

        let (status, body) = server
            .transform_to_reply(Some(json!("hello")), &message)
            .unwrap();
        assert_eq!(status, 200);
        assert!(body.contains("<Encrypt>"));

        let package = wechat_sdk::xmlutil::parse(&body);
        let doc = package.as_document();
        let ciphertext = wechat_sdk::xmlutil::evaluate(&doc, "//xml/Encrypt/text()").string();
        let msg_signature = wechat_sdk::xmlutil::evaluate(&doc, "//xml/MsgSignature/text()").string();
        let nonce = wechat_sdk::xmlutil::evaluate(&doc, "//xml/Nonce/text()").string();
        let timestamp = wechat_sdk::xmlutil::evaluate(&doc, "//xml/TimeStamp/text()").string();

        let plain = encryptor
            .decrypt(&ciphertext, &msg_signature, &nonce, &timestamp)
            .unwrap();
        assert!(plain.contains("<![CDATA[hello]]>"));
    }

    #[tokio::test]
    async fn serve_defaults_to_success() {
        let body = "<xml><InfoType>updateauthorized</InfoType></xml>";
        let (_, reply) = Server::new("token")
            .serve(&HashMap::new(), body)
            .await
            .unwrap();
        assert_eq!(reply, DEFAULT_REPLY);
    }

    #[test]
    fn authorizer_handler_matches_app_id() {
        let server = Server::new("token").on_authorizer("authorized", "wx1", |_| {
            Some(Value::from("hit"))
        });
        let message = json!({"InfoType": "authorized", "AuthorizerAppid": "wx1"});
        assert_eq!(
            server.handle(&message).and_then(|v| v.as_str().map(str::to_owned)),
            Some("hit".to_owned())
        );

        let other = json!({"InfoType": "authorized", "AuthorizerAppid": "wx2"});
        assert!(server.handle(&other).is_none());
    }
}
