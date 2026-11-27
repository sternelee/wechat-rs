//! copyright © ecdata.cn 2021 - present
//! 小程序后端接口对接配置信息
//! created by shaipe 20210228

use std::fs::File;
use wechat_sdk::WechatResult;

/// 配置信息结构体，对齐 EasyWeChat\MiniApp\Config
#[derive(Debug, Clone, Default)]
pub struct WeappConfig {
    /// 应用id
    pub app_id: String,
    /// 应用密钥
    pub secret: String,
    /// 小程序名称
    pub name: String,
    /// 消息推送 Token
    pub token: String,
    /// 消息加解密 EncodingAESKey
    pub aes_key: String,
    /// 是否强制要求加密消息，对齐 PHP `require_encryption`
    pub require_encryption: bool,
    /// 是否使用稳定版 access_token
    pub use_stable_access_token: bool,
}

impl WeappConfig {
    /// 创建配置
    pub fn new<S: Into<String>>(app_id: S, secret: S) -> WeappConfig {
        WeappConfig {
            app_id: app_id.into(),
            secret: secret.into(),
            ..Default::default()
        }
    }

    /// 设置消息 token/aes_key
    pub fn with_message<S: Into<String>>(mut self, token: S, aes_key: S) -> Self {
        self.token = token.into();
        self.aes_key = aes_key.into();
        self
    }

    /// 使用稳定版 access_token
    pub fn stable_access_token(mut self, stable: bool) -> Self {
        self.use_stable_access_token = stable;
        self
    }

    /// 是否强制要求加密消息
    pub fn require_encryption(mut self, require: bool) -> Self {
        self.require_encryption = require;
        self
    }

    /// 从 JSON 配置构建，对齐 easywechat 配置键名
    pub fn from_value(value: &serde_json::Value) -> WechatResult<WeappConfig> {
        let app_id = value["app_id"].as_str().unwrap_or_default();
        if app_id.is_empty() {
            return Err(error! {code: -1, msg: "Missing config: app_id"});
        }

        Ok(WeappConfig {
            app_id: app_id.to_owned(),
            secret: value["secret"].as_str().unwrap_or_default().to_owned(),
            name: value["name"].as_str().unwrap_or_default().to_owned(),
            token: value["token"].as_str().unwrap_or_default().to_owned(),
            aes_key: value["aes_key"].as_str().unwrap_or_default().to_owned(),
            require_encryption: value["require_encryption"].as_bool().unwrap_or_default(),
            use_stable_access_token: value["use_stable_access_token"]
                .as_bool()
                .unwrap_or_default(),
        })
    }

    /// 是否配置了消息加密
    pub fn has_encryption(&self) -> bool {
        !self.token.is_empty() && !self.aes_key.is_empty()
    }

    /// 加载yml配置文件
    pub fn load_yaml(conf_path: &str) -> WechatResult<WeappConfig> {
        use yaml_rust::yaml;
        // open file
        let mut f = match File::open(conf_path) {
            Ok(f) => f,
            Err(e) => {
                return Err(error! {
                    code: 4004,
                    msg: format!("{}", e)
                });
            }
        };
        let mut s = String::new();
        use std::io::Read;
        match f.read_to_string(&mut s) {
            Ok(s) => s,
            Err(e) => {
                return Err(error! {
                    code: 4004,
                    msg: format!("Error Reading file: {}", e)
                });
            }
        };
        // load string to yaml loader
        let docs = yaml::YamlLoader::load_from_str(&s).unwrap();
        // get first yaml hash doc
        let yaml_doc = &docs[0];
        // get server value
        let server = yaml_doc["weapp"].clone();

        Ok(WeappConfig::load_yaml_node(&server))
    }

    /// 根据yaml配置点进加载配置
    /// @param1: yaml 配置节点
    pub fn load_yaml_node(conf_node: &yaml_rust::yaml::Yaml) -> WeappConfig {
        WeappConfig {
            app_id: conf_node["app_id"].as_str().unwrap_or_default().to_owned(),
            name: conf_node["name"].as_str().unwrap_or_default().to_owned(),
            secret: conf_node["secret"].as_str().unwrap_or_default().to_owned(),
            token: conf_node["token"].as_str().unwrap_or_default().to_owned(),
            aes_key: conf_node["aes_key"].as_str().unwrap_or_default().to_owned(),
            require_encryption: conf_node["require_encryption"].as_bool().unwrap_or(false),
            use_stable_access_token: conf_node["use_stable_access_token"]
                .as_bool()
                .unwrap_or(false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_from_value() {
        let conf = WeappConfig::from_value(&serde_json::json!({
            "app_id": "wx1",
            "secret": "s",
            "token": "t",
            "aes_key": "a",
            "require_encryption": true,
            "use_stable_access_token": true
        }))
        .unwrap();

        assert_eq!(conf.app_id, "wx1");
        assert!(conf.has_encryption());
        assert!(conf.require_encryption);
        assert!(conf.use_stable_access_token);
    }

    #[test]
    fn missing_app_id() {
        assert!(WeappConfig::from_value(&serde_json::json!({})).is_err());
    }
}
