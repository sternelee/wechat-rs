//! copyright © ecdata.cn 2021 - present
//! 微信系列对接处理公用工具类
//! created by shaipe 20210228

/// 字义微信结果类型
pub type WechatResult<T> = Result<T, WechatError>;

#[macro_use]
pub mod macros;

#[macro_use]
extern crate lazy_static;

// 错误信息处理定义
mod errors;
pub use errors::{ErrorKind, WechatError};

// 加解密处理
mod wxcrypto;
pub use wxcrypto::{
    aes128_cbc_decrypt, aes128_cbc_encrypt, aes256_cbc_decrypt, aes256_cbc_encrypt, WeChatCrypto,
};

// 公共AccessToken管理
mod token;
pub use authorizer::ComponentOAuth;
pub use token::{AccessToken, Token};

// JsApiTicket 管理
mod ticket;
pub use ticket::JsApiTicket;

// 字符串与签名工具
pub mod signature;
pub mod strutil;

// 消息加解密
pub mod encryptor;
pub use encryptor::{EncryptedMessage, Encryptor};

// XML <-> JSON
pub mod xml;

// 微信消息服务器
pub mod server;
pub use server::{Server, ServerResponse};

// 导出常量配置
pub mod constant;

#[cfg(any(feature = "req_async", feature = "axum"))]
mod reqw_client;
#[cfg(any(feature = "req_async", feature = "axum"))]
pub use reqw_client::Client;

pub mod authorizer;
pub mod cache;
pub mod config;
pub use config::*;

pub mod aes_crypt;
pub mod xmlutil;
pub use aes_crypt::AesCrypt;

pub mod form;
pub mod url_encoding;
pub use url_encoding::get_url_encode;
/// 写入文件到日志
// #[allow(dead_code)]
pub fn write_to_file(conf_path: &str, content: String) -> WechatResult<bool> {
    use std::fs::OpenOptions;
    use std::io::prelude::*;

    // 以读,写,创建,追加的方式打开文件
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        // .append(true)
        .open(conf_path);

    // 向文件中写入内容
    match file {
        Ok(mut stream) => match stream.write_all(content.as_bytes()) {
            Ok(_) => Ok(true),
            Err(err) => Err(error!("{:?}", err)),
        },
        Err(err) => Err(error!("{:?}", err)),
    }
}

/// 获取当前时间戮
pub fn current_timestamp() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as u64
}

///
#[inline]
pub fn json_decode(data: &str) -> WechatResult<serde_json::Value> {
    let obj: serde_json::Value = match serde_json::from_str(data) {
        Ok(decoded) => decoded,
        Err(ref e) => {
            return Err(error! {
                code: -3,
                msg: format!("Json decode error: {}", e),
            });
        }
    };
    // 微信接口也会返回数组/标量（如批量接口），非对象时直接原样返回，避免 panic
    let dic = match obj.as_object() {
        Some(dic) => dic,
        None => return Ok(obj),
    };
    let code = if dic.contains_key("errcode") {
        "errcode"
    } else {
        "code"
    };

    let code = match obj[code].as_i64() {
        Some(v) => v,
        None => 0,
    };
    if code != 0 {
        let msg: String = if dic.contains_key("msg") {
            obj["msg"].to_string()
        } else {
            obj["errmsg"].to_string()
        };
        return Err(error! {
            code: code as i32,
            msg: msg,
        });
    }
    Ok(obj)
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }

    #[test]
    fn json_decode_handles_non_object_payloads() {
        // 微信批量接口会返回数组，不应 panic
        let value = super::json_decode(r#"[{"openid":"o1"}]"#).unwrap();
        assert!(value.is_array());

        let value = super::json_decode(r#""plain""#).unwrap();
        assert_eq!(value.as_str(), Some("plain"));
    }

    #[test]
    fn json_decode_raises_on_error_code() {
        let err = super::json_decode(r#"{"errcode":40001,"errmsg":"invalid credential"}"#).unwrap_err();
        match err.kind {
            super::ErrorKind::Custom { code, .. } => assert_eq!(code, 40001),
            other => panic!("unexpected kind: {:?}", other),
        }
    }
}
