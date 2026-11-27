//! copyright © ecdata.cn 2021 - present
//! 支付回调服务器，对齐 EasyWeChat\Pay\Server
//!
//! - JSON 回调：`resource.ciphertext/nonce/associated_data` 用 AES-256-GCM 解密
//! - XML 回调：`req_info` 用 AES-256-ECB(md5(v2 密钥)) 解密，`event_ciphertext` 用 AES-256-GCM 解密

use crate::aes::{aes_ecb_decrypt, aes_gcm_decrypt};
use crate::merchant::Merchant;
use crate::message::Message;
use serde_json::Value;
use wechat_sdk::{xml, WechatResult as Result};

/// 默认成功响应，对齐 Server::serve()
pub const DEFAULT_REPLY: &str = r#"{"code":"SUCCESS","message":"成功"}"#;

/// 处理器
pub type Handler = Box<dyn Fn(&Message) -> Option<String> + Send + Sync>;

/// 审批条件
pub type Condition = Box<dyn Fn(&Message) -> bool + Send + Sync>;

/// 支付回调服务器
pub struct Server {
    merchant: Merchant,
    handlers: Vec<(Condition, Handler)>,
}

impl std::fmt::Debug for Server {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Server")
            .field("handlers", &self.handlers.len())
            .finish()
    }
}

impl Server {
    /// 创建回调服务器
    pub fn new(merchant: Merchant) -> Server {
        Server {
            merchant,
            handlers: Vec::new(),
        }
    }

    /// 商户信息
    pub fn merchant(&self) -> &Merchant {
        &self.merchant
    }

    /// 注册处理器，对齐 with()
    pub fn with<F, C>(&mut self, condition: C, handler: F) -> &mut Self
    where
        C: Fn(&Message) -> bool + Send + Sync + 'static,
        F: Fn(&Message) -> Option<String> + Send + Sync + 'static,
    {
        self.handlers.push((Box::new(condition), Box::new(handler)));
        self
    }

    /// 支付成功处理器，对齐 handlePaid
    pub fn handle_paid<F>(&mut self, handler: F) -> &mut Self
    where
        F: Fn(&Message) -> Option<String> + Send + Sync + 'static,
    {
        self.with(
            |message| {
                message.event_type().ok().as_deref() == Some("TRANSACTION.SUCCESS")
                    && message.trade_state() == Some("SUCCESS")
            },
            handler,
        )
    }

    /// 退款处理器，对齐 handleRefunded
    pub fn handle_refunded<F>(&mut self, handler: F) -> &mut Self
    where
        F: Fn(&Message) -> Option<String> + Send + Sync + 'static,
    {
        self.with(
            |message| {
                matches!(
                    message.event_type().ok().as_deref(),
                    Some("REFUND.SUCCESS") | Some("REFUND.ABNORMAL") | Some("REFUND.CLOSED")
                )
            },
            handler,
        )
    }

    /// 解析回调消息，对齐 getRequestMessage
    pub fn request_message(&self, content_type: &str, body: &str) -> Result<Message> {
        let is_xml = (content_type.contains("text/xml") || content_type.contains("application/xml"))
            && body.trim_start().starts_with("<xml");

        if is_xml {
            let attributes = self.decode_xml_message(body)?;
            return Ok(Message::new(attributes.clone(), attributes, body));
        }

        self.decode_json_message(body)
    }

    /// 解析 XML 回调，对齐 decodeXmlMessage
    fn decode_xml_message(&self, contents: &str) -> Result<Value> {
        let mut attributes = xml::to_json(contents)?;

        if !attributes.is_object() {
            return Err(error! {code: -1, msg: "Invalid request body."});
        }

        if let Some(req_info) = attributes["req_info"].as_str().filter(|v| !v.is_empty()) {
            let key = self.merchant.require_v2_secret_key()?;
            let plain = aes_ecb_decrypt(req_info, key)?;
            let inner = xml::to_json(&plain)?;
            merge_object(&mut attributes, inner);
        }

        let event_ciphertext = attributes["event_ciphertext"]
            .as_str()
            .filter(|v| !v.is_empty())
            .map(|v| v.to_owned());
        let event_nonce = attributes["event_nonce"].as_str().map(|v| v.to_owned());
        let event_aad = attributes["event_associated_data"].as_str().map(|v| v.to_owned());

        if let (Some(ciphertext), Some(nonce), Some(aad)) = (event_ciphertext, event_nonce, event_aad)
        {
            let plain = aes_gcm_decrypt(
                &ciphertext,
                &self.merchant.secret_key,
                &nonce,
                &aad,
            )?;
            let inner = xml::to_json(&plain)?;
            merge_object(&mut attributes, inner);
        }

        Ok(attributes)
    }

    /// 解析 JSON 回调，对齐 decodeJsonMessage
    fn decode_json_message(&self, contents: &str) -> Result<Message> {
        let origin: Value = serde_json::from_str(contents).map_err(|e| {
            error! {
                code: -1,
                msg: format!("Invalid request body: {}", e),
            }
        })?;

        if !origin["resource"].is_object() {
            return Err(error! {code: -1, msg: "Invalid request body."});
        }

        let resource = &origin["resource"];
        let ciphertext = resource["ciphertext"].as_str().unwrap_or_default();
        if ciphertext.is_empty() {
            return Err(error! {code: -1, msg: "Invalid request."});
        }

        let nonce = resource["nonce"]
            .as_str()
            .ok_or_else(|| error! {code: -1, msg: "Invalid request resource."})?;
        let associated_data = resource["associated_data"]
            .as_str()
            .ok_or_else(|| error! {code: -1, msg: "Invalid request resource."})?;

        let plain = aes_gcm_decrypt(ciphertext, &self.merchant.secret_key, nonce, associated_data)?;
        let attributes: Value = serde_json::from_str(&plain).map_err(|e| {
            error! {
                code: -1,
                msg: format!("Failed to decrypt request message: {}", e),
            }
        })?;

        Ok(Message::new(attributes, origin, contents))
    }

    /// 已解密消息，对齐 getDecryptedMessage
    pub fn decrypted_message(&self, content_type: &str, body: &str) -> Result<Message> {
        self.request_message(content_type, body)
    }

    /// 执行处理器链，返回第一个非空回复
    pub fn handle(&self, message: &Message) -> Option<String> {
        for (condition, handler) in &self.handlers {
            if condition(message) {
                if let Some(reply) = handler(message) {
                    return Some(reply);
                }
            }
        }

        None
    }

    /// 完整回调处理，对齐 serve()：成功返回成功 JSON，失败返回 500 错误 JSON
    pub fn serve(&self, content_type: &str, body: &str) -> String {
        match self.request_message(content_type, body) {
            Ok(message) => self
                .handle(&message)
                .unwrap_or_else(|| DEFAULT_REPLY.to_owned()),
            Err(e) => format!(
                r#"{{"code":"ERROR","message":"{}"}}"#,
                e.to_string().replace('"', "'")
            ),
        }
    }
}

/// 把 inner 对象的字段合并进 target
fn merge_object(target: &mut Value, inner: Value) {
    if let (Some(target), Some(inner)) = (target.as_object_mut(), inner.as_object()) {
        for (key, value) in inner {
            target.insert(key.clone(), value.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aes::aes_gcm_encrypt;

    const AES_KEY: &str = "0123456789abcdef0123456789abcdef";
    const NONCE: &str = "nonce-vector-1";

    fn merchant() -> Merchant {
        Merchant::new(
            "1900000109",
            "key",
            "cert",
            AES_KEY,
            Some("v2-secret-key".into()),
        )
    }

    #[test]
    fn serve_decodes_json_callback() {
        let plain = r#"{"out_trade_no":"T1","trade_state":"SUCCESS","event_type":"TRANSACTION.SUCCESS"}"#;
        let ciphertext = aes_gcm_encrypt(
            plain,
            AES_KEY,
            "nonce-1",
            "transaction",
        )
        .unwrap();

        let body = serde_json::json!({
            "id": "evt-1",
            "event_type": "TRANSACTION.SUCCESS",
            "resource_type": "encrypt-resource",
            "resource": {
                "algorithm": "AEAD_AES_256_GCM",
                "ciphertext": ciphertext,
                "nonce": "nonce-1",
                "associated_data": "transaction",
            }
        })
        .to_string();

        let mut server = Server::new(merchant());
        server.handle_paid(|message| {
            assert_eq!(message.out_trade_no(), Some("T1"));
            Some(r#"{"code":"SUCCESS","message":"OK"}"#.to_owned())
        });

        assert_eq!(
            server.serve("application/json", &body),
            r#"{"code":"SUCCESS","message":"OK"}"#
        );
    }

    #[test]
    fn serve_falls_back_to_default_reply() {
        let plain = r#"{"out_trade_no":"T2","trade_state":"NOTPAY"}"#;
        let ciphertext = aes_gcm_encrypt(plain, AES_KEY, "n", "").unwrap();

        let body = serde_json::json!({
            "event_type": "TRANSACTION.SUCCESS",
            "resource": {"ciphertext": ciphertext, "nonce": "n", "associated_data": ""}
        })
        .to_string();

        let server = Server::new(merchant());
        assert_eq!(server.serve("application/json", &body), DEFAULT_REPLY);
    }

    #[test]
    fn serve_reports_error_for_bad_body() {
        let server = Server::new(merchant());
        let reply = server.serve("application/json", "not-json");
        assert!(reply.contains(r#""code":"ERROR""#));
    }

    #[test]
    fn request_message_decodes_xml_req_info() {
        use crate::aes::{aes_ecb_encrypt, pkcs7_unpad};

        let v2_key = "v2-secret-key";
        let plain = "<xml><out_trade_no>T3</out_trade_no><return_code>SUCCESS</return_code></xml>";
        let ciphertext = aes_ecb_encrypt(plain, v2_key).unwrap();

        let body = format!("<xml><req_info>{}</req_info></xml>", ciphertext);
        let message = Server::new(merchant())
            .request_message("text/xml", &body)
            .unwrap();

        assert_eq!(message.get("out_trade_no").and_then(|v| v.as_str()), Some("T3"));
        assert_eq!(pkcs7_unpad(b"abc\x03\x03\x03"), b"abc");
    }

    #[test]
    fn request_message_decodes_xml_event_ciphertext() {
        let plain = "<xml><out_trade_no>T4</out_trade_no></xml>";
        let ciphertext = aes_gcm_encrypt(plain, AES_KEY, NONCE, "aad").unwrap();

        let body = String::from("<xml><event_ciphertext>")
            + &ciphertext
            + "</event_ciphertext><event_nonce>"
            + NONCE
            + "</event_nonce><event_associated_data>aad</event_associated_data></xml>";

        let message = Server::new(merchant())
            .request_message("application/xml", &body)
            .unwrap();

        assert_eq!(message.get("out_trade_no").and_then(|v| v.as_str()), Some("T4"));
    }
}
