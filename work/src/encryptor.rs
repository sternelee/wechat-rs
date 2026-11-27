//! copyright © ecdata.cn 2021 - present
//! 企业微信消息加解密，对齐 EasyWeChat\Work\Encryptor
//! created by shaipe 20210228

use wechat_sdk::{Encryptor as KernelEncryptor, WechatResult as Result};

/// 企业微信消息加解密器
///
/// 与公众号一致，但 receive_id 为空（企业微信加密串末尾不带 appId）
#[derive(Debug, Clone)]
pub struct Encryptor {
    inner: KernelEncryptor,
}

impl Encryptor {
    /// 创建加解密器
    pub fn new<S: Into<String>>(corp_id: S, token: S, aes_key: S) -> Result<Encryptor> {
        let inner = KernelEncryptor::new(&corp_id.into(), &token.into(), &aes_key.into(), None)?;

        Ok(Encryptor { inner })
    }

    /// 获取内部 Kernel 加解密器
    pub fn inner(&self) -> &KernelEncryptor {
        &self.inner
    }

    /// 获取 token
    pub fn get_token(&self) -> &str {
        self.inner.get_token()
    }

    /// 解密回调消息
    pub fn decrypt(&self, ciphertext: &str, msg_signature: &str, nonce: &str, timestamp: &str) -> Result<String> {
        self.inner.decrypt(ciphertext, msg_signature, nonce, timestamp)
    }

    /// 解密 XML 包体（对齐 decrypt_xml）
    pub fn decrypt_xml(
        &self,
        xml: &str,
        query: &std::collections::HashMap<String, String>,
    ) -> Result<String> {
        self.inner.decrypt_xml(xml, query)
    }

    /// 加密回复消息
    pub fn encrypt(
        &self,
        plaintext: &str,
        nonce: Option<&str>,
        timestamp: Option<u64>,
        message_type: &str,
    ) -> Result<String> {
        self.inner
            .encrypt(plaintext, nonce, timestamp, message_type)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AES_KEY: &str = "kdjCGGJKSRjjhESfPO5lTSWtYS0v5pQX47skCkZczio";

    #[test]
    fn encrypt_then_decrypt() {
        let encryptor = Encryptor::new("corp", "token", AES_KEY).unwrap();
        let xml = encryptor.encrypt("<xml><Content>hi</Content></xml>", None, None, "xml").unwrap();
        assert!(xml.contains("<Encrypt>"));

        let package = wechat_sdk::xmlutil::parse(&xml);
        let doc = package.as_document();
        let ciphertext = wechat_sdk::xmlutil::evaluate(&doc, "//xml/Encrypt/text()").string();
        let signature = wechat_sdk::xmlutil::evaluate(&doc, "//xml/MsgSignature/text()").string();
        let nonce = wechat_sdk::xmlutil::evaluate(&doc, "//xml/Nonce/text()").string();
        let timestamp = wechat_sdk::xmlutil::evaluate(&doc, "//xml/TimeStamp/text()").string();

        let plain = encryptor
            .decrypt(&ciphertext, &signature, &nonce, &timestamp)
            .unwrap();

        assert_eq!(plain, "<xml><Content>hi</Content></xml>");
    }

    #[test]
    fn invalid_aes_key() {
        assert!(Encryptor::new("corp", "token", "short").is_err());
    }
}
