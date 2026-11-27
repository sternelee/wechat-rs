//! copyright © ecdata.cn 2021 - present
//! 公众号工具，对齐 EasyWeChat\OfficialAccount\Utils
//! created by shaipe 20210302

use crate::application::Application;
use serde_json::Value;
use wechat_sdk::WechatResult as Result;

/// 公众号工具
#[derive(Debug)]
pub struct Utils<'a> {
    app: &'a Application,
}

impl<'a> Utils<'a> {
    /// 创建工具
    pub fn new(app: &'a Application) -> Utils<'a> {
        Utils { app }
    }

    /// 生成 JS-SDK 配置，对齐 Utils::buildJsSdkConfig
    pub async fn build_js_sdk_config(
        &self,
        url: &str,
        js_api_list: Vec<String>,
        open_tag_list: Vec<String>,
        debug: bool,
    ) -> Result<Value> {
        self.app
            .ticket()
            .build_config(url, js_api_list, open_tag_list, debug)
            .await
    }

    /// 只生成签名部分，对齐 JsApiTicket::configSignature
    pub async fn config_signature(
        &self,
        url: &str,
        nonce: Option<&str>,
        timestamp: Option<u64>,
    ) -> Result<Value> {
        self.app.ticket().config_signature(url, nonce, timestamp).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MpConfig;

    #[test]
    fn utils_can_be_created() {
        let app = Application::new(MpConfig::new("wx123", "secret"));
        let utils = app.utils();
        // 只验证可构造，签名需要网络与 ticket
        assert_eq!(format!("{:?}", utils).contains("Utils"), true);
    }
}
