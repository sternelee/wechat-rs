//! copyright © ecdata.cn 2021 - present
//! 服务商消息服务器，对齐 EasyWeChat\OpenWork\Server

use serde_json::Value;
use std::collections::HashMap;
use wechat_sdk::{constant::MSG_TYPE_XML, xml, Encryptor, ServerResponse, WechatResult as Result};

/// 默认回复内容（注意：服务商回调默认回复 success 小写）
pub const DEFAULT_REPLY: &str = "success";

/// 回调匹配条件
#[derive(Debug, Clone, PartialEq)]
pub enum HandlerKind {
    /// InfoType 匹配，如 suite_ticket / create_auth
    InfoType(&'static str),
    /// InfoType=change_contact 且 ChangeType 匹配
    ChangeContact(&'static str),
    /// MsgType=event 且 Event 匹配
    Event(&'static str),
}

impl HandlerKind {
    /// 判断消息是否匹配
    pub fn matches(&self, message: &Value) -> bool {
        let str_of = |key: &str| message[key].as_str().unwrap_or_default().to_owned();

        match self {
            HandlerKind::InfoType(value) => str_of("InfoType") == *value,
            HandlerKind::ChangeContact(value) => {
                str_of("InfoType") == "change_contact" && str_of("ChangeType") == *value
            }
            HandlerKind::Event(value) => str_of("MsgType") == "event" && str_of("Event") == *value,
        }
    }
}

/// 回调处理器
pub type MessageHandler = Box<dyn Fn(&Value) -> Option<Value> + Send + Sync>;

/// 服务商消息服务器，对齐 OpenWork\Server
pub struct Server {
    encryptor: Option<Encryptor>,
    provider_encryptor: Option<Encryptor>,
    handlers: Vec<(HandlerKind, MessageHandler)>,
}

impl std::fmt::Debug for Server {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Server")
            .field("handlers", &self.handlers.len())
            .finish()
    }
}

impl Server {
    /// 创建消息服务器
    ///
    /// - encryptor: 第三方应用（suite）加解密器
    /// - provider_encryptor: 服务商自身加解密器（用于 echostr 校验）
    pub fn new(encryptor: Option<Encryptor>, provider_encryptor: Option<Encryptor>) -> Server {
        Server {
            encryptor,
            provider_encryptor,
            handlers: Vec::new(),
        }
    }

    /// 注册处理器（对齐 with）
    pub fn with<F>(&mut self, kind: HandlerKind, handler: F) -> &mut Self
    where
        F: Fn(&Value) -> Option<Value> + Send + Sync + 'static,
    {
        self.handlers.push((kind, Box::new(handler)));
        self
    }

    /// suite_ticket 事件，对齐 handleSuiteTicketRefreshed
    pub fn handle_suite_ticket_refreshed<F>(&mut self, handler: F) -> &mut Self
    where
        F: Fn(&Value) -> Option<Value> + Send + Sync + 'static,
    {
        self.with(HandlerKind::InfoType("suite_ticket"), handler)
    }

    /// 授权成功，对齐 handleAuthCreated
    pub fn handle_auth_created<F>(&mut self, handler: F) -> &mut Self
    where
        F: Fn(&Value) -> Option<Value> + Send + Sync + 'static,
    {
        self.with(HandlerKind::InfoType("create_auth"), handler)
    }

    /// 授权变更，对齐 handleAuthChanged
    pub fn handle_auth_changed<F>(&mut self, handler: F) -> &mut Self
    where
        F: Fn(&Value) -> Option<Value> + Send + Sync + 'static,
    {
        self.with(HandlerKind::InfoType("change_auth"), handler)
    }

    /// 取消授权，对齐 handleAuthCancelled
    pub fn handle_auth_cancelled<F>(&mut self, handler: F) -> &mut Self
    where
        F: Fn(&Value) -> Option<Value> + Send + Sync + 'static,
    {
        self.with(HandlerKind::InfoType("cancel_auth"), handler)
    }

    /// 通讯录变更：新增成员
    pub fn handle_user_created<F>(&mut self, handler: F) -> &mut Self
    where
        F: Fn(&Value) -> Option<Value> + Send + Sync + 'static,
    {
        self.with(HandlerKind::ChangeContact("create_user"), handler)
    }

    /// 通讯录变更：更新成员
    pub fn handle_user_updated<F>(&mut self, handler: F) -> &mut Self
    where
        F: Fn(&Value) -> Option<Value> + Send + Sync + 'static,
    {
        self.with(HandlerKind::ChangeContact("update_user"), handler)
    }

    /// 通讯录变更：删除成员
    pub fn handle_user_deleted<F>(&mut self, handler: F) -> &mut Self
    where
        F: Fn(&Value) -> Option<Value> + Send + Sync + 'static,
    {
        self.with(HandlerKind::ChangeContact("delete_user"), handler)
    }

    /// 通讯录变更：新增部门
    pub fn handle_party_created<F>(&mut self, handler: F) -> &mut Self
    where
        F: Fn(&Value) -> Option<Value> + Send + Sync + 'static,
    {
        self.with(HandlerKind::ChangeContact("create_party"), handler)
    }

    /// 通讯录变更：更新部门
    pub fn handle_party_updated<F>(&mut self, handler: F) -> &mut Self
    where
        F: Fn(&Value) -> Option<Value> + Send + Sync + 'static,
    {
        self.with(HandlerKind::ChangeContact("update_party"), handler)
    }

    /// 通讯录变更：删除部门
    pub fn handle_party_deleted<F>(&mut self, handler: F) -> &mut Self
    where
        F: Fn(&Value) -> Option<Value> + Send + Sync + 'static,
    {
        self.with(HandlerKind::ChangeContact("delete_party"), handler)
    }

    /// 通讯录变更：标签更新，对齐 handleUserTagUpdated
    pub fn handle_user_tag_updated<F>(&mut self, handler: F) -> &mut Self
    where
        F: Fn(&Value) -> Option<Value> + Send + Sync + 'static,
    {
        self.with(HandlerKind::ChangeContact("update_tag"), handler)
    }

    /// 共享应用变更，对齐 handleShareAgentChanged
    pub fn handle_share_agent_changed<F>(&mut self, handler: F) -> &mut Self
    where
        F: Fn(&Value) -> Option<Value> + Send + Sync + 'static,
    {
        self.with(HandlerKind::InfoType("share_agent_change"), handler)
    }

    /// 重置永久授权码，对齐 handleResetPermanentCode
    pub fn handle_reset_permanent_code<F>(&mut self, handler: F) -> &mut Self
    where
        F: Fn(&Value) -> Option<Value> + Send + Sync + 'static,
    {
        self.with(HandlerKind::InfoType("reset_permanent_code"), handler)
    }

    /// 变更应用管理员，对齐 handleChangeAppAdmin
    pub fn handle_change_app_admin<F>(&mut self, handler: F) -> &mut Self
    where
        F: Fn(&Value) -> Option<Value> + Send + Sync + 'static,
    {
        self.with(HandlerKind::Event("change_app_admin"), handler)
    }

    /// URL 校验：echostr 由服务商加解密器解密后回显
    pub fn verify_url(&self, query: &HashMap<String, String>) -> Result<String> {
        let echostr = query.get("echostr").cloned().unwrap_or_default();
        if echostr.is_empty() {
            return Err(error! {code: -40001, msg: "Missing echostr in query."});
        }

        match &self.provider_encryptor {
            Some(encryptor) => encryptor.decrypt(
                &echostr,
                query.get("msg_signature").map(|v| v.as_str()).unwrap_or_default(),
                query.get("nonce").map(|v| v.as_str()).unwrap_or_default(),
                query.get("timestamp").map(|v| v.as_str()).unwrap_or_default(),
            ),
            None => Ok(echostr),
        }
    }

    /// 解析请求消息，加密消息用 suite 加解密器解密
    pub fn request_message(&self, query: &HashMap<String, String>, body: &str) -> Result<Value> {
        if let (Some(encryptor), Some(_)) = (&self.encryptor, query.get("msg_signature")) {
            if body.contains("<Encrypt>") {
                let plain = encryptor.decrypt_xml(body, query)?;
                return xml::to_json(&plain);
            }
        }

        xml::to_json(body)
    }

    /// 执行处理器链，返回第一个非空回复
    pub fn handle(&self, message: &Value) -> Option<Value> {
        for (kind, handler) in &self.handlers {
            if kind.matches(message) {
                if let Some(reply) = handler(message) {
                    return Some(reply);
                }
            }
        }

        None
    }

    /// 完整回调处理，对齐 serve()
    ///
    /// - echostr：用 provider encryptor 解密后回显（text/plain）
    /// - 消息：suite encryptor 解密 → 处理器链
    /// - 处理器结果：重写 To/From、补 CreateTime，并用 suite encryptor 加密回复
    /// - 无处理器命中：返回默认 "success"
    pub fn serve(
        &self,
        query: &HashMap<String, String>,
        body: &str,
    ) -> Result<ServerResponse> {
        if query
            .get("echostr")
            .map(|value| !value.is_empty())
            .unwrap_or(false)
        {
            let echostr = self.verify_url(query)?;

            return Ok(ServerResponse::ok(&echostr));
        }

        let message = self.request_message(query, body)?;
        let reply = self.handle(&message);

        self.transform_to_reply(reply, &message)
    }

    /// 处理器结果 → 响应，对齐 transformToReply($response, $message, $this->encryptor)
    pub fn transform_to_reply(
        &self,
        reply: Option<Value>,
        message: &Value,
    ) -> Result<ServerResponse> {
        let reply = match reply {
            Some(reply) => reply,
            None => return Ok(ServerResponse::ok(DEFAULT_REPLY)),
        };

        // 对齐 RespondXmlMessage：字符串视为 text，对象保留全部属性（news/image/voice...）
        let mut attributes = wechat_sdk::server::normalize_reply(&reply)?;

        if attributes
            .get("Content")
            .and_then(Value::as_str)
            .map(|c| c.is_empty())
            .unwrap_or(false)
        {
            return Ok(ServerResponse::ok(DEFAULT_REPLY));
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

        let plain = xml::build(&Value::Object(attributes));

        match &self.encryptor {
            Some(encryptor) => Ok(ServerResponse::xml(&encryptor.encrypt(
                &plain,
                None,
                None,
                MSG_TYPE_XML,
            )?)),
            None => Ok(ServerResponse::xml(&plain)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AES_KEY: &str = "kdjCGGJKSRjjhESfPO5lTSWtYS0v5pQX47skCkZczio";

    fn text_event(info_type: &str) -> serde_json::Value {
        serde_json::json!({
            "ToUserName": "suite",
            "FromUserName": "sys",
            "CreateTime": 1500000000,
            "MsgType": "event",
            "InfoType": info_type,
        })
    }

    #[test]
    fn handler_kind_matching() {
        let message = text_event("suite_ticket");
        assert!(HandlerKind::InfoType("suite_ticket").matches(&message));
        assert!(!HandlerKind::InfoType("create_auth").matches(&message));
        assert!(!HandlerKind::Event("change_app_admin").matches(&message));
    }

    #[test]
    fn change_contact_matching() {
        let message = serde_json::json!({"InfoType": "change_contact", "ChangeType": "create_user"});
        assert!(HandlerKind::ChangeContact("create_user").matches(&message));
        assert!(!HandlerKind::ChangeContact("delete_user").matches(&message));
    }

    #[test]
    fn serve_dispatches_handlers() {
        let mut server = Server::new(None, None);
        server.handle_suite_ticket_refreshed(|message| {
            Some(Value::from(format!(
                "ticket:{}",
                message["InfoType"].as_str().unwrap_or_default()
            )))
        });

        let message = text_event("suite_ticket");
        assert_eq!(
            server.handle(&message).and_then(|v| v.as_str().map(str::to_owned)),
            Some("ticket:suite_ticket".to_owned())
        );

        let other = text_event("create_auth");
        assert_eq!(server.handle(&other), None);
    }

    #[test]
    fn serve_returns_default_reply() {
        let server = Server::new(None, None);
        let body = "<xml><ToUserName><![CDATA[suite]]></ToUserName>\
<FromUserName><![CDATA[sys]]></FromUserName>\
<CreateTime>1500000000</CreateTime>\
<MsgType><![CDATA[event]]></MsgType>\
<InfoType><![CDATA[create_auth]]></InfoType></xml>";

        let response = server.serve(&HashMap::new(), body).unwrap();
        assert_eq!(response.body(), DEFAULT_REPLY);
        assert_eq!(response.content_type(), "text/plain");
    }

    #[test]
    fn serve_encrypts_handler_reply() {
        let encryptor = Encryptor::new("suite", "token", AES_KEY, None).unwrap();
        let mut server = Server::new(Some(encryptor.clone()), None);
        server.handle_auth_created(|_| Some(Value::from("created")));

        let body = "<xml><ToUserName><![CDATA[suite]]></ToUserName>\
<FromUserName><![CDATA[sys]]></FromUserName>\
<CreateTime>1500000000</CreateTime>\
<MsgType><![CDATA[event]]></MsgType>\
<InfoType><![CDATA[create_auth]]></InfoType></xml>";

        let response = server.serve(&HashMap::new(), body).unwrap();
        assert_eq!(response.content_type(), "application/xml");
        assert!(response.body().contains("<Encrypt>"));

        let package = wechat_sdk::xmlutil::parse(response.body());
        let doc = package.as_document();
        let ciphertext = wechat_sdk::xmlutil::evaluate(&doc, "//xml/Encrypt/text()").string();
        let msg_signature = wechat_sdk::xmlutil::evaluate(&doc, "//xml/MsgSignature/text()").string();
        let nonce = wechat_sdk::xmlutil::evaluate(&doc, "//xml/Nonce/text()").string();
        let timestamp = wechat_sdk::xmlutil::evaluate(&doc, "//xml/TimeStamp/text()").string();

        let plain = encryptor
            .decrypt(&ciphertext, &msg_signature, &nonce, &timestamp)
            .unwrap();
        assert!(plain.contains("<![CDATA[created]]>"));
        assert!(plain.contains("<ToUserName><![CDATA[sys]]></ToUserName>"));
        assert!(plain.contains("<FromUserName><![CDATA[suite]]></FromUserName>"));
    }

    #[test]
    fn verify_url_uses_provider_encryptor() {
        let encryptor = Encryptor::new("corp", "token", AES_KEY, None).unwrap();
        let encrypted = encryptor.encrypt("echo-content", None, None, "xml").unwrap();

        let package = wechat_sdk::xmlutil::parse(&encrypted);
        let doc = package.as_document();
        let ciphertext = wechat_sdk::xmlutil::evaluate(&doc, "//xml/Encrypt/text()").string();
        let signature = wechat_sdk::xmlutil::evaluate(&doc, "//xml/MsgSignature/text()").string();
        let nonce = wechat_sdk::xmlutil::evaluate(&doc, "//xml/Nonce/text()").string();
        let timestamp = wechat_sdk::xmlutil::evaluate(&doc, "//xml/TimeStamp/text()").string();

        let mut query = HashMap::new();
        query.insert("echostr".to_owned(), ciphertext);
        query.insert("msg_signature".to_owned(), signature);
        query.insert("nonce".to_owned(), nonce);
        query.insert("timestamp".to_owned(), timestamp);

        let server = Server::new(None, Some(encryptor));
        assert_eq!(server.verify_url(&query).unwrap(), "echo-content");
    }
}
