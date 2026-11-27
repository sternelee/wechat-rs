//! copyright © ecdata.cn 2021 - present
//! 字符串工具，对齐 EasyWeChat Kernel\Support\Str
//! created by shaipe 20210228

use rand::prelude::*;

const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";

/// 生成指定长度的随机字符串
pub fn random(length: usize) -> String {
    let mut rng = rand::thread_rng();
    (0..length)
        .map(|_| {
            let idx = rng.gen_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}

/// 生成16位随机串（微信 nonce_str/nonceStr 常用长度）
pub fn random_nonce() -> String {
    random(16)
}

/// 驼峰转下划线
pub fn snake_case(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 4);
    for (i, c) in value.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.extend(c.to_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// 下划线转驼峰
pub fn camel_case(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut upper = false;
    for c in value.chars() {
        if c == '_' || c == '-' {
            upper = true;
        } else if upper {
            out.extend(c.to_uppercase());
            upper = false;
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_string_works() {
        let s = random(32);
        assert_eq!(s.len(), 32);
        assert!(s.chars().all(|c| c.is_ascii_alphanumeric()));
    }

    #[test]
    fn case_convert_works() {
        assert_eq!(snake_case("AppId"), "app_id");
        assert_eq!(camel_case("app_id"), "appId");
    }
}
