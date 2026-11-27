//! copyright © ecdata.cn 2021 - present
//! 小程序工具，对齐 EasyWeChat\MiniApp\Utils
//! created by shaipe 20210228

use crate::application::Application;
use crate::decryptor::Decryptor;
use serde_json::Value;
use wechat_sdk::WechatResult as Result;

/// 小程序工具
pub struct Utils<'a> {
    app: &'a Application,
}

impl<'a> Utils<'a> {
    /// 创建工具
    pub fn new(app: &'a Application) -> Utils<'a> {
        Utils { app }
    }

    /// 登录凭证校验，对齐 Utils::codeToSession
    ///
    /// 返回 { openid, session_key, unionid, errcode, errmsg }
    pub async fn code_to_session(&self, code: &str) -> Result<Value> {
        let account = self.app.account();
        let res = self
            .app
            .get_without_token(
                "/sns/jscode2session",
                &[
                    ("appid", account.app_id.as_str()),
                    ("secret", account.secret.as_str()),
                    ("js_code", code),
                    ("grant_type", "authorization_code"),
                ],
            )
            .await?;

        if res["openid"].as_str().unwrap_or_default().is_empty() {
            return Err(error! {
                code: -1,
                msg: format!("code2Session error: {}", res)
            });
        }

        Ok(res)
    }

    /// 解密用户数据，对齐 Utils::decryptSession
    pub fn decrypt_session(&self, session_key: &str, iv: &str, ciphertext: &str) -> Result<Value> {
        Decryptor::decrypt(session_key, iv, ciphertext)
    }

    /// 通过 code 获取手机号，对齐 Utils::getPhoneNumber
    ///
    /// POST /wxa/business/getuserphonenumber
    pub async fn get_phone_number(&self, code: &str) -> Result<Value> {
        let res = self
            .app
            .post("/wxa/business/getuserphonenumber", &serde_json::json!({ "code": code }))
            .await?;

        if res["errcode"].as_i64().unwrap_or(0) != 0 {
            return Err(error! {
                code: res["errcode"].as_i64().unwrap_or(-1) as i32,
                msg: format!("getPhoneNumber error: {}", res)
            });
        }

        if res["phone_info"].is_null() {
            return Err(error! {
                code: -1,
                msg: format!("getPhoneNumber error: {}", res)
            });
        }

        Ok(res)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::WeappConfig;

    #[test]
    fn utils_can_be_created() {
        let app = Application::new(WeappConfig::new("wx1", "secret"));
        let utils = app.utils();
        assert!(utils.decrypt_session("bad", "bad", "bad").is_err());
    }
}
