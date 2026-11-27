//! copyright © ecdata.cn 2021 - present
//! XML 与 JSON 互转，对齐 EasyWeChat\Kernel\Support\Xml
//! created by shaipe 20210228
//!
//! 微信开放平台/公众号/企业微信的消息都是 XML，这里提供与 PHP 版本一致的
//! `Xml::parse` / `Xml::build` 语义：单层节点转 map，同名单节点出现多次时转为数组。

use crate::WechatResult as Result;
use sxd_document::dom::{ChildOfElement, Element};
use sxd_document::parser;

/// 解析 XML 为 JSON 对象
///
/// ```
/// use wechat_sdk::xml::to_json;
/// let v = to_json("<xml><a>1</a><b>x</b></xml>").unwrap();
/// assert_eq!(v["a"], "1");
/// ```
pub fn to_json<T: AsRef<str>>(xml: T) -> Result<serde_json::Value> {
    let package = parser::parse(xml.as_ref())
        .map_err(|e| error! {code: -40002, msg: format!("Parse XML failed: {:?}", e)})?;
    let doc = package.as_document();
    let root = doc.root();

    for child in root.children() {
        if let sxd_document::dom::ChildOfRoot::Element(elem) = child {
            return Ok(serde_json::Value::Object(element_to_object(elem)));
        }
    }

    Err(error! {code: -40002, msg: "Parse XML failed: no root element"})
}

fn element_to_object(elem: Element) -> serde_json::Map<String, serde_json::Value> {
    let mut map = serde_json::Map::new();

    for child in elem.children() {
        if let ChildOfElement::Element(child_elem) = child {
            let name = child_elem.name().local_part().to_owned();
            let value = element_to_value(child_elem);
            match map.remove(&name) {
                None => {
                    map.insert(name, value);
                }
                Some(serde_json::Value::Array(mut arr)) => {
                    arr.push(value);
                    map.insert(name, serde_json::Value::Array(arr));
                }
                Some(existing) => {
                    map.insert(
                        name,
                        serde_json::Value::Array(vec![existing, value]),
                    );
                }
            }
        }
    }

    map
}

fn element_to_value(elem: Element) -> serde_json::Value {
    let obj = element_to_object(elem);
    if obj.is_empty() {
        serde_json::Value::String(text_of(elem))
    } else {
        serde_json::Value::Object(obj)
    }
}

fn text_of(elem: Element) -> String {
    let mut text = String::new();
    for child in elem.children() {
        if let ChildOfElement::Text(t) = child {
            text.push_str(t.text());
        }
    }
    text.trim().to_owned()
}

/// XML 转义
pub fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// 把 JSON 对象构建为 XML（字符串统一使用 CDATA，与微信示例一致）
pub fn build(value: &serde_json::Value) -> String {
    let mut out = String::from("<xml>");
    if let Some(map) = value.as_object() {
        for (k, v) in map {
            out.push_str(&build_node(k, v));
        }
    }
    out.push_str("</xml>");
    out
}

fn build_node(key: &str, value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Array(arr) => arr
            .iter()
            .map(|v| build_node(key, v))
            .collect::<Vec<String>>()
            .join(""),
        serde_json::Value::Object(_) => format!("<{k}>{v}</{k}>", k = escape(key), v = build(value)),
        serde_json::Value::String(s) => format!(
            "<{k}><![CDATA[{v}]]></{k}>",
            k = escape(key),
            v = cdata(s)
        ),
        serde_json::Value::Null => format!("<{k}></{k}>", k = escape(key)),
        other => format!("<{k}>{v}</{k}>", k = escape(key), v = other),
    }
}

/// CDATA 内容需要拆分 `]]>`
fn cdata(value: &str) -> String {
    value.replace("]]>", "]]]]><![CDATA[>")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_text_message() {
        let xml = "<xml>
            <ToUserName><![CDATA[toUser]]></ToUserName>
            <FromUserName><![CDATA[fromUser]]></FromUserName>
            <CreateTime>1348831860</CreateTime>
            <MsgType><![CDATA[text]]></MsgType>
            <Content><![CDATA[this is a test]]></Content>
            <MsgId>1234567890123456</MsgId>
        </xml>";
        let v = to_json(xml).unwrap();
        assert_eq!(v["ToUserName"], "toUser");
        assert_eq!(v["MsgType"], "text");
        assert_eq!(v["Content"], "this is a test");
        assert_eq!(v["CreateTime"], "1348831860");
    }

    #[test]
    fn parse_nested_and_repeated_nodes() {
        let xml = "<xml><list><item>1</item><item>2</item></list></xml>";
        let v = to_json(xml).unwrap();
        assert_eq!(v["list"]["item"][0], "1");
        assert_eq!(v["list"]["item"][1], "2");
    }

    #[test]
    fn build_roundtrip() {
        let v = serde_json::json!({
            "ToUserName": "toUser",
            "Content": "hello",
        });
        let xml = build(&v);
        assert!(xml.contains("<ToUserName><![CDATA[toUser]]></ToUserName>"));
        let parsed = to_json(&xml).unwrap();
        assert_eq!(parsed["Content"], "hello");
    }

    #[test]
    fn build_handles_numbers() {
        let v = serde_json::json!({ "CreateTime": 1348831860 });
        assert_eq!(
            build(&v),
            "<xml><CreateTime>1348831860</CreateTime></xml>"
        );
    }
}
