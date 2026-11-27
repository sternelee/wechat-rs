//! copyright © ecdata.cn 2021 - present
//! 支付回调消息，对齐 EasyWeChat\Pay\Message

use serde_json::Value;
use wechat_sdk::WechatResult as Result;

/// 支付回调消息
#[derive(Debug, Clone)]
pub struct Message {
    /// 解密后的业务数据
    attributes: Value,
    /// 原始报文解析结果（JSON 回调含 event_type 等外层字段）
    origin: Value,
    /// 原始报文
    raw: String,
}

impl Message {
    pub fn new(attributes: Value, origin: Value, raw: impl Into<String>) -> Message {
        Message {
            attributes,
            origin,
            raw: raw.into(),
        }
    }

    /// 业务数据
    pub fn attributes(&self) -> &Value {
        &self.attributes
    }

    /// 原始报文解析结果
    pub fn origin(&self) -> &Value {
        &self.origin
    }

    /// 原始报文
    pub fn raw(&self) -> &str {
        &self.raw
    }

    /// 业务字段
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.attributes.get(key)
    }

    /// 事件类型，对齐 getEventType
    pub fn event_type(&self) -> Result<String> {
        self.origin["event_type"]
            .as_str()
            .or_else(|| self.attributes["event_type"].as_str())
            .map(|v| v.to_owned())
            .ok_or_else(|| error! {code: -1, msg: "Invalid event type."})
    }

    /// 交易状态
    pub fn trade_state(&self) -> Option<&str> {
        self.attributes["trade_state"].as_str()
    }

    /// 商户订单号
    pub fn out_trade_no(&self) -> Option<&str> {
        self.attributes["out_trade_no"].as_str()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_reads_event_type_and_trade_state() {
        let message = Message::new(
            serde_json::json!({"out_trade_no": "T1", "trade_state": "SUCCESS"}),
            serde_json::json!({"event_type": "TRANSACTION.SUCCESS"}),
            "{}",
        );

        assert_eq!(message.event_type().unwrap(), "TRANSACTION.SUCCESS");
        assert_eq!(message.trade_state(), Some("SUCCESS"));
        assert_eq!(message.out_trade_no(), Some("T1"));
    }

    #[test]
    fn message_requires_event_type() {
        let message = Message::new(serde_json::json!({}), serde_json::json!({}), "{}");
        assert!(message.event_type().is_err());
    }
}
