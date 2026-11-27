//! copyright © ecdata.cn 2021 - present
//! 支付客户端，对齐 EasyWeChat\Pay\Client
//!
//! - `/v3/`、`/hk/v3/`、`/global/v3/` 走 v3 签名（Authorization 头）
//! - 其余走 v2 签名（XML + sign 字段）

use crate::merchant::Merchant;
use crate::signature::{request_path, LegacySignature, Signature};
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;
use wechat_sdk::{form::Form, form::File, xml, Client as HttpClient, WechatResult as Result};

/// 支付接口域名
pub const PAY_API_DOMAIN: &str = "https://api.mch.weixin.qq.com";

/// v3 路径前缀
pub const V3_URI_PREFIXES: [&str; 3] = ["/v3/", "/hk/v3/", "/global/v3/"];

/// 判断是否 v3 请求，对齐 isV3Request
pub fn is_v3_path(path: &str) -> bool {
    let path = request_path(path);
    V3_URI_PREFIXES.iter().any(|prefix| path.starts_with(prefix))
}

/// 支付客户端
#[derive(Debug, Clone)]
pub struct Client {
    merchant: Merchant,
    http: HttpClient,
    /// 额外请求头，对齐 `withHeader()`
    extra_headers: Vec<(String, String)>,
    /// 平台证书序列号，对齐 `withSerialHeader()`
    platform_serial: Option<String>,
}

impl Client {
    /// 创建客户端
    pub fn new(merchant: Merchant) -> Client {
        Client {
            merchant,
            http: HttpClient::default(),
            extra_headers: Vec::new(),
            platform_serial: None,
        }
    }

    /// 附加请求头，对齐 `withHeader()`
    pub fn with_header<S: Into<String>>(mut self, name: S, value: S) -> Client {
        let name = name.into();
        self.extra_headers.retain(|(n, _)| n != &name);
        self.extra_headers.push((name, value.into()));
        self
    }

    /// 加 `Wechatpay-Serial` 头，对齐 `withSerialHeader()`
    ///
    /// 未指定序列号时取第一个平台证书；没有任何平台证书时抛错，
    /// 对齐 PHP 的 `InvalidConfigException('Missing platform certificate.')`。
    pub fn with_serial_header(mut self, serial: Option<&str>) -> Result<Client> {
        let serial = match serial {
            Some(serial) => serial.to_owned(),
            None => match self.merchant.first_platform_cert() {
                Some((serial, _)) => serial.clone(),
                None => {
                    return Err(error! {
                        code: -1,
                        msg: "Missing platform certificate.".to_owned(),
                    })
                }
            },
        };

        self.platform_serial = Some(serial);
        Ok(self)
    }

    /// 平台证书序列号
    pub fn platform_serial(&self) -> Option<&str> {
        self.platform_serial.as_deref()
    }

    /// 商户信息
    pub fn merchant(&self) -> &Merchant {
        &self.merchant
    }

    /// 底层 HTTP 客户端
    pub fn http(&self) -> &HttpClient {
        &self.http
    }

    /// 发起请求（自动判断 v2/v3），对齐 request()
    pub async fn request(&self, method: &str, path: &str, body: Option<&str>) -> Result<Value> {
        let url = if path.starts_with("https://") || path.starts_with("http://") {
            path.to_owned()
        } else {
            format!("{}{}", PAY_API_DOMAIN, path)
        };

        if is_v3_path(&url) {
            self.request_v3(method, &url, body).await
        } else {
            self.request_v2(method, &url, body).await
        }
    }

    /// v3 请求，自动加 Authorization
    async fn request_v3(&self, method: &str, url: &str, body: Option<&str>) -> Result<Value> {
        let body = body.unwrap_or_default();
        let headers = self.v3_headers(method, url, body)?;

        let text = self
            .http
            .request_raw(
                method,
                url,
                headers,
                if body.is_empty() {
                    None
                } else {
                    Some(body.to_owned())
                },
            )
            .await?;

        parse_v3_response(&text)
    }

    /// 组装 v3 请求头：签名 + 额外头 + `Wechatpay-Serial`
    fn v3_headers(&self, method: &str, url: &str, body: &str) -> Result<Vec<(String, String)>> {
        self.v3_headers_with_content_type(method, url, body, Some("application/json"))
    }

    /// 组装 v3 请求头，`content_type` 为 None 时不下发 Content-Type
    /// （multipart 上传由 `Form` 自己带 boundary，对齐 PHP `Form::toOptions()`）
    fn v3_headers_with_content_type(
        &self,
        method: &str,
        url: &str,
        body: &str,
        content_type: Option<&str>,
    ) -> Result<Vec<(String, String)>> {
        let authorization = Signature::new(&self.merchant).create_header(method, url, body)?;

        let mut headers = vec![
            ("Accept".to_owned(), "application/json".to_owned()),
            ("Authorization".to_owned(), authorization),
            ("User-Agent".to_owned(), "wechat-pay-rs".to_owned()),
        ];

        if let Some(content_type) = content_type {
            headers.insert(1, ("Content-Type".to_owned(), content_type.to_owned()));
        }

        if let Some(serial) = &self.platform_serial {
            headers.push(("Wechatpay-Serial".to_owned(), serial.clone()));
        }

        for (name, value) in &self.extra_headers {
            headers.retain(|(n, _)| n != name);
            headers.push((name.clone(), value.clone()));
        }

        Ok(headers)
    }

    /// 上传图片/视频等媒体文件（仅 v3），对齐 `uploadMedia()`
    ///
    /// `meta` 缺省时按 PHP 计算 `{filename, sha256}`；签名 body 使用 meta JSON，
    /// 而不是整个 multipart 实体。
    pub async fn upload_media(
        &self,
        uri: &str,
        contents: &[u8],
        meta: Option<&Value>,
        filename: Option<&str>,
    ) -> Result<Value> {
        let url = if uri.starts_with("https://") || uri.starts_with("http://") {
            uri.to_owned()
        } else {
            format!("{}{}", PAY_API_DOMAIN, uri)
        };

        let meta_json = match meta {
            Some(meta) => serde_json::to_string(meta).map_err(|e| {
                error! {
                    code: -1,
                    msg: format!("Encode meta failed: {}", e),
                }
            })?,
            None => serde_json::json!({
                "filename": filename.unwrap_or("file"),
                "sha256": sha256_hex(contents),
            })
            .to_string(),
        };

        let form = Form::new()
            .file(File::from_bytes("file", filename.unwrap_or("file"), contents.to_vec()))
            .content_part("meta", meta_json.clone(), "application/json");

        // 签名用 meta JSON 作为 body，对齐 PHP `$signatureOptions['body'] = $meta`；
        // Content-Type 交给 multipart（带 boundary），避免出现两个 Content-Type
        let headers = self.v3_headers_with_content_type("POST", &url, &meta_json, None)?;

        let text = self
            .http
            .post_multipart_with_headers(&url, &form, &headers)
            .await?;

        parse_v3_response(&text)
    }

    /// v2 请求：补签名后按 XML 发送
    async fn request_v2(&self, method: &str, url: &str, body: Option<&str>) -> Result<Value> {
        if method.eq_ignore_ascii_case("GET") {
            let text = self.http.get(url).await?;
            return parse_v2_response(&text);
        }

        let params: BTreeMap<String, String> = match body {
            Some(body) if !body.trim().is_empty() => serde_json::from_str(body).unwrap_or_default(),
            _ => BTreeMap::new(),
        };

        self.post_v2(&params_string_path(url), &params).await
    }

    /// v3 POST JSON，对齐 post()
    pub async fn post<T: Serialize + ?Sized>(&self, path: &str, body: &T) -> Result<Value> {
        let body = serde_json::to_string(body).map_err(|e| {
            error! {
                code: -1,
                msg: format!("Serialize request body failed: {}", e),
            }
        })?;

        self.request("POST", path, Some(&body)).await
    }

    /// v3 GET，对齐 get()；命中 v2 签名 GET 端点时先补签再请求
    pub async fn get(&self, path: &str, query: &[(&str, &str)]) -> Result<Value> {
        if is_v2_signed_get(path) {
            let params: BTreeMap<String, String> = query
                .iter()
                .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
                .collect();

            return self.get_v2(path, &params).await;
        }

        let url = if query.is_empty() {
            format!("{}{}", PAY_API_DOMAIN, path)
        } else {
            format!(
                "{}{}?{}",
                PAY_API_DOMAIN,
                path,
                query
                    .iter()
                    .map(|(key, value)| format!("{}={}", key, value))
                    .collect::<Vec<String>>()
                    .join("&")
            )
        };

        self.request("GET", &url, None).await
    }

    /// v2 GET（query 参数补 sign），对齐 Client::V2_URI_OVER_GETS 分支
    pub async fn get_v2(
        &self,
        path: &str,
        query: &BTreeMap<String, String>,
    ) -> Result<Value> {
        let signed = LegacySignature::new(&self.merchant).sign(query)?;
        let query_string = signed
            .iter()
            .map(|(key, value)| format!("{}={}", key, value))
            .collect::<Vec<String>>()
            .join("&");
        let url = if query_string.is_empty() {
            format!("{}{}", PAY_API_DOMAIN, path)
        } else {
            format!("{}{}?{}", PAY_API_DOMAIN, path, query_string)
        };

        let text = self.http.get(&url).await?;
        parse_v2_response(&text)
    }

    /// v2 POST（参数自动补 sign），对齐 request() 的 v2 分支
    pub async fn post_v2(&self, path: &str, params: &BTreeMap<String, String>) -> Result<Value> {
        let signed = LegacySignature::new(&self.merchant).sign(params)?;
        let value = serde_json::to_value(&signed).map_err(|e| {
            error! {
                code: -1,
                msg: format!("Serialize params failed: {}", e),
            }
        })?;
        let xml = xml::build(&value);

        self.post_v2_xml(path, &xml).await
    }

    /// v2 POST 原始 XML（已含 sign）
    pub async fn post_v2_xml(&self, path: &str, xml: &str) -> Result<Value> {
        let url = format!("{}{}", PAY_API_DOMAIN, path);
        let text = self.http.post_xml(&url, xml).await?;

        parse_v2_response(&text)
    }

    /// 以 XML 提交并直接用返回的原始文本（下载账单等）
    pub async fn post_v2_raw(&self, path: &str, params: &BTreeMap<String, String>) -> Result<String> {
        let signed = LegacySignature::new(&self.merchant).sign(params)?;
        let value = serde_json::to_value(&signed).map_err(|e| {
            error! {
                code: -1,
                msg: format!("Serialize params failed: {}", e),
            }
        })?;
        let xml = xml::build(&value);
        let url = format!("{}{}", PAY_API_DOMAIN, path);

        self.http.post_xml(&url, &xml).await
    }
}

/// 拼接 path 用（保留 query）
/// v2 里需要用 GET + 签名 query 的特殊绝对路径，对齐 Client::V2_URI_OVER_GETS
/// （首项 /appauth/getaccesstoken 返回 JSON，其余返回 XML）
pub const V2_URI_OVER_GETS: [&str; 5] = [
    "/appauth/getaccesstoken",
    "/papay/entrustweb",
    "/papay/h5entrustweb",
    "/papay/partner/entrustweb",
    "/papay/partner/h5entrustweb",
];

pub fn is_v2_signed_get(path: &str) -> bool {
    V2_URI_OVER_GETS.contains(&path)
}

fn params_string_path(url: &str) -> String {
    request_path(url)
}

/// 解析 v3 JSON 响应并判定错误，对齐 Client::request 的 v3 分支
pub fn parse_v3_response(text: &str) -> Result<Value> {
    if text.trim().is_empty() {
        return Ok(Value::Null);
    }

    let res: Value = serde_json::from_str(text).unwrap_or_else(|_| Value::String(text.to_owned()));

    // v3 错误体：{"code":"PARAM_ERROR","message":"..."}
    if let Some(code) = res["code"].as_str() {
        return Err(error! {
            code: res["status"].as_i64().unwrap_or(-1) as i32,
            msg: format!("{}: {}", code, res["message"].as_str().unwrap_or_default()),
        });
    }

    Ok(res)
}

/// 十六进制 sha256，对齐 PHP `hash('sha256', $contents)`
pub fn sha256_hex(contents: &[u8]) -> String {
    openssl::sha::sha256(contents)
        .iter()
        .map(|byte| format!("{:02x}", byte))
        .collect()
}

/// 解析 v2 XML 响应并做失败判定，对齐 Client::request 的 failureJudge
pub fn parse_v2_response(text: &str) -> Result<Value> {
    let value = xml::to_json(text).or_else(|_| {
        serde_json::from_str(text).map_err(|e| {
            error! {
                code: -1,
                msg: format!("Parse response failed: {}", e),
            }
        })
    })?;

    if let Some(code) = value["retcode"].as_i64() {
        if code != 0 {
            return Err(error! {
                code: code as i32,
                msg: format!("Request Failed: {}", value),
            });
        }
        return Ok(value);
    }

    let return_code = value["return_code"].as_str().unwrap_or("SUCCESS");
    let result_code = value["result_code"].as_str().unwrap_or("SUCCESS");

    if return_code != "SUCCESS" || result_code != "SUCCESS" {
        return Err(error! {
            code: -1,
            msg: format!(
                "Request Failed: return_code={}, result_code={}, err_code_des={}",
                return_code,
                result_code,
                value["err_code_des"].as_str().unwrap_or_default()
            ),
        });
    }

    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use openssl::rsa::Rsa;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};
    use std::thread;

    fn merchant() -> Merchant {
        let rsa = Rsa::generate(2048).unwrap();
        let private = String::from_utf8(rsa.private_key_to_pem().unwrap()).unwrap();

        Merchant::new("1900000109", private, "cert", "secretKey/apiV3key", Some("chengduhongtuikeji20210911888888".into()))
    }

    fn merchant_with_platform_cert(serial: &str) -> Merchant {
        signing_merchant(Some(serial))
    }

    /// 带真实证书的商户：v3 签名需要证书序列号
    fn signing_merchant(serial: Option<&str>) -> Merchant {
        let rsa = Rsa::generate(2048).unwrap();
        let private = String::from_utf8(rsa.private_key_to_pem().unwrap()).unwrap();
        let pkey = openssl::pkey::PKey::from_rsa(rsa).unwrap();
        let certificate = self_signed_cert_with(&pkey);

        let merchant = Merchant::new(
            "1900000109",
            private,
            certificate,
            "secretKey/apiV3key",
            Some("chengduhongtuikeji20210911888888".into()),
        );

        match serial {
            Some(serial) => merchant.add_platform_cert(serial, self_signed_cert()),
            None => merchant,
        }
    }

    /// 自签证书，用于验证平台证书解析路径
    fn self_signed_cert() -> String {
        let rsa = Rsa::generate(2048).unwrap();
        let pkey = openssl::pkey::PKey::from_rsa(rsa).unwrap();

        self_signed_cert_with(&pkey)
    }

    fn self_signed_cert_with(pkey: &openssl::pkey::PKey<openssl::pkey::Private>) -> String {
        use openssl::asn1::Asn1Time;
        use openssl::hash::MessageDigest;
        use openssl::x509::{X509NameBuilder, X509};

        let mut name = X509NameBuilder::new().unwrap();
        name.append_entry_by_text("CN", "wechat-pay-test").unwrap();
        let name = name.build();

        let mut builder = X509::builder().unwrap();
        builder.set_version(2).unwrap();
        builder.set_subject_name(&name).unwrap();
        builder.set_issuer_name(&name).unwrap();
        builder.set_pubkey(pkey).unwrap();
        builder.set_not_before(&Asn1Time::days_from_now(0).unwrap()).unwrap();
        builder
            .set_not_after(&Asn1Time::days_from_now(365).unwrap())
            .unwrap();
        builder.sign(pkey, MessageDigest::sha256()).unwrap();

        String::from_utf8(builder.build().to_pem().unwrap()).unwrap()
    }

    /// 极简 loopback 服务器：抓取完整的原始请求，回固定 JSON
    fn capture_server(response: &'static str) -> (u16, Arc<Mutex<Vec<String>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let captured: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = captured.clone();

        thread::spawn(move || {
            for stream in listener.incoming() {
                let mut stream = match stream {
                    Ok(stream) => stream,
                    Err(_) => continue,
                };

                let mut data: Vec<u8> = Vec::new();
                let mut buf = [0u8; 4096];

                loop {
                    let read = match stream.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(read) => read,
                    };
                    data.extend_from_slice(&buf[..read]);

                    if let Some(pos) = data.windows(4).position(|w| w == b"\r\n\r\n") {
                        let head = String::from_utf8_lossy(&data[..pos]).to_lowercase();
                        let length: usize = head
                            .lines()
                            .find_map(|line| line.strip_prefix("content-length:"))
                            .and_then(|value| value.trim().parse().ok())
                            .unwrap_or(0);

                        if data.len() >= pos + 4 + length {
                            break;
                        }
                    }
                }

                sink.lock()
                    .unwrap()
                    .push(String::from_utf8_lossy(&data).to_string());

                let body = response;
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
            }
        });

        (port, captured)
    }

    #[test]
    fn v3_path_detection() {
        assert!(is_v3_path("/v3/pay/transactions/jsapi"));
        assert!(is_v3_path("https://api.mch.weixin.qq.com/v3/refund/domestic/refunds"));
        assert!(is_v3_path("/hk/v3/pay/transactions/jsapi"));
        assert!(!is_v3_path("/pay/unifiedorder"));
        assert!(!is_v3_path("https://api.mch.weixin.qq.com/pay/micropay"));
    }

    #[test]
    fn v2_response_success() {
        let xml = "<xml><return_code><![CDATA[SUCCESS]]></return_code>\
<result_code><![CDATA[SUCCESS]]></result_code>\
<prepay_id><![CDATA[wx123]]></prepay_id></xml>";
        let value = parse_v2_response(xml).unwrap();
        assert_eq!(value["prepay_id"], "wx123");
    }

    #[test]
    fn v2_response_failure() {
        let xml = "<xml><return_code><![CDATA[FAIL]]></return_code>\
<return_msg><![CDATA[签名失败]]></return_msg></xml>";
        assert!(parse_v2_response(xml).is_err());
    }

    #[test]
    fn client_holds_merchant() {
        let client = Client::new(merchant());
        assert_eq!(client.merchant().mch_id, "1900000109");
        assert_eq!(PAY_API_DOMAIN, "https://api.mch.weixin.qq.com");
        assert!(client.platform_serial().is_none());
    }

    #[test]
    fn serial_header_requires_platform_certificate() {
        // 对齐 PHP：没有平台证书时抛 InvalidConfigException('Missing platform certificate.')
        let err = Client::new(merchant()).with_serial_header(None).unwrap_err();
        assert!(format!("{}", err).contains("Missing platform certificate."));
    }

    #[test]
    fn serial_header_falls_back_to_first_platform_certificate() {
        let client = Client::new(merchant_with_platform_cert("SERIAL-1"))
            .with_serial_header(None)
            .unwrap();
        assert_eq!(client.platform_serial(), Some("SERIAL-1"));

        let client = Client::new(merchant_with_platform_cert("SERIAL-1"))
            .with_serial_header(Some("SERIAL-2"))
            .unwrap();
        assert_eq!(client.platform_serial(), Some("SERIAL-2"));
    }

    #[test]
    fn sha256_hex_matches_php_hash() {
        assert_eq!(
            sha256_hex(b"hello"),
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    #[tokio::test]
    async fn upload_media_sends_file_and_meta_parts() {
        let (port, captured) = capture_server("{\"media_id\":\"MEDIA-1\"}");
        let client = Client::new(merchant_with_platform_cert("SERIAL-1"))
            .with_serial_header(None)
            .unwrap();

        let res = client
            .upload_media(
                &format!("http://127.0.0.1:{}/v3/merchant/media/upload", port),
                b"PNGDATA",
                None,
                Some("logo.png"),
            )
            .await
            .unwrap();

        assert_eq!(res["media_id"], "MEDIA-1");

        let request = captured.lock().unwrap()[0].clone();
        let lower = request.to_lowercase();
        // 微信支付 v3 签名与序列号头都在（hyper 会把头名转小写）
        assert!(lower.contains("authorization: wechatpay2-sha256-rsa2048"));
        assert!(lower.contains("wechatpay-serial: serial-1"));
        // multipart 两个 part：file（带文件名）与 meta（application/json）
        assert!(request.contains("name=\"file\"; filename=\"logo.png\""));
        assert!(request.contains("name=\"meta\""));
        assert!(request.contains("Content-Type: application/json"));

        // 整条请求只能有一个顶层 Content-Type，且必须是 multipart（对齐 PHP Form::toOptions 覆盖默认值）
        // 只看请求头区（首个空行之前），body 里的 part 头不算顶层头
        let header_block = request.split("\r\n\r\n").next().unwrap_or_default();
        let top_level_content_types: Vec<&str> = header_block
            .lines()
            .filter(|line| line.to_lowercase().starts_with("content-type:"))
            .collect();
        assert_eq!(
            top_level_content_types.len(),
            1,
            "unexpected content-type headers: {:?}",
            top_level_content_types
        );
        assert!(top_level_content_types[0]
            .to_lowercase()
            .contains("multipart/form-data; boundary="));
        assert!(request.contains("PNGDATA"));
        // meta 默认值对齐 PHP：filename + sha256(contents)
        assert!(request.contains("\"filename\":\"logo.png\""));
        assert!(request.contains(&sha256_hex(b"PNGDATA")));
    }

    #[tokio::test]
    async fn upload_media_accepts_explicit_meta() {
        let (port, captured) = capture_server("{\"media_id\":\"MEDIA-2\"}");
        let client = Client::new(signing_merchant(None));
        let meta = serde_json::json!({"filename": "a.mp4", "sha256": "deadbeef"});

        client
            .upload_media(
                &format!("http://127.0.0.1:{}/v3/merchant/media/upload", port),
                b"VIDEO",
                Some(&meta),
                Some("a.mp4"),
            )
            .await
            .unwrap();

        let request = captured.lock().unwrap()[0].clone();
        assert!(request.contains("deadbeef"));
        assert!(!request.to_lowercase().contains("wechatpay-serial"));
    }
}
