//! copyright © shaipe 2020 - persent
//! 微信对接的网络请求客户端
//!
//! 对齐 EasyWeChat Kernel\HttpClient：
//! - `AccessTokenExpiredRetryStrategy`：access_token 过期时**先刷新 token 再重试一次**，
//!   见 [`Client::request_with_access_token`] / [`Client::post_with_access_token`]；
//!   普通 `get`/`post` 不做无意义的同请求重试。
//! - 统一封装 JSON / XML / 表单请求

use crate::WechatResult;
use bytes::Bytes;
use reqwest::header;
use reqwest::Client as HttpClient;
use serde::Serialize;
use std::time::Duration;

const DEFAULT_USER_AGENT: &'static str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/70.0.3534.4 Safari/537.36";

/// access_token 失效的错误码
const TOKEN_EXPIRED_CODES: [i64; 4] = [40001, 40014, 42001, 42007];

/// 请求客户端
#[derive(Debug, Clone)]
pub struct Client {
    pub(crate) client: HttpClient,
}

impl Default for Client {
    fn default() -> Self {
        Client::new()
    }
}

impl Client {
    pub fn new() -> Self {
        let mut headers = header::HeaderMap::new();
        headers.insert(
            header::USER_AGENT,
            header::HeaderValue::from_static(DEFAULT_USER_AGENT),
        );

        Client {
            client: HttpClient::builder()
                .timeout(Duration::from_secs(300))
                .connect_timeout(Duration::from_secs(300))
                .default_headers(headers)
                .build()
                .unwrap(),
        }
    }

    /// post方式提交数据
    /// url:
    /// param:
    pub async fn post<T: Serialize + ?Sized>(&self, url: &str, params: &T) -> WechatResult<String> {
        self.do_post(url, params).await
    }

    /// POST JSON，并解析 JSON 结果（errcode 非 0 时返回错误）
    pub async fn post_json<T: Serialize + ?Sized>(
        &self,
        url: &str,
        params: &T,
    ) -> WechatResult<serde_json::Value> {
        let res = self.do_post(url, params).await?;
        crate::json_decode(&res)
    }

    /// POST 原始表单（application/x-www-form-urlencoded）
    /// 带查询参数的 JSON POST（如 access_token 放 query）
    pub async fn post_json_with_query<T: Serialize + ?Sized>(
        &self,
        url: &str,
        query: &[(String, String)],
        body: &T,
    ) -> WechatResult<serde_json::Value> {
        let url = if query.is_empty() {
            url.to_owned()
        } else {
            format!("{}?{}", url, crate::signature::query_string(query))
        };

        self.post_json(&url, body).await
    }

    /// multipart/form-data 上传（素材、媒体文件等）
    pub async fn post_multipart(
        &self,
        url: &str,
        form: &crate::form::Form,
    ) -> WechatResult<String> {
        self.post_multipart_with_headers(url, form, &[]).await
    }

    /// multipart/form-data 上传，并附加自定义请求头（如微信支付 v3 的 Authorization / Wechatpay-Serial）
    pub async fn post_multipart_with_headers(
        &self,
        url: &str,
        form: &crate::form::Form,
        headers: &[(String, String)],
    ) -> WechatResult<String> {
        let (content_type, body) = form.build()?;

        let mut request = self
            .client
            .post(url)
            .header(reqwest::header::CONTENT_TYPE, content_type);

        // multipart 的 Content-Type 由 Form 决定（带 boundary），调用方传入的同名头必须丢弃，
        // 否则 reqwest 会 append 出两个 Content-Type（PHP 侧只有 multipart 一个）。
        for (name, value) in headers {
            if name.eq_ignore_ascii_case("content-type") {
                continue;
            }

            request = request.header(name.as_str(), value.as_str());
        }

        let res = request
            .body(body)
            .send()
            .await
            .map_err(|e| {
                error! {
                    code: -1,
                    msg: format!("Send request error: {}", e),
                }
            })?;

        match res.status().as_u16() {
            200..=299 => res.text().await.map_err(|e| {
                error! {
                    code: -1,
                    msg: format!("Send request error: {}", e),
                }
            }),
            status => Err(error! {
                code: 500,
                msg: format!("status={}", status),
            }),
        }
    }

    pub async fn post_form(&self, url: &str, body: &str) -> WechatResult<String> {
        match self
            .client
            .post(url)
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(body.to_owned())
            .send()
            .await
        {
            Ok(res) => match res.status().as_u16() {
                200..=299 => match res.text().await {
                    Ok(txt) => Ok(txt),
                    Err(e) => Err(error! {
                        code: -1,
                        msg: format!("Send request error: {}", e),
                    }),
                },
                status => Err(error! {
                    code: 500,
                    msg: format!("status={}", status),
                }),
            },
            Err(e) => Err(error! {
                code: 500,
                msg: format!("Send request error: {}", e),
            }),
        }
    }

    /// POST XML（微信支付 v2 接口）
    pub async fn post_xml(&self, url: &str, xml: &str) -> WechatResult<String> {
        match self
            .client
            .post(url)
            .header(header::CONTENT_TYPE, "text/xml")
            .body(xml.to_owned())
            .send()
            .await
        {
            Ok(res) => match res.status().as_u16() {
                200..=299 => match res.text().await {
                    Ok(txt) => Ok(txt),
                    Err(e) => Err(error! {
                        code: -1,
                        msg: format!("Send request error: {}", e),
                    }),
                },
                status => Err(error! {
                    code: 500,
                    msg: format!("status={}", status),
                }),
            },
            Err(e) => Err(error! {
                code: 500,
                msg: format!("Send request error: {}", e),
            }),
        }
    }

    /// 发送带自定义 header 的请求（微信支付 v3 需要 Authorization 头）
    pub async fn request_raw(
        &self,
        method: &str,
        url: &str,
        headers: Vec<(String, String)>,
        body: Option<String>,
    ) -> WechatResult<String> {
        let method = reqwest::Method::from_bytes(method.as_bytes()).map_err(|e| {
            error! {
                code: -1,
                msg: format!("Invalid http method: {}", e),
            }
        })?;

        let mut req = self.client.request(method, url);
        for (k, v) in headers {
            req = req.header(k, v);
        }
        if let Some(body) = body {
            req = req
                .header(header::CONTENT_TYPE, "application/json")
                .body(body);
        }

        match req.send().await {
            Ok(res) => {
                let status = res.status().as_u16();
                let text = res.text().await.map_err(|e| {
                    error! {
                        code: -1,
                        msg: format!("Send request error: {}", e),
                    }
                })?;
                if status >= 400 {
                    log!("request_raw status={} body={}", status, text);
                    return Err(error! {
                        code: status as i32,
                        msg: text,
                    });
                }
                Ok(text)
            }
            Err(e) => Err(error! {
                code: 500,
                msg: format!("Send request error: {}", e),
            }),
        }
    }

    /// 带 access_token 的 GET：命中过期错误码时调用 `refresh` 换新 token 后重试一次
    ///
    /// 对齐 EasyWeChat `AccessTokenAwareClient` + `AccessTokenExpiredRetryStrategy`
    /// （`shouldRetry` 内会先 `accessToken->refresh()`）。
    ///
    /// - `token`：本次请求使用的 access_token
    /// - `build_url`：用 token 拼出完整 URL（token 在 query 或 header 里都由调用方决定）
    /// - `refresh`：强制刷新 access_token，返回新的 token
    pub async fn request_with_access_token<F, R, Fut>(
        &self,
        token: &str,
        build_url: F,
        refresh: R,
    ) -> WechatResult<String>
    where
        F: Fn(&str) -> String,
        R: Fn() -> Fut,
        Fut: std::future::Future<Output = WechatResult<String>>,
    {
        let first = self.request_raw("GET", &build_url(token), Vec::new(), None).await?;

        if !Self::is_token_expired(&first) {
            return Ok(first);
        }

        // token 已过期：先刷新，再用新 token 重试一次
        let fresh_token = refresh().await?;

        self.request_raw("GET", &build_url(&fresh_token), Vec::new(), None)
            .await
    }

    /// 带 access_token 的 JSON POST：命中过期错误码时刷新 token 后重试一次
    pub async fn post_with_access_token<T, F, R, Fut>(
        &self,
        token: &str,
        body: &T,
        build_url: F,
        refresh: R,
    ) -> WechatResult<String>
    where
        T: Serialize + ?Sized,
        F: Fn(&str) -> String,
        R: Fn() -> Fut,
        Fut: std::future::Future<Output = WechatResult<String>>,
    {
        let payload = serde_json::to_string(body).map_err(|e| {
            error! {
                code: -1,
                msg: format!("Serialize body failed: {}", e),
            }
        })?;

        let first = self
            .request_raw("POST", &build_url(token), Vec::new(), Some(payload.clone()))
            .await?;

        if !Self::is_token_expired(&first) {
            return Ok(first);
        }

        let fresh_token = refresh().await?;

        self.request_raw(
            "POST",
            &build_url(&fresh_token),
            Vec::new(),
            Some(payload),
        )
        .await
    }

    async fn do_post<T: Serialize + ?Sized>(
        &self,
        url: &str,
        params: &T,
    ) -> WechatResult<String> {
        match self.client.post(url).json(params).send().await {
            Ok(res) => match res.status().as_u16() {
                200..=299 => match res.text().await {
                    Ok(txt) => Ok(txt),
                    Err(e) => Err(error! {
                        code: -1,
                        msg: format!("Send request error: {}", e),
                    }),
                },
                status => Err(error! {
                    code: 500,
                    msg: format!("status={}", status),
                }),
            },
            Err(e) => Err(error! {
                code: 500,
                msg: format!("Send request error: {}", e),
            }),
        }
    }

    /// 判断返回体是否为 access_token 失效
    /// 是否 access_token 失效（对齐 AccessTokenExpiredRetryStrategy 的 decider）
    ///
    /// PHP 的 decider 为「响应体包含 42001 且包含 access_token expired」；
    /// 这里同时覆盖 40001/40014/42007 等同属 token 失效的错误码，并对非 JSON 响应
    /// 退化为文本判断。
    pub fn is_token_expired(body: &str) -> bool {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(body) {
            let code = v["errcode"].as_i64().unwrap_or(0);
            if TOKEN_EXPIRED_CODES.contains(&code) {
                return true;
            }
        }

        body.contains("access_token expired")
    }

    /// 发送二进制文件
    pub async fn post_betyes(&self, url: &str, body: Bytes) -> WechatResult<String> {
        match self.client.post(url).body(body).send().await {
            Ok(res) => {
                if res.status() == 200 {
                    match res.text().await {
                        Ok(txt) => Ok(txt),
                        Err(e) => Err(error! {
                            code: -1,
                            msg: format!("Send request error: {}", e),
                        }),
                    }
                } else {
                    Err(error! {
                        code: 500,
                        msg: format!("status={}", res.status()),
                    })
                }
            }
            Err(e) => Err(error! {
                code: 500,
                msg: format!("Send request error: {}", e),
            }),
        }
    }

    /// get方法
    #[allow(dead_code)]
    pub async fn get(&self, url: &str) -> WechatResult<String> {
        match self.client.get(url).send().await {
            Ok(res) => match res.status().as_u16() {
                200..=299 => match res.text().await {
                    Ok(txt) => Ok(txt),
                    Err(e) => Err(error! {
                        code: -1,
                        msg: format!("Send request error: {}", e),
                    }),
                },
                status => Err(error! {
                    code: 500,
                    msg: format!("status={}", status),
                }),
            },
            Err(e) => Err(error! {
                code: 500,
                msg: format!("Send request error: {}", e),
            }),
        }
    }

    /// GET 并解析 JSON
    pub async fn get_json(&self, url: &str) -> WechatResult<serde_json::Value> {
        let res = self.get(url).await?;
        crate::json_decode(&res)
    }

    /// 拼接查询参数（自动 urlencode）
    pub fn build_url(base: &str, path: &str, query: &[(&str, &str)]) -> String {
        use percent_encoding::{utf8_percent_encode, AsciiSet, CONTROLS};

        /// RFC3986 中除 unreserved 之外的字符才需要编码
        /// 即保留 A-Z a-z 0-9 - . _ ~
        const QUERY_ENCODE_SET: &AsciiSet = &CONTROLS
            .add(b' ')
            .add(b'"')
            .add(b'#')
            .add(b'$')
            .add(b'%')
            .add(b'&')
            .add(b'\'')
            .add(b'(')
            .add(b')')
            .add(b'*')
            .add(b'+')
            .add(b',')
            .add(b'/')
            .add(b':')
            .add(b';')
            .add(b'<')
            .add(b'=')
            .add(b'>')
            .add(b'?')
            .add(b'@')
            .add(b'[')
            .add(b'\\')
            .add(b']')
            .add(b'^')
            .add(b'`')
            .add(b'{')
            .add(b'|')
            .add(b'}');

        let pairs: Vec<String> = query
            .iter()
            .map(|(k, v)| {
                format!(
                    "{}={}",
                    utf8_percent_encode(k, QUERY_ENCODE_SET),
                    utf8_percent_encode(v, QUERY_ENCODE_SET)
                )
            })
            .collect();
        let query = pairs.join("&");
        if query.is_empty() {
            format!("{}{}", base, path)
        } else {
            let sep = if path.contains('?') { '&' } else { '?' };
            format!("{}{}{}{}", base, path, sep, query)
        }
    }

    /// GET 二进制数据
    pub async fn get_bytes(&self, url: &str) -> WechatResult<Vec<u8>> {
        match self.client.get(url).send().await {
            Ok(res) => {
                if res.status() == 200 {
                    match res.bytes().await {
                        Ok(b) => Ok(b.to_vec()),
                        Err(e) => Err(error! { code: -1, msg: format!("Send request error: {}", e) }),
                    }
                } else {
                    Err(error! { code: 500, msg: format!("status={}", res.status()) })
                }
            }
            Err(e) => Err(error! { code: 500, msg: format!("Send request error: {}", e) }),
        }
    }

    pub async fn post_bytes(&self, url: &str, params: &serde_json::Value) -> WechatResult<Vec<u8>> {
        match self.client.post(url).json(params).send().await {
            Ok(res) => {
                if res.status() == 200 {
                    match res.bytes().await {
                        Ok(b) => Ok(b.to_vec()),
                        Err(e) => Err(error! { code: -1, msg: format!("Send request error: {}", e) }),
                    }
                } else {
                    Err(error! { code: 500, msg: format!("status={}", res.status()) })
                }
            }
            Err(e) => Err(error! { code: 500, msg: format!("Send request error: {}", e) }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 极简 HTTP 服务器：按顺序返回预设响应，用于验证 token 过期重试
    fn serve_sequence(responses: Vec<String>) -> (u16, std::sync::Arc<std::sync::Mutex<Vec<String>>>) {
        use std::io::{BufRead, BufReader, Write};
        use std::net::TcpListener;
        use std::sync::{Arc, Mutex};

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let seen: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let requests = seen.clone();

        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut request_line = String::new();

                if reader.read_line(&mut request_line).is_err() {
                    continue;
                }
                // 读到 header 结束
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                        break;
                    }
                }

                requests.lock().unwrap().push(request_line.trim().to_owned());

                let body = responses
                    .get(requests.lock().unwrap().len() - 1)
                    .cloned()
                    .unwrap_or_else(|| "{}".to_owned());
                let reply = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );

                let mut stream = stream;
                let _ = stream.write_all(reply.as_bytes());
                let _ = stream.flush();
            }
        });

        (port, seen)
    }


    #[test]
    fn build_url_with_query() {
        let url = Client::build_url(
            "https://api.weixin.qq.com",
            "/cgi-bin/user/info",
            &[("access_token", "abc"), ("openid", "o1")],
        );
        assert_eq!(
            url,
            format!(
                "{}/cgi-bin/user/info?access_token={}&openid={}",
                "https://api.weixin.qq.com", "abc", "o1"
            )
        );
    }

    #[test]
    fn build_url_without_query() {
        let url = Client::build_url("https://api.weixin.qq.com", "/cgi-bin/token", &[]);
        assert_eq!(url, "https://api.weixin.qq.com/cgi-bin/token");
    }

    #[tokio::test]
    async fn retries_with_refreshed_token_after_expiry() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        let (port, seen) = serve_sequence(vec![
            r#"{"errcode":42001,"errmsg":"access_token expired"}"#.to_owned(),
            r#"{"ok":true}"#.to_owned(),
        ]);
        let base = format!("http://127.0.0.1:{}", port);
        let client = Client::new();
        let refreshes = Arc::new(AtomicUsize::new(0));
        let counter = refreshes.clone();
        let base_url = base.clone();

        let res = client
            .request_with_access_token(
                "stale-token",
                move |token| format!("{}/cgi-bin/test?access_token={}", base_url, token),
                move || {
                    let counter = counter.clone();
                    async move {
                        counter.fetch_add(1, Ordering::SeqCst);
                        Ok("fresh-token".to_owned())
                    }
                },
            )
            .await
            .unwrap();

        assert_eq!(res, r#"{"ok":true}"#);
        assert_eq!(refreshes.load(Ordering::SeqCst), 1);

        let seen = seen.lock().unwrap().clone();
        assert_eq!(seen.len(), 2);
        assert!(seen[0].contains("access_token=stale-token"));
        assert!(seen[1].contains("access_token=fresh-token"));
    }

    #[tokio::test]
    async fn no_retry_when_token_valid() {
        use std::sync::Arc;

        let (port, seen) = serve_sequence(vec![r#"{"ok":true}"#.to_owned()]);
        let client = Client::new();
        let base_url = format!("http://127.0.0.1:{}", port);
        let refreshes = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = refreshes.clone();

        let res = client
            .request_with_access_token(
                "good-token",
                move |token| format!("{}/cgi-bin/test?access_token={}", base_url, token),
                move || {
                    let counter = counter.clone();
                    async move {
                        counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        Ok("unused".to_owned())
                    }
                },
            )
            .await
            .unwrap();

        assert_eq!(res, r#"{"ok":true}"#);
        assert_eq!(refreshes.load(std::sync::atomic::Ordering::SeqCst), 0);
        assert_eq!(seen.lock().unwrap().len(), 1);
    }

    /// 42001 重试时刷新得到的 token 必须回写缓存，否则后续调用会重复“刷新+重试”
    #[tokio::test]
    async fn retry_refreshes_and_updates_cache() {
        use std::sync::Arc;

        let _guard = crate::cache::TEST_GUARD
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let (port, seen) = serve_sequence(vec![
            r#"{"errcode":42001,"errmsg":"access_token expired"}"#.to_owned(),
            r#"{"ok":true}"#.to_owned(),
        ]);
        let base = format!("http://127.0.0.1:{}", port);
        let client = Client::new();

        let token = Arc::new(crate::token::AccessToken::new("test", "wx_retry_cache", "secret"));
        token.forget();
        assert!(token.get_cached().is_none());

        let refresher = token.clone();
        let res = client
            .request_with_access_token(
                "stale-token",
                move |access_token| format!("{}/cgi-bin/test?access_token={}", base, access_token),
                move || {
                    let refresher = refresher.clone();
                    async move {
                        let fresh = refresher
                            .refresh_with(|| async {
                                Ok(crate::token::Token {
                                    access_token: "fresh-token".to_owned(),
                                    create_time: crate::current_timestamp(),
                                    expires: 7200,
                                })
                            })
                            .await?;
                        Ok(fresh.access_token)
                    }
                },
            )
            .await
            .unwrap();

        assert_eq!(res, r#"{"ok":true}"#);
        let seen = seen.lock().unwrap().clone();
        assert_eq!(seen.len(), 2);
        assert!(seen[1].contains("access_token=fresh-token"));
        assert_eq!(
            token
                .get_cached()
                .expect("refreshed token must be persisted")
                .access_token,
            "fresh-token"
        );
    }

    #[test]
    fn token_expired_detection_from_plain_text() {
        assert!(Client::is_token_expired("access_token expired"));
        assert!(!Client::is_token_expired("system busy"));
    }

    #[test]
    fn token_expired_detection() {
        assert!(Client::is_token_expired(r#"{"errcode":42001,"errmsg":"access_token expired"}"#));
        assert!(!Client::is_token_expired(r#"{"errcode":0,"errmsg":"ok"}"#));
        assert!(!Client::is_token_expired("not-json"));
    }
}
