//! copyright © ecdata.cn 2021 - present
//! 微信签名工具，对齐 EasyWeChat Kernel\Encryptor::createSignature 及 JsApiTicket::configSignature
//! created by shaipe 20210228

use crypto::digest::Digest;
use crypto::sha1::Sha1;

/// 计算 SHA1 十六进制摘要
pub fn sha1(data: &str) -> String {
    let mut hasher = Sha1::new();
    hasher.input_str(data);
    hasher.result_str()
}

/// 微信回调签名：把 token、timestamp、nonce 等参数按字典序排序后拼接再做 sha1
///
/// 对齐 EasyWeChat: `sort($attributes, SORT_STRING); sha1(implode('', $attributes))`
pub fn create_signature<S: AsRef<str>>(attributes: &[S]) -> String {
    let mut items: Vec<String> = attributes.iter().map(|s| s.as_ref().to_owned()).collect();
    items.sort();
    sha1(&items.concat())
}

/// 校验微信服务器（公众号/企业微信/开放平台）签名
pub fn verify_signature(token: &str, timestamp: &str, nonce: &str, signature: &str) -> bool {
    let real = create_signature(&[token, timestamp, nonce]);
    constant_time_eq(&real, signature)
}

/// 校验加密消息签名，额外拼接 Encrypt 密文
pub fn verify_msg_signature(
    token: &str,
    timestamp: &str,
    nonce: &str,
    encrypt: &str,
    msg_signature: &str,
) -> bool {
    let real = create_signature(&[token, timestamp, nonce, encrypt]);
    constant_time_eq(&real, msg_signature)
}

/// JS-SDK 签名
///
/// 对齐 EasyWeChat:
/// `sha1(sprintf('jsapi_ticket=%s&noncestr=%s&timestamp=%s&url=%s', ...))`
pub fn jsapi_signature(ticket: &str, nonce: &str, timestamp: u64, url: &str) -> String {
    sha1(&format!(
        "jsapi_ticket={}&noncestr={}&timestamp={}&url={}",
        ticket, nonce, timestamp, url
    ))
}

/// 校验 JS-SDK 签名
pub fn verify_jsapi_signature(
    ticket: &str,
    nonce: &str,
    timestamp: u64,
    url: &str,
    signature: &str,
) -> bool {
    constant_time_eq(&jsapi_signature(ticket, nonce, timestamp, url), signature)
}

/// 签名比对（避免时序攻击）
pub fn constant_time_eq(a: &str, b: &str) -> bool {
    let a = a.as_bytes();
    let b = b.as_bytes();
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for i in 0..a.len() {
        diff |= a[i] ^ b[i];
    }
    diff == 0
}

/// 把 HashMap 参数按 key 字典序拼接成 `k=v&k=v`（微信支付签名用）
pub fn query_string(params: &[(String, String)]) -> String {
    let mut items: Vec<(String, String)> = params.to_vec();
    items.sort_by(|a, b| a.0.cmp(&b.0));
    items
        .iter()
        .filter(|(_, v)| !v.is_empty())
        .map(|(k, v)| format!("{}={}", k, v))
        .collect::<Vec<String>>()
        .join("&")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_signature_sorts_attributes() {
        // 与 easywechat 的排序行为保持一致
        let a = create_signature(&["nonce", "token", "1587121403"]);
        let b = create_signature(&["token", "1587121403", "nonce"]);
        assert_eq!(a, b);
    }

    #[test]
    fn verify_signature_works() {
        let token = "shaipe";
        let timestamp = "1587121403";
        let nonce = "1661748508";
        let sign = create_signature(&[token, timestamp, nonce]);
        assert!(verify_signature(token, timestamp, nonce, &sign));
        assert!(!verify_signature(token, timestamp, nonce, "invalid"));
    }

    #[test]
    fn jsapi_signature_matches_php_format() {
        let sign = jsapi_signature("ticket", "nonce", 1587121403, "https://example.com/a");
        let expect = sha1("jsapi_ticket=ticket&noncestr=nonce&timestamp=1587121403&url=https://example.com/a");
        assert_eq!(sign, expect);
    }
}
