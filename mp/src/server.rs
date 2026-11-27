//! copyright © ecdata.cn 2021 - present
//! 公众号消息服务器，对齐 EasyWeChat\OfficialAccount\Server
//! created by shaipe 20210302

use crate::application::Application;
use crate::message::Message;
use serde_json::Value;
use std::collections::HashMap;
use wechat_sdk::{xml, Server, ServerResponse, WechatResult as Result};

/// 消息服务器
#[derive(Debug, Clone)]
pub struct MpServer {
    inner: Server,
}

impl MpServer {
    /// 从应用创建消息服务器
    pub fn new(app: &Application) -> Result<MpServer> {
        Ok(MpServer {
            inner: app.server()?,
        })
    }

    /// 接入验证，返回 echostr
    pub fn verify_url(&self, query: &HashMap<String, String>) -> Result<String> {
        self.inner.verify_url(query)
    }

    /// 解析消息为 JSON，对齐 Server::getRequestMessage()
    pub fn request_message(&self, query: &HashMap<String, String>, body: &str) -> Result<Value> {
        self.inner.parse(query, body)
    }

    /// 解析消息为强类型 Message
    pub fn message(&self, query: &HashMap<String, String>, body: &str) -> Result<Message> {
        let value = self.request_message(query, body)?;
        let plain = value.as_str().unwrap_or_default().to_owned();
        let xml = if plain.is_empty() {
            xml::build(&value)
        } else {
            plain
        };

        Ok(Message::parse(&xml))
    }

    /// 把回复内容转换为回复 XML，必要时加密
    ///
    /// 对齐 Server::transformToReply：自动反转 ToUserName / FromUserName，
    /// 并补上 CreateTime，最后在安全模式下用 Encryptor 加密。
    pub fn transform_reply(
        &self,
        message: &Value,
        reply: &Value,
        nonce: Option<&str>,
        timestamp: Option<u64>,
    ) -> Result<String> {
        let mut body = serde_json::Map::new();
        body.insert(
            "ToUserName".to_owned(),
            Value::String(message["FromUserName"].as_str().unwrap_or_default().to_owned()),
        );
        body.insert(
            "FromUserName".to_owned(),
            Value::String(message["ToUserName"].as_str().unwrap_or_default().to_owned()),
        );
        body.insert(
            "CreateTime".to_owned(),
            Value::from(wechat_sdk::current_timestamp()),
        );

        if let Some(map) = reply.as_object() {
            for (k, v) in map {
                body.insert(k.to_owned(), v.to_owned());
            }
        }

        let plain = xml::build(&Value::Object(body));
        self.inner.encrypt_reply(&plain, nonce, timestamp)
    }

    /// 回复纯文本消息
    pub fn reply_text(
        &self,
        message: &Value,
        content: &str,
        nonce: Option<&str>,
        timestamp: Option<u64>,
    ) -> Result<String> {
        let reply = serde_json::json!({
            "MsgType": "text",
            "Content": content,
        });

        self.transform_reply(message, &reply, nonce, timestamp)
    }

    /// 底层 sdk Server
    pub fn inner(&self) -> &Server {
        &self.inner
    }

    /// 追加处理器，对齐 InteractWithHandlers::with
    pub fn with_handler<F>(mut self, handler: F) -> Self
    where
        F: Fn(&Value) -> Option<Value> + Send + Sync + 'static,
    {
        self.inner = self.inner.with_handler(handler);
        self
    }

    /// 按 MsgType 监听，对齐 addMessageListener
    pub fn on_message_type<F>(mut self, message_type: &str, handler: F) -> Self
    where
        F: Fn(&Value) -> Option<Value> + Send + Sync + 'static,
    {
        self.inner = self.inner.on_message_type(message_type, handler);
        self
    }

    /// 按 Event 监听，对齐 addEventListener
    pub fn on_event<F>(mut self, event: &str, handler: F) -> Self
    where
        F: Fn(&Value) -> Option<Value> + Send + Sync + 'static,
    {
        self.inner = self.inner.on_event(event, handler);
        self
    }

    /// 执行处理器链，对齐 Server::handle
    pub fn handle(&self, message: &Value) -> Option<Value> {
        self.inner.handle(message)
    }

    /// 完整回调：echostr 回显 / 消息解密 / 处理器 / 加密回复
    ///
    /// 对齐 OfficialAccount\Server::serve（MiniApp\Server 直接继承同一实现）。
    pub fn serve(&self, query: &HashMap<String, String>, body: &str) -> Result<ServerResponse> {
        self.inner.serve(query, body)
    }

    /// 处理器结果转成回复，对齐 transformToReply
    pub fn transform_to_reply(&self, reply: Option<Value>, message: &Value) -> Result<ServerResponse> {
        self.inner.transform_to_reply(reply, message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MpConfig;
    use wechat_sdk::signature::create_signature;

    fn query(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn verify_url_and_parse_text_message() {
        let app = Application::new(MpConfig::new("wx123", "token").with_token("token"));
        let server = MpServer::new(&app).unwrap();

        let timestamp = "1587121403";
        let nonce = "1661748508";
        let signature = create_signature(&["token", timestamp, nonce]);
        let q = query(&[
            ("echostr", "echo"),
            ("timestamp", timestamp),
            ("nonce", nonce),
            ("signature", &signature),
        ]);
        assert_eq!(server.verify_url(&q).unwrap(), "echo");

        let body = "<xml>\
            <ToUserName><![CDATA[gh_1]]></ToUserName>\
            <FromUserName><![CDATA[oUser]]></FromUserName>\
            <CreateTime>1348831860</CreateTime>\
            <MsgType><![CDATA[text]]></MsgType>\
            <Content><![CDATA[hello]]></Content>\
            <MsgId>1234567890123456</MsgId>\
            </xml>";
        let msg = server.request_message(&q, body).unwrap();
        assert_eq!(msg["Content"], "hello");

        let parsed = server.message(&q, body).unwrap();
        assert_eq!(parsed.get_from_user(), "oUser");

        let reply = server.reply_text(&msg, "hi", None, None).unwrap();
        assert!(reply.contains("<![CDATA[oUser]]>"));
        assert!(reply.contains("<![CDATA[hi]]>"));
    }

    #[test]
    fn serve_dispatches_to_message_listener() {
        let app = Application::new(MpConfig::new("wx123", "token").with_token("token"));
        let server = MpServer::new(&app)
            .unwrap()
            .on_message_type("text", |message| {
                Some(serde_json::json!({
                    "MsgType": "text",
                    "Content": format!("echo:{}", message["Content"].as_str().unwrap_or_default()),
                }))
            });

        let timestamp = "1587121403";
        let nonce = "1661748508";
        let signature = create_signature(&["token", timestamp, nonce]);
        let q = query(&[
            ("timestamp", timestamp),
            ("nonce", nonce),
            ("signature", &signature),
        ]);
        let body = "<xml>\
            <ToUserName><![CDATA[gh_1]]></ToUserName>\
            <FromUserName><![CDATA[oUser]]></FromUserName>\
            <CreateTime>1348831860</CreateTime>\
            <MsgType><![CDATA[text]]></MsgType>\
            <Content><![CDATA[hello]]></Content>\
            </xml>";

        let response = server.serve(&q, body).unwrap();
        assert_eq!(response.status(), 200);
        assert!(response.body().contains("<![CDATA[echo:hello]]>"));
        assert!(response.body().contains("<![CDATA[oUser]]>"));
        assert!(response.body().contains("<![CDATA[gh_1]]>"));
    }

    #[test]
    fn serve_returns_success_without_handler_match() {
        let app = Application::new(MpConfig::new("wx123", "token").with_token("token"));
        let server = MpServer::new(&app)
            .unwrap()
            .on_event("subscribe", |_| Some(serde_json::json!("welcome")));

        let body = "<xml>\
            <ToUserName><![CDATA[gh_1]]></ToUserName>\
            <FromUserName><![CDATA[oUser]]></FromUserName>\
            <MsgType><![CDATA[text]]></MsgType>\
            <Content><![CDATA[hello]]></Content>\
            </xml>";

        let response = server.serve(&signed_query("token"), body).unwrap();
        assert_eq!(response.body(), "success");
        assert_eq!(response.content_type(), "text/plain");
    }

    /// 构造带合法签名的明文请求 query（明文回调必须带 signature）
    fn signed_query(token: &str) -> HashMap<String, String> {
        let timestamp = "1587121403";
        let nonce = "1661748508";
        let signature = create_signature(&[token, timestamp, nonce]);

        [
            ("timestamp", timestamp),
            ("nonce", nonce),
            ("signature", signature.as_str()),
        ]
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
    }

    #[test]
    fn serve_echoes_echostr() {
        let app = Application::new(MpConfig::new("wx123", "token").with_token("token"));
        let server = MpServer::new(&app).unwrap();

        let timestamp = "1587121403";
        let nonce = "1661748508";
        let signature = create_signature(&["token", timestamp, nonce]);
        let q = query(&[
            ("echostr", "echo-123"),
            ("timestamp", timestamp),
            ("nonce", nonce),
            ("signature", &signature),
        ]);

        assert_eq!(server.serve(&q, "").unwrap().body(), "echo-123");
    }
}
