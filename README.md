# wechat-rs

> 微信公众号、开放平台、小程序、小商店、微信支付,企业微信接口，采用Rust语言的对接实现接口对接的sdk，方便使用Rust语言进行微信开发的同学开箱即用。

## 子模块

> 针对每个子模块进行整体性说明
> 子模块采用features的方式进行设计规划,可以整体使用也可以独立使用按项目的需要进行引入.

例如只需要使用小程序模块,在``cargo.toml``中引入时添加features

```toml
wechat = {version="0.1.0", features=["weapp"]}
```

> 目录结构：
> ```
> |-- mp 微信公众号
> |-- open 开放平台
> |-- pay 微信支付
> |-- sdk 基础功能应用集
> |-- src 微信对接应用对外公开主模块
> |-- store 微信小店
> |-- weapp 微信小程序
> |-- web 微信对接功能的web应用实现demo
> |-- work 企业微信
> ```

### 公众号

> 微信公众号接口对接

### 开放平台

> 微信开放平台接口功能对接

### 小程序

> 微信小程序接口功能对接

### 微信小店

> 微信小店功能接口对接

### 微信支付

> 微信支付功能,包含服务商接口对接

### 企业微信

> 只使用一个第三方平台，小程序发版也只有一个模板；


## 与 EasyWeChat 6.x 的对齐矩阵

> 参考实现：`easywechat-6.x`（Kernel + MiniApp/OfficialAccount/OpenPlatform/OpenWork/Pay/Work）
> 下表列出 rust 侧对应位置，便于按 PHP 概念查找等价实现。

| EasyWeChat | wechat-rs | 说明 |
| --- | --- | --- |
| `Kernel\Config` | `sdk::config::Config` / `mp::MpConfig` / `weapp::WeappConfig` / `work::WorkConfig` / `open::openplatform::OpenPlatformConfig` | 各模块自带必填项校验 |
| `Kernel\Encryptor` | `sdk::encryptor::Encryptor` | AES-256-CBC + PKCS7，错误码同 PHP 侧（-40001/-40004/-40005/-40006/-40007/-41003） |
| `Kernel\ServerResponse` | `sdk::ServerResponse` | 200 文本/XML/JSON 响应 |
| `Kernel\Support\Str` | `sdk::strutil` | random / snake_case / camel_case |
| `Kernel\Support\Xml` | `sdk::xml`、`sdk::xmlutil` | XML ↔ JSON |
| `Kernel\Support\Pkcs7` | `sdk::encryptor::{pkcs7_padding, pkcs7_unpadding}` | |
| `Kernel\Support\AesCbc` | `sdk::wxcrypto::{aes128_cbc_encrypt, aes128_cbc_decrypt, aes256_cbc_encrypt, aes256_cbc_decrypt}` | |
| `Kernel\Support\AesGcm` / `AesEcb` | `wechat_pay::aes::{aes_gcm_encrypt, aes_gcm_decrypt, aes_ecb_encrypt, aes_ecb_decrypt}` | 基于 openssl |
| `Kernel\Form\Form` / `File` / `DataPart` | `sdk::form::{Form, File}::from_bytes` + `Form::content_part` + `Client::post_multipart(_with_headers)` | 素材/媒体上传；`content_part` 用于 v3 媒体上传的 `meta` JSON 段 |
| `Kernel\HttpClient\*` | `sdk::Client` | `AccessTokenAwareClient` + `AccessTokenExpiredRetryStrategy`：`request_with_access_token` / `post_with_access_token` 命中 40001/40014/42001/42007 时**先强制刷新 token（新 token 立即写回缓存，对齐 PHP `refresh()` 的 `cache->set`）再重试一次**；`build_url`、`request_raw`、`post_multipart`（PHP 只判定 42001，此处为超集，并额外识别 `access_token expired` 文案）；XML 请求体用 `text/xml`（对齐 `Kernel/HttpClient/HttpClientMethods.php`），XML 响应用 `application/xml`（对齐 `RespondXmlMessage`） |
| `Kernel\Message` | `mp::message::Message`、`wechat_pay::Message` | |
| `Kernel\Traits\InteractWithHandlers` | `sdk::Server::{with_handler, on_message_type, on_event, handle}` | 处理器链，`mp`/`work`/`open` 的 Server 都转发到这里 |
| `Kernel\Traits\RespondXmlMessage` | `sdk::Server::transform_to_reply`（`sdk::server::normalize_reply`） | 反转 To/From、补 CreateTime、按需加密；字符串视为 text，对象保留全部属性（news/image/voice/video 等） |
| cache 注入（`InteractWithCache`） | `sdk::cache::{CacheStore, set_cache_store}` + `wechat_redis::RedisCacheStore` | 默认进程内 MemoryCache，可换成 Redis 共享 token/ticket |
| 缓存 key（各 `getKey`） | `AccessToken::get_key` / `JsApiTicket::get_key` / `work::{AccessToken, JsApiTicket}::get_key` | 与 PHP 完全一致：`{app_type}.access_token.{appId}.{secret}.{stable}`、`{app_type}.jsapi_ticket.{appId}`、`work.access_token.{corpId}.{secret}`、`work.jsapi_ticket.{corpId}[.{agentId}]` |
| `OfficialAccount\Application` | `mp::Application` | Account/Encryptor/Server/AccessToken/JsApiTicket/Utils/OAuth；`with_oauth_component` + `mp::AuthorizerOAuth` 支持第三方平台代公众号授权（授权页带 `component_appid`，code 换 token 走 `/sns/oauth2/component/access_token`），`with_encryptor` 可注入组件侧加解密器 |
| `OfficialAccount\Server` | `mp::MpServer` | `serve()`、echostr 回显、明文/密文校验、`on_message_type`/`on_event`、回复加密 |
| `MiniApp\Application` | `weapp::Application` | 含 stable_token、Encryptor、`require_encryption`（配置项透传到 `server()`）、`with_encryptor`、`get_server()`（回调 serve + handler）；`qrcode()` / `subscribe_message()` 复用应用同一份 token 配置 |
| `MiniApp\Utils` / `Decryptor` | `weapp::Utils` / `weapp::Decryptor` | code2Session、getPhoneNumber、session 解密 |
| `OpenPlatform\*` | `open::openplatform::{Application, ComponentAccessToken, VerifyTicket, AuthorizerAccessToken, Authorization, Server}` | component token、授权、代公众号/小程序调用 |
| `OpenPlatform\Application::getOAuth` / `createAuthorizerOAuthFactory` | `open::openplatform::Application::{oauth, authorizer_oauth, component_oauth}` | 组件自身授权（`client_id` = 组件 appid）与代公众号授权工厂（`component_appid` + 惰性组件 token） |
| `OpenPlatform\Application::getOfficialAccount` / `getMiniApp` | `Application::{official_account, mini_app}` | 按 PHP 语义注入授权方 appid + 组件 token/aes_key + `require_encryption` + 组件 Encryptor + OAuth factory；`component_oauth()` 暴露共享组件上下文 |
| `OpenWork\*` | `open::openwork::{Application, ProviderAccessToken, SuiteAccessToken, SuiteTicket, AuthorizerAccessToken, JsApiTicket, Server}` | 服务商/第三方，`Server::serve` 回包按 suite encryptor 加密；`JsApiTicket` 走共享缓存（`expires_in` 为 TTL，缺省 7200s，可用 `with_base_domain` 指向代理/测试服务） |
| `OpenWork\Application::{getOAuth, getCorpOAuth}` | `open::openwork::{Application::oauth, Application::corp_oauth, SuiteOAuth}` | 授权页 `appid` 为 suite_id（`oauth`）或授权方 corp_id（`corp_oauth`）并带 `component_appid`；`suite_ticket()`/`suite_access_token()` 支持显式值或管理器按需刷新；`user_info` → `cgi-bin/service/getuserinfo3rd`、`user_detail` → `cgi-bin/service/getuserdetail3rd`、`user_from_code` 合并两者；`qr_connect_url` 对应服务商扫码登录 |
| `Pay\Merchant` / `Signature` / `LegacySignature` | `wechat_pay::{Merchant, Signature, LegacySignature}` | v3 RSA-SHA256 头、v2 MD5/HMAC |
| `Pay\Client::withSerialHeader` / `uploadMedia` | `wechat_pay::Client::{with_serial_header, with_header, upload_media}`、`wechat_pay::Client::sha256_hex` | `Wechatpay-Serial` 头（缺省取首张平台证书，缺失即报错 `Missing platform certificate.`）；媒体上传 multipart 含 `file` + `meta`，签名 body 用 meta JSON，sha256 由 `Client::sha256_hex` 计算 |
| `Pay\Validator` / `ResponseValidator` | `wechat_pay::Validator` | 回调与响应验签、300 秒时钟偏移 |
| `Pay\Server` | `wechat_pay::Server` | 通知验签 + AES-GCM 解密、`handle_paid` / `handle_refunded` |
| `Pay\Utils` | `wechat_pay::Utils` | JSAPI/APP/小程序/扫码 二次签名 |
| `Pay\URLSchemeBuilder` | `wechat_pay::URLSchemeBuilder` | `weixin://` scheme |
| `Work\*` | `work::{Application, AccessToken, JsApiTicket, Encryptor, Server, Utils, OAuth}` | 企业微信 `qyapi.weixin.qq.com`；`Server` 提供 `serve()` 与 change_contact/user/party/batch_job 全套处理器；`Application::oauth()` 对齐 `getOAuth`（企业微信授权页带 `agentid`、`qr_connect_url`、`user_id`/`user_detail`/`user`/`user_from_code`），应用内 get/post 走 token 感知客户端 |

### 测试

```
cargo test --features full --workspace
```

各 crate 的单元测试覆盖签名、加解密、消息解析、处理器分发与响应构造，不依赖外网：
- token 过期重试、Redis 缓存接入都用进程内的极简 HTTP/RESP 服务器做真实 socket 往返；
- 需要真实 Redis 的手工用例默认 `#[ignore]`，用
  `WECHAT_REDIS_TEST_URL=redis://127.0.0.1/9 cargo test -p wechat_redis -- --ignored` 手动触发。

### 缓存后端

```rust
// 默认进程内缓存
let token = wechat_sdk::AccessToken::new("official_account", app_id, secret);

// 换成 Redis：之后 access_token / jsapi_ticket / verify_ticket 全部走 Redis
wechat_redis::install_redis_cache("redis://127.0.0.1/")?;
```

`wechat_sdk::cache::CacheStore` 是自定义缓存的接入点（实现 `get`/`set`/`forget`/`keys` 即可）。

## 布署

```
1、修改第三方里面的域名配置，并全网发布
2、待全网发布成功后
1）、执行 
-- api 请求域名
2）、更新extjson并保存第三方配置
3、布署时，布署几套，而且每套服务器分布式布署

需要测试的点

检测第三方access_token是否共用，已经授权过的客户能正常使用；
后台公众号/小程序授权；
小程序发布；
网页授权；
以及检测h5微信相关的操作；

```