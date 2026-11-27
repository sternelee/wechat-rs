//! copyright © ecdata.cn 2021 - present
//! 订阅消息
//! created by shaipe 20210228
//! https://developers.weixin.qq.com/miniprogram/dev/api-backend/open-api/subscribe-message/subscribeMessage.send.html

use wechat_sdk::{constant::API_DOMAIN, AccessToken, Client, WechatResult};

#[derive(Debug, Clone)]
pub struct SubscribeMessage {
    pub app_id: String,
    pub secret: String,
    /// 是否使用稳定版 access_token（与应用配置保持一致）
    pub stable: bool,
}

impl SubscribeMessage {
    pub fn new(app_id: &str, secret: &str) -> Self {
        Self {
            app_id: app_id.to_owned(),
            secret: secret.to_owned(),
            stable: false,
        }
    }

    /// 使用稳定版 access_token
    pub fn with_stable(mut self, stable: bool) -> Self {
        self.stable = stable;
        self
    }

    async fn get_access_token(&self) -> String {
        match AccessToken::new("mini_app", &self.app_id, &self.secret)
            .stable(self.stable)
            .get()
            .await
        {
            Some(s) => s.access_token,
            None => "".to_owned(),
        }
    }

    /// 发送订阅消息
    pub async fn send(
        &self,
        touser: &str,
        template_id: &str,
        page: &str,
        data: serde_json::Value,
        miniprogram_state: &str,
        lang: &str,
    ) -> WechatResult<serde_json::Value> {
        let access_token = self.get_access_token().await;
        if access_token == "" { return Err(error!("Access token not found")); }
        let url = format!("{}/cgi-bin/message/subscribe/send?access_token={}", API_DOMAIN, access_token);
        let val = json!({
            "touser": touser,
            "template_id": template_id,
            "page": page,
            "data": data,
            "miniprogram_state": miniprogram_state,
            "lang": lang
        });
        let res = Client::new().post(&url, &val).await?;
        wechat_sdk::json_decode(&res)
    }
}
