//! copyright © ecdata.cn 2021 - present
//! 微信消息服务器，对齐 EasyWeChat\Kernel/Server 与各 Application 的 Server
//! created by shaipe 20210228
//!
//! 负责：
//! 1. 服务器接入验证（echostr 回显）
//! 2. 普通模式签名校验
//! 3. 安全模式（AES）消息解密
//! 4. 把 XML 消息解析为 JSON 结构，方便上层处理

use crate::encryptor::Encryptor;
use crate::signature;
use crate::{xml, WechatResult as Result};
use std::collections::HashMap;
use std::sync::Arc;

/// 消息处理器：返回 Some 即终止链，返回 None 交给下一个
pub type MessageHandler = Arc<dyn Fn(&serde_json::Value) -> Option<serde_json::Value> + Send + Sync>;

/// 微信消息服务器
#[derive(Clone)]
pub struct Server {
    /// 消息校验 token
    token: String,
    /// 加解密器，安全模式下必须
    encryptor: Option<Encryptor>,
    /// 是否强制要求加密消息
    require_encryption: bool,
    /// 消息格式 xml/json
    message_format: String,
    /// 处理器链，对齐 InteractWithHandlers::handlers
    handlers: Vec<MessageHandler>,
}

impl std::fmt::Debug for Server {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Server")
            .field("token", &self.token)
            .field("require_encryption", &self.require_encryption)
            .field("message_format", &self.message_format)
            .field("handlers", &self.handlers.len())
            .finish()
    }
}

impl Server {
    /// 创建 Server
    /// - aes_key 为空表示仅使用明文模式
    pub fn new(
        app_id: &str,
        token: &str,
        aes_key: Option<&str>,
        require_encryption: bool,
    ) -> Result<Server> {
        let encryptor = match aes_key {
            Some(key) if !key.is_empty() => Some(Encryptor::new(app_id, token, key, None)?),
            _ => None,
        };

        Ok(Server {
            token: token.to_owned(),
            encryptor,
            require_encryption,
            message_format: "xml".to_owned(),
            handlers: Vec::new(),
        })
    }

    /// 追加处理器（对齐 with/withHandler）
    pub fn with_handler<F>(mut self, handler: F) -> Self
    where
        F: Fn(&serde_json::Value) -> Option<serde_json::Value> + Send + Sync + 'static,
    {
        self.handlers.push(Arc::new(handler));
        self
    }

    /// 按 MsgType 过滤后追加（对齐 addMessageListener）
    pub fn on_message_type<F>(self, message_type: &str, handler: F) -> Self
    where
        F: Fn(&serde_json::Value) -> Option<serde_json::Value> + Send + Sync + 'static,
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

    /// 按 Event 过滤后追加（对齐 addEventListener）
    pub fn on_event<F>(self, event: &str, handler: F) -> Self
    where
        F: Fn(&serde_json::Value) -> Option<serde_json::Value> + Send + Sync + 'static,
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

    /// 处理消息链，返回第一个非 None 的处理器结果
    pub fn handle(&self, message: &serde_json::Value) -> Option<serde_json::Value> {
        for handler in &self.handlers {
            if let Some(response) = handler(message) {
                return Some(response);
            }
        }

        None
    }

    /// 完整回调流程，对齐 Server::serve
    /// - echostr 校验请求直接回显
    /// - 加密消息解密后交给处理器链
    /// - 处理器结果转成（可加密的）回复
    pub fn serve(
        &self,
        query: &HashMap<String, String>,
        body: &str,
    ) -> Result<ServerResponse> {
        if let Some(echostr) = query.get("echostr").filter(|value| !value.is_empty()) {
            let echostr = echostr.clone();
            self.verify_url(query)?;

            return Ok(ServerResponse::ok(&echostr));
        }

        let message = self.parse(query, body)?;
        let response = self.handle(&message);

        self.transform_to_reply(response, &message)
    }

    /// 处理器结果 → 回复（重写 To/From、补 CreateTime、按需加密）
    pub fn transform_to_reply(
        &self,
        response: Option<serde_json::Value>,
        message: &serde_json::Value,
    ) -> Result<ServerResponse> {
        let response = match response {
            Some(response) => response,
            None => return Ok(ServerResponse::ok("success")),
        };

        let mut attributes = normalize_reply(&response)?;

        attributes.insert(
            "ToUserName".to_owned(),
            serde_json::Value::String(str_field(message, "FromUserName").unwrap_or_default()),
        );
        attributes.insert(
            "FromUserName".to_owned(),
            serde_json::Value::String(str_field(message, "ToUserName").unwrap_or_default()),
        );
        attributes.insert(
            "CreateTime".to_owned(),
            serde_json::Value::from(crate::current_timestamp()),
        );

        let xml = xml::build(&serde_json::Value::Object(attributes));

        match &self.encryptor {
            Some(encryptor) => Ok(ServerResponse::xml(&encryptor.encrypt(
                &xml,
                None,
                None,
                &self.message_format,
            )?)),
            None => Ok(ServerResponse::xml(&xml)),
        }
    }

    /// 设置消息格式（xml/json），企业微信部分接口使用 json
    /// 注入外部 Encryptor（第三方平台代公众号/小程序时使用组件侧加解密器）
    pub fn with_encryptor(mut self, encryptor: crate::encryptor::Encryptor) -> Self {
        self.encryptor = Some(encryptor);
        self
    }

    pub fn with_format(mut self, format: &str) -> Self {
        self.message_format = format.to_owned();
        self
    }

    /// 获取 token
    pub fn get_token(&self) -> &str {
        &self.token
    }

    /// 是否配置了加密
    pub fn has_encryptor(&self) -> bool {
        self.encryptor.is_some()
    }

    /// 服务器接入验证：校验 signature 并返回 echostr
    ///
    /// 对齐 easywechat `getQueryValue($query, 'echostr')` 分支
    pub fn verify_url(&self, query: &HashMap<String, String>) -> Result<String> {
        let echostr = query.get("echostr").cloned().unwrap_or_default();
        if echostr.is_empty() {
            return Err(error! {code: -40001, msg: "Missing echostr."});
        }

        let signature_val = query.get("signature").cloned().unwrap_or_default();
        let timestamp = query.get("timestamp").cloned().unwrap_or_default();
        let nonce = query.get("nonce").cloned().unwrap_or_default();

        if !signature::verify_signature(&self.token, &timestamp, &nonce, &signature_val) {
            // 安全模式下接入验证可以使用 msg_signature
            let msg_signature = query.get("msg_signature").cloned().unwrap_or_default();
            if msg_signature.is_empty()
                || !signature::verify_msg_signature(
                    &self.token,
                    &timestamp,
                    &nonce,
                    &echostr,
                    &msg_signature,
                )
            {
                return Err(error! {code: -40001, msg: "Invalid signature."});
            }
        }

        Ok(echostr)
    }

    /// 判断是否为加密请求
    pub fn is_encrypted(&self, query: &HashMap<String, String>, body: &str) -> bool {
        if !query.contains_key("msg_signature") {
            return false;
        }
        body.contains("</Encrypt>") || body.contains("</encrypt>")
    }

    /// 解析请求消息，返回 JSON 结构
    ///
    /// - 明文模式：校验 signature 后直接解析
    /// - 安全模式：校验 msg_signature、解密后再解析
    pub fn parse(&self, query: &HashMap<String, String>, body: &str) -> Result<serde_json::Value> {
        if self.is_encrypted(query, body) {
            let encryptor = match &self.encryptor {
                Some(v) => v,
                None => {
                    return Err(error! {code: -40001, msg: "Encrypted message but no aes_key configured."})
                }
            };

            let plain = match self.message_format.as_str() {
                "json" => {
                    let data: serde_json::Value = serde_json::from_str(body).map_err(|e| {
                        error! {code: -40002, msg: format!("Parse JSON failed: {:?}", e)}
                    })?;
                    let ciphertext = data["encrypt"].as_str().unwrap_or_default();
                    let timestamp = query
                        .get("timestamp")
                        .cloned()
                        .or_else(|| data["timestamp"].as_str().map(|s| s.to_owned()))
                        .unwrap_or_default();
                    let nonce = query
                        .get("nonce")
                        .cloned()
                        .or_else(|| data["nonce"].as_str().map(|s| s.to_owned()))
                        .unwrap_or_default();
                    let msg_signature = query
                        .get("msg_signature")
                        .cloned()
                        .or_else(|| data["msgsignature"].as_str().map(|s| s.to_owned()))
                        .unwrap_or_default();
                    encryptor.decrypt(ciphertext, &msg_signature, &nonce, &timestamp)?
                }
                _ => encryptor.decrypt_xml(body, query)?,
            };

            return self.parse_plain(&plain);
        }

        if self.require_encryption {
            return Err(error! {code: -40001, msg: "Encrypted message is required, plaintext message rejected."});
        }

        let signature_val = query.get("signature").cloned().unwrap_or_default();
        let timestamp = query.get("timestamp").cloned().unwrap_or_default();
        let nonce = query.get("nonce").cloned().unwrap_or_default();

        // 对齐 PHP Kernel\Traits\DecryptMessage::validatePlainRequest：签名必须存在且必须校验通过
        if signature_val.is_empty() {
            return Err(error! {code: -40001, msg: "Request signature must not be empty."});
        }

        if !signature::verify_signature(&self.token, &timestamp, &nonce, &signature_val) {
            return Err(error! {code: -40001, msg: "Invalid signature."});
        }

        self.parse_plain(body)
    }

    fn parse_plain(&self, body: &str) -> Result<serde_json::Value> {
        match self.message_format.as_str() {
            "json" => serde_json::from_str(body)
                .map_err(|e| error! {code: -40002, msg: format!("Parse JSON failed: {:?}", e)}),
            _ => xml::to_json(body),
        }
    }

    /// 加密回复内容（安全模式）
    pub fn encrypt_reply(
        &self,
        plain_xml: &str,
        nonce: Option<&str>,
        timestamp: Option<u64>,
    ) -> Result<String> {
        match &self.encryptor {
            Some(enc) => enc.encrypt(plain_xml, nonce, timestamp, &self.message_format),
            None => Ok(plain_xml.to_owned()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn verify_url_echoes_echostr() {
        let server = Server::new("wx1", "token", None, false).unwrap();
        let timestamp = "1587121403";
        let nonce = "1661748508";
        let sign = signature::create_signature(&["token", timestamp, nonce]);
        let q = query(&[
            ("echostr", "hello"),
            ("timestamp", timestamp),
            ("nonce", nonce),
            ("signature", &sign),
        ]);
        assert_eq!(server.verify_url(&q).unwrap(), "hello");
    }

    #[test]
    fn verify_url_rejects_bad_signature() {
        let server = Server::new("wx1", "token", None, false).unwrap();
        let q = query(&[
            ("echostr", "hello"),
            ("timestamp", "1"),
            ("nonce", "2"),
            ("signature", "bad"),
        ]);
        assert!(server.verify_url(&q).is_err());
    }

    #[test]
    fn parse_rejects_plaintext_when_encryption_required() {
        // 对齐 PHP：require_encryption 时明文请求必须被拒绝
        // 32 字节 AES key 的 base64（去掉 padding 走 Encryptor 的补齐逻辑）
        let key = base64::encode(&[7u8; 32]).trim_end_matches('=').to_owned();
        let server = Server::new("wx1", "token", Some(&key), true).unwrap();
        let body = "<xml><ToUserName><![CDATA[to]]></ToUserName><FromUserName><![CDATA[from]]></FromUserName><MsgType><![CDATA[text]]></MsgType><Content><![CDATA[hi]]></Content></xml>";

        let err = server.parse(&HashMap::new(), body).unwrap_err();

        assert!(format!("{}", err).contains("plaintext message rejected"));
    }

    #[test]
    fn parse_plain_message() {
        let server = Server::new("wx1", "token", None, false).unwrap();
        let timestamp = "1587121403";
        let nonce = "1661748508";
        let sign = signature::create_signature(&["token", timestamp, nonce]);
        let q = query(&[
            ("timestamp", timestamp),
            ("nonce", nonce),
            ("signature", &sign),
        ]);
        let body = "<xml><MsgType><![CDATA[text]]></MsgType><Content><![CDATA[hi]]></Content></xml>";
        let msg = server.parse(&q, body).unwrap();
        assert_eq!(msg["MsgType"], "text");
        assert_eq!(msg["Content"], "hi");
    }

    #[test]
    fn parse_rejects_plain_request_without_signature() {
        // 对齐 PHP DecryptMessage::validatePlainRequest：签名不能为空
        let server = Server::new("wx1", "token", None, false).unwrap();
        let err = server
            .parse(
                &query(&[("timestamp", "1587121403"), ("nonce", "1661748508")]),
                "<xml><MsgType><![CDATA[text]]></MsgType></xml>",
            )
            .unwrap_err();

        assert!(format!("{}", err).contains("Request signature must not be empty."));
    }

    #[test]
    fn parse_encrypted_message() {
        let aes_key = "kdjCGGJKSRjjhESfPO5lTSWtYS0v5pQX47skCkZczio";
        let server = Server::new("wxce775970ff046a47", "shaipe", Some(aes_key), true).unwrap();
        let enc = Encryptor::new("wxce775970ff046a47", "shaipe", aes_key, None).unwrap();

        let body = "<xml><MsgType><![CDATA[text]]></MsgType><Content><![CDATA[hi]]></Content></xml>";
        let encrypted = enc
            .encrypt_as_array(body, Some("1234567890"), Some(1587121403))
            .unwrap();
        let xml = format!(
            "<xml><Encrypt><![CDATA[{}]]></Encrypt><MsgSignature><![CDATA[{}]]></MsgSignature>\
             <TimeStamp>1587121403</TimeStamp><Nonce><![CDATA[1234567890]]></Nonce></xml>",
            encrypted.ciphertext, encrypted.signature
        );
        let q = query(&[
            ("timestamp", "1587121403"),
            ("nonce", "1234567890"),
            ("msg_signature", &encrypted.signature),
        ]);
        let msg = server.parse(&q, &xml).unwrap();
        assert_eq!(msg["Content"], "hi");
    }
}

/// 取字符串字段（Missing/非字符串返回 None）
pub(crate) fn str_field(message: &serde_json::Value, key: &str) -> Option<String> {
    message.get(key).and_then(|value| match value {
        serde_json::Value::String(text) => Some(text.clone()),
        serde_json::Value::Number(number) => Some(number.to_string()),
        _ => None,
    })
}

/// 归一化处理器返回值，对齐 RespondXmlMessage::normalizeResponse
pub fn normalize_reply(
    response: &serde_json::Value,
) -> Result<serde_json::Map<String, serde_json::Value>> {
    match response {
        serde_json::Value::Object(map) => {
            if !map.contains_key("MsgType") {
                return Err(error! {code: -1, msg: "MsgType cannot be empty."});
            }

            Ok(map.clone())
        }
        serde_json::Value::String(text) => {
            let mut map = serde_json::Map::new();
            map.insert("MsgType".to_owned(), serde_json::Value::from("text"));
            map.insert("Content".to_owned(), serde_json::Value::from(text.clone()));

            Ok(map)
        }
        serde_json::Value::Number(number) => {
            let mut map = serde_json::Map::new();
            map.insert("MsgType".to_owned(), serde_json::Value::from("text"));
            map.insert("Content".to_owned(), serde_json::Value::from(number.to_string()));

            Ok(map)
        }
        other => Err(error! {
            code: -1,
            msg: format!("Invalid Response type {:?}.", other),
        }),
    }
}

/// 服务端响应，对齐 EasyWeChat\Kernel\ServerResponse
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerResponse {
    status: u16,
    body: String,
    content_type: String,
}

impl ServerResponse {
    /// 自定义响应
    pub fn new(status: u16, body: &str, content_type: &str) -> ServerResponse {
        ServerResponse {
            status,
            body: body.to_owned(),
            content_type: content_type.to_owned(),
        }
    }

    /// 200 文本响应
    pub fn ok(body: &str) -> ServerResponse {
        ServerResponse::new(200, body, "text/plain")
    }

    /// 200 XML 响应
    pub fn xml(body: &str) -> ServerResponse {
        ServerResponse::new(200, body, "application/xml")
    }

    /// 200 JSON 响应
    pub fn json(body: &str) -> ServerResponse {
        ServerResponse::new(200, body, "application/json")
    }

    /// 状态码
    pub fn status(&self) -> u16 {
        self.status
    }

    /// 响应体
    pub fn body(&self) -> &str {
        &self.body
    }

    /// content-type
    pub fn content_type(&self) -> &str {
        &self.content_type
    }
}

#[cfg(test)]
mod response_tests {
    use super::*;

    #[test]
    fn builds_responses() {
        let ok = ServerResponse::ok("SUCCESS");
        assert_eq!(ok.status(), 200);
        assert_eq!(ok.body(), "SUCCESS");
        assert_eq!(ok.content_type(), "text/plain");

        assert_eq!(ServerResponse::xml("<xml/>").content_type(), "application/xml");
        assert_eq!(
            ServerResponse::json("{}").content_type(),
            "application/json"
        );
    }

    #[test]
    fn transform_reply_keeps_arbitrary_attributes() {
        // 对齐 RespondXmlMessage：处理器返回数组时保留全部字段（news 图文消息）
        let server = Server::new("wxapp", "token", None, false).unwrap();
        let message = serde_json::json!({
            "ToUserName": "gh_account",
            "FromUserName": "openid_1",
            "MsgType": "text",
            "Content": "hi"
        });
        let reply = serde_json::json!({
            "MsgType": "news",
            "ArticleCount": 1,
            "Articles": [{
                "Title": "标题",
                "Description": "描述",
                "Url": "https://example.com/a",
                "PicUrl": "https://example.com/a.png"
            }]
        });

        let response = server.transform_to_reply(Some(reply), &message).unwrap();
        let body = response.body();

        assert!(body.contains("<MsgType>news</MsgType>") || body.contains("<MsgType><![CDATA[news]]></MsgType>"));
        assert!(body.contains("<ArticleCount>1</ArticleCount>"));
        assert!(body.contains("标题"));
        assert!(body.contains("<ToUserName>openid_1</ToUserName>") || body.contains("<ToUserName><![CDATA[openid_1]]></ToUserName>"));
        assert!(body.contains("gh_account"));
    }

    #[test]
    fn transform_reply_rejects_object_without_msg_type() {
        let server = Server::new("wxapp", "token", None, false).unwrap();
        let message = serde_json::json!({"ToUserName": "a", "FromUserName": "b"});
        let err = server
            .transform_to_reply(Some(serde_json::json!({"Content": "no type"})), &message)
            .unwrap_err();

        assert!(format!("{:?}", err).contains("MsgType"));
    }
}
