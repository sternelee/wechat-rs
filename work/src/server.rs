//! copyright © ecdata.cn 2021 - present
//! 企业微信消息服务器，对齐 EasyWeChat\Work\Server
//! created by shaipe 20210228

use crate::encryptor::Encryptor;
use serde_json::{json, Value};
use std::sync::Arc;
use wechat_sdk::{constant::MSG_TYPE_XML, xml, ServerResponse, WechatResult as Result};
use std::collections::HashMap;

/// 默认回复
pub const DEFAULT_REPLY: &str = "SUCCESS";

/// 回调处理器：返回 Some 即终止链，返回 None 交给下一个
pub type Handler = Arc<dyn Fn(&Value) -> Option<Value> + Send + Sync>;

/// 企业微信消息服务器
#[derive(Clone)]
pub struct Server {
    encryptor: Option<Encryptor>,
    message_type: String,
    handlers: Vec<Handler>,
}

impl std::fmt::Debug for Server {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Server")
            .field("message_type", &self.message_type)
            .field("handlers", &self.handlers.len())
            .finish()
    }
}

/// 取字符串字段
fn str_field(message: &Value, key: &str) -> Option<String> {
    message.get(key).and_then(|value| match value {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    })
}

impl Server {
    /// 创建消息服务器
    pub fn new(encryptor: Option<Encryptor>) -> Server {
        Server {
            encryptor,
            message_type: MSG_TYPE_XML.to_owned(),
            handlers: Vec::new(),
        }
    }

    /// 追加处理器，对齐 InteractWithHandlers::with
    pub fn with_handler<F>(mut self, handler: F) -> Self
    where
        F: Fn(&Value) -> Option<Value> + Send + Sync + 'static,
    {
        self.handlers.push(Arc::new(handler));
        self
    }

    /// 按 MsgType 监听，对齐 addMessageListener
    pub fn on_message_type<F>(self, message_type: &str, handler: F) -> Self
    where
        F: Fn(&Value) -> Option<Value> + Send + Sync + 'static,
    {
        let message_type = message_type.to_owned();

        self.with_handler(move |message| {
            if str_field(message, "MsgType").as_deref() == Some(message_type.as_str()) {
                handler(message)
            } else {
                None
            }
        })
    }

    /// 按 Event 监听，对齐 addEventListener
    pub fn on_event<F>(self, event: &str, handler: F) -> Self
    where
        F: Fn(&Value) -> Option<Value> + Send + Sync + 'static,
    {
        let event = event.to_owned();

        self.with_handler(move |message| {
            if str_field(message, "Event").as_deref() == Some(event.as_str()) {
                handler(message)
            } else {
                None
            }
        })
    }

    /// 通讯录变更事件（Event=change_contact），对齐 handleContactChanged
    pub fn handle_contact_changed<F>(self, handler: F) -> Self
    where
        F: Fn(&Value) -> Option<Value> + Send + Sync + 'static,
    {
        self.on_event("change_contact", handler)
    }

    /// 标签变更（change_contact + ChangeType=update_tag），对齐 handleUserTagUpdated
    pub fn handle_user_tag_updated<F>(self, handler: F) -> Self
    where
        F: Fn(&Value) -> Option<Value> + Send + Sync + 'static,
    {
        self.on_change_type("update_tag", handler)
    }

    /// 成员创建，对齐 handleUserCreated
    pub fn handle_user_created<F>(self, handler: F) -> Self
    where
        F: Fn(&Value) -> Option<Value> + Send + Sync + 'static,
    {
        self.on_change_type("create_user", handler)
    }

    /// 成员更新，对齐 handleUserUpdated
    pub fn handle_user_updated<F>(self, handler: F) -> Self
    where
        F: Fn(&Value) -> Option<Value> + Send + Sync + 'static,
    {
        self.on_change_type("update_user", handler)
    }

    /// 成员删除，对齐 handleUserDeleted
    pub fn handle_user_deleted<F>(self, handler: F) -> Self
    where
        F: Fn(&Value) -> Option<Value> + Send + Sync + 'static,
    {
        self.on_change_type("delete_user", handler)
    }

    /// 部门创建，对齐 handlePartyCreated
    pub fn handle_party_created<F>(self, handler: F) -> Self
    where
        F: Fn(&Value) -> Option<Value> + Send + Sync + 'static,
    {
        self.on_change_type("create_party", handler)
    }

    /// 部门更新，对齐 handlePartyUpdated
    pub fn handle_party_updated<F>(self, handler: F) -> Self
    where
        F: Fn(&Value) -> Option<Value> + Send + Sync + 'static,
    {
        self.on_change_type("update_party", handler)
    }

    /// 部门删除，对齐 handlePartyDeleted
    pub fn handle_party_deleted<F>(self, handler: F) -> Self
    where
        F: Fn(&Value) -> Option<Value> + Send + Sync + 'static,
    {
        self.on_change_type("delete_party", handler)
    }

    /// 批量任务结果（Event=batch_job_result），对齐 handleBatchJobsFinished
    pub fn handle_batch_jobs_finished<F>(self, handler: F) -> Self
    where
        F: Fn(&Value) -> Option<Value> + Send + Sync + 'static,
    {
        self.on_event("batch_job_result", handler)
    }

    /// change_contact + ChangeType 组合过滤
    fn on_change_type<F>(self, change_type: &str, handler: F) -> Self
    where
        F: Fn(&Value) -> Option<Value> + Send + Sync + 'static,
    {
        let change_type = change_type.to_owned();

        self.with_handler(move |message| {
            if str_field(message, "Event").as_deref() == Some("change_contact")
                && str_field(message, "ChangeType").as_deref() == Some(change_type.as_str())
            {
                handler(message)
            } else {
                None
            }
        })
    }

    /// 执行处理器链，对齐 Server::handle
    pub fn handle(&self, message: &Value) -> Option<Value> {
        for handler in &self.handlers {
            if let Some(response) = handler(message) {
                return Some(response);
            }
        }

        None
    }

    /// 完整回调流程，对齐 Work\Server::serve
    ///
    /// - echostr：用 Encryptor 解密后回显
    /// - 消息：解密 → 处理器链 → 默认 "SUCCESS" / 加密回复
    pub fn serve(&self, query: &HashMap<String, String>, body: &str) -> Result<ServerResponse> {
        if query
            .get("echostr")
            .map(|value| !value.is_empty())
            .unwrap_or(false)
        {
            let echostr = self.verify_url(query)?;

            return Ok(ServerResponse::ok(&echostr));
        }

        let message = self.request_message(query, body)?;
        let response = self.handle(&message);

        match response {
            None => Ok(ServerResponse::ok(DEFAULT_REPLY)),
            Some(response) => {
                let xml_reply = self.transform_value_reply(&response, &message)?;
                Ok(ServerResponse::xml(&xml_reply))
            }
        }
    }

    /// 处理器返回值（字符串或带 MsgType 的对象）→ 可加密回复
    pub fn transform_value_reply(&self, reply: &Value, request: &Value) -> Result<String> {
        let attributes = match reply {
            Value::String(text) => json!({"MsgType": "text", "Content": text}),
            Value::Number(number) => json!({"MsgType": "text", "Content": number.to_string()}),
            Value::Object(map) => {
                if !map.contains_key("MsgType") {
                    return Err(error! {code: -1, msg: "MsgType cannot be empty."});
                }

                reply.clone()
            }
            _ => {
                return Err(error! {
                    code: -1,
                    msg: "Invalid handler response.",
                })
            }
        };

        let plain = xml::build(&attributes);

        self.transform_reply(Some(plain), request, None, None)
    }

    /// 设置回复格式：xml / json
    pub fn with_format<S: Into<String>>(mut self, message_type: S) -> Server {
        self.message_type = message_type.into();
        self
    }

    /// 是否启用加解密
    pub fn has_encryptor(&self) -> bool {
        self.encryptor.is_some()
    }

    /// URL 校验：企业微信的回显内容需要用 Encryptor 解密
    ///
    /// 对齐 Server::serve 中 echostr 分支
    pub fn verify_url(&self, query: &HashMap<String, String>) -> Result<String> {
        let echostr = query.get("echostr").cloned().unwrap_or_default();
        if echostr.is_empty() {
            return Err(error! {code: -40001, msg: "Missing echostr in query."});
        }

        match &self.encryptor {
            Some(encryptor) => encryptor.decrypt(
                &echostr,
                query.get("msg_signature").map(|v| v.as_str()).unwrap_or_default(),
                query.get("nonce").map(|v| v.as_str()).unwrap_or_default(),
                query.get("timestamp").map(|v| v.as_str()).unwrap_or_default(),
            ),
            None => Ok(echostr),
        }
    }

    /// 解析请求消息，加密消息会先解密
    ///
    /// 对齐 getRequestMessage + decryptRequestMessage
    pub fn request_message(&self, query: &HashMap<String, String>, body: &str) -> Result<Value> {
        if let (Some(encryptor), Some(_)) = (&self.encryptor, query.get("msg_signature")) {
            if !body.trim().is_empty() && body.contains("<Encrypt>") {
                let plain = encryptor.decrypt_xml(body, query)?;
                return xml::to_json(&plain);
            }
        }

        xml::to_json(body)
    }

    /// 处理并生成回复
    ///
    /// 无回复时返回 "SUCCESS"，对齐 serve 的默认响应
    pub fn transform_reply(
        &self,
        reply: Option<String>,
        request: &Value,
        nonce: Option<&str>,
        timestamp: Option<u64>,
    ) -> Result<String> {
        let plain = match reply {
            Some(plain) => plain,
            None => return Ok(DEFAULT_REPLY.to_owned()),
        };

        let xml_reply = Self::swap_user(&plain, request)?;

        match &self.encryptor {
            Some(encryptor) => encryptor.encrypt(&xml_reply, nonce, timestamp, &self.message_type),
            None => Ok(xml_reply),
        }
    }

    /// 文本回复
    pub fn reply_text(&self, content: &str, request: &Value, encrypt: bool) -> Result<String> {
        let to_user = request["FromUserName"].as_str().unwrap_or_default();
        let from_user = request["ToUserName"].as_str().unwrap_or_default();
        let time = wechat_sdk::current_timestamp();

        let mut value = json!({
            "ToUserName": to_user,
            "FromUserName": from_user,
            "CreateTime": time,
            "MsgType": "text",
            "Content": content,
        });

        if !encrypt || self.encryptor.is_none() {
            return Ok(xml::build(&value));
        }

        let _ = &mut value;
        let plain = xml::build(&value);

        match &self.encryptor {
            Some(encryptor) => encryptor.encrypt(&plain, None, None, &self.message_type),
            None => Ok(plain),
        }
    }

    /// 反转请求中的收发双方与时间，对齐 transformToReply 的基础处理
    fn swap_user(plain: &str, request: &Value) -> Result<String> {
        let value = xml::to_json(plain)?;
        if !value.is_object() {
            return Ok(plain.to_owned());
        }

        let to_user = request["FromUserName"].as_str().unwrap_or_default();
        let from_user = request["ToUserName"].as_str().unwrap_or_default();

        let mut object = match value {
            Value::Object(map) => map,
            _ => return Ok(plain.to_owned()),
        };

        object.insert("ToUserName".to_owned(), Value::String(to_user.to_owned()));
        object.insert("FromUserName".to_owned(), Value::String(from_user.to_owned()));
        object.insert(
            "CreateTime".to_owned(),
            Value::Number(serde_json::Number::from(wechat_sdk::current_timestamp())),
        );

        Ok(xml::build(&Value::Object(object)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AES_KEY: &str = "kdjCGGJKSRjjhESfPO5lTSWtYS0v5pQX47skCkZczio";
    const TEXT_XML: &str = "<xml><ToUserName><![CDATA[corp]]></ToUserName>\
<FromUserName><![CDATA[user]]></FromUserName>\
<CreateTime>1348831860</CreateTime>\
<MsgType><![CDATA[text]]></MsgType>\
<Content><![CDATA[hello]]></Content>\
<MsgId>1234567890123456</MsgId></xml>";

    #[test]
    fn parse_plain_message_without_encryptor() {
        let server = Server::new(None);
        let message = server.request_message(&HashMap::new(), TEXT_XML).unwrap();
        assert_eq!(message["Content"], "hello");
        assert_eq!(server.transform_reply(None, &message, None, None).unwrap(), "SUCCESS");
    }

    #[test]
    fn reply_text_swaps_users() {
        let server = Server::new(None);
        let message = server.request_message(&HashMap::new(), TEXT_XML).unwrap();
        let reply = server.reply_text("world", &message, false).unwrap();

        assert!(reply.contains("<ToUserName><![CDATA[user]]></ToUserName>"));
        assert!(reply.contains("<FromUserName><![CDATA[corp]]></FromUserName>"));
        assert!(reply.contains("<Content><![CDATA[world]]></Content>"));
    }

    #[test]
    fn verify_url_decrypts_echostr() {
        let encryptor = Encryptor::new("corp", "token", AES_KEY).unwrap();
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

        let server = Server::new(Some(encryptor));
        assert_eq!(server.verify_url(&query).unwrap(), "echo-content");
    }

    #[test]
    fn verify_url_requires_echostr() {
        let server = Server::new(None);
        assert!(server.verify_url(&HashMap::new()).is_err());
    }

    #[test]
    fn serve_returns_success_by_default() {
        let server = Server::new(None);
        let response = server.serve(&HashMap::new(), TEXT_XML).unwrap();

        assert_eq!(response.status(), 200);
        assert_eq!(response.body(), DEFAULT_REPLY);
        assert_eq!(response.content_type(), "text/plain");
    }

    #[test]
    fn serve_dispatches_contact_changed() {
        let server = Server::new(None).handle_contact_changed(|message| {
            Some(json!({ "MsgType": "text", "Content": format!("ct:{}", message["ChangeType"].as_str().unwrap_or_default()) }))
        });

        let body = "<xml><ToUserName><![CDATA[corp]]></ToUserName>\
<FromUserName><![CDATA[sys]]></FromUserName>\
<CreateTime>1348831860</CreateTime>\
<MsgType><![CDATA[event]]></MsgType>\
<Event><![CDATA[change_contact]]></Event>\
<ChangeType><![CDATA[create_user]]></ChangeType></xml>";

        let response = server.serve(&HashMap::new(), body).unwrap();
        assert!(response.body().contains("<![CDATA[ct:create_user]]>"));
        assert!(response.body().contains("<ToUserName><![CDATA[sys]]></ToUserName>"));
    }

    #[test]
    fn handlers_are_matched_by_change_type() {
        let server = Server::new(None)
            .handle_user_created(|_| Some(json!("created")))
            .handle_user_tag_updated(|_| Some(json!("tag")));

        let body = "<xml><ToUserName><![CDATA[corp]]></ToUserName>\
<FromUserName><![CDATA[sys]]></FromUserName>\
<MsgType><![CDATA[event]]></MsgType>\
<Event><![CDATA[change_contact]]></Event>\
<ChangeType><![CDATA[update_tag]]></ChangeType></xml>";

        let message = server.request_message(&HashMap::new(), body).unwrap();
        let reply = server.handle(&message).unwrap();
        assert_eq!(reply, json!("tag"));
    }

    #[test]
    fn message_listener_filters_by_msg_type() {
        let server = Server::new(None)
            .on_message_type("image", |_| Some(json!("image")))
            .on_message_type("text", |message| {
                Some(json!(format!("text:{}", message["Content"].as_str().unwrap_or_default())))
            });

        let message = server.request_message(&HashMap::new(), TEXT_XML).unwrap();
        assert_eq!(server.handle(&message), Some(json!("text:hello")));
    }

    #[test]
    fn batch_job_result_event_is_dispatched() {
        let server = Server::new(None).handle_batch_jobs_finished(|_| Some(json!("done")));

        let body = "<xml><ToUserName><![CDATA[corp]]></ToUserName>\
<FromUserName><![CDATA[sys]]></FromUserName>\
<MsgType><![CDATA[event]]></MsgType>\
<Event><![CDATA[batch_job_result]]></Event></xml>";

        let message = server.request_message(&HashMap::new(), body).unwrap();
        assert_eq!(server.handle(&message), Some(json!("done")));
    }

    #[test]
    fn serve_encrypts_reply_in_safe_mode() {
        let encryptor = Encryptor::new("corp", "token", AES_KEY).unwrap();
        let server = Server::new(Some(encryptor.clone()))
            .on_message_type("text", |_| Some(json!("pong")));

        let response = server.serve(&HashMap::new(), TEXT_XML).unwrap();
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
        assert!(plain.contains("<![CDATA[pong]]>"));
    }
}
