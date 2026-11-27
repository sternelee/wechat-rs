//! copyright © ecdata.cn 2021 - present
//! 企业微信工具，对齐 EasyWeChat\Work\Utils
//! created by shaipe 20210228

use crate::application::Application;
use serde_json::{json, Value};
use wechat_sdk::{strutil::random, WechatResult as Result};

/// 企业微信工具
pub struct Utils<'a> {
    app: &'a Application,
}

impl<'a> Utils<'a> {
    /// 创建工具
    pub fn new(app: &'a Application) -> Utils<'a> {
        Utils { app }
    }

    /// 网页应用 JS-SDK 配置，对齐 buildJsSdkConfig
    pub async fn build_js_sdk_config(
        &self,
        url: &str,
        js_api_list: &[&str],
        open_tag_list: &[&str],
        debug: bool,
        beta: bool,
    ) -> Result<Value> {
        let signature = self
            .app
            .ticket()
            .create_config_signature(url, Some(&random(16)), None)
            .await?;

        Ok(Self::merge_config(
            js_api_list,
            open_tag_list,
            debug,
            Some(beta),
            signature,
        ))
    }

    /// 应用 JS-SDK 配置，对齐 buildJsSdkAgentConfig
    pub async fn build_js_sdk_agent_config(
        &self,
        agent_id: i64,
        url: &str,
        js_api_list: &[&str],
        open_tag_list: &[&str],
        debug: bool,
    ) -> Result<Value> {
        let signature = self
            .app
            .ticket()
            .create_agent_config_signature(agent_id, url, Some(&random(16)), None)
            .await?;

        Ok(Self::merge_config(
            js_api_list,
            open_tag_list,
            debug,
            None,
            signature,
        ))
    }

    /// 合并配置项
    fn merge_config(
        js_api_list: &[&str],
        open_tag_list: &[&str],
        debug: bool,
        beta: Option<bool>,
        signature: Value,
    ) -> Value {
        let mut config = json!({
            "jsApiList": js_api_list,
            "openTagList": open_tag_list,
            "debug": debug,
        });

        if let Some(beta) = beta {
            config["beta"] = json!(beta);
        }

        if let (Some(target), Value::Object(source)) = (config.as_object_mut(), signature) {
            for (key, value) in source {
                target.insert(key, value);
            }
        }

        config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_config_puts_signature_fields() {
        let merged = Utils::merge_config(
            &["chooseImage"],
            &[],
            false,
            Some(true),
            json!({"appId": "corp", "signature": "sig"}),
        );

        assert_eq!(merged["jsApiList"][0], "chooseImage");
        assert_eq!(merged["appId"], "corp");
        assert_eq!(merged["signature"], "sig");
        assert_eq!(merged["beta"], true);
    }
}
