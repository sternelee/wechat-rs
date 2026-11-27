//! copyright © ecdata.cn 2021 - present
//! multipart/form-data 表单，对齐 EasyWeChat\Kernel\Form\Form 与 File

use std::fs;
use std::path::Path;

use crate::{WechatResult as Result};

/// 表单文件
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct File {
    /// 表单字段名
    pub field: String,
    /// 本地路径
    pub path: String,
    /// 文件名，默认取路径文件名
    pub name: String,
    /// MIME 类型
    pub mime: String,
    /// 直接给出的内容（对齐 PHP `File::from($contents)`），非空时忽略 path
    pub contents: Option<Vec<u8>>,
}

impl File {
    /// 创建文件
    pub fn new<S: Into<String>>(field: S, path: S) -> File {
        let path = path.into();
        let name = Path::new(&path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("file")
            .to_owned();

        File {
            field: field.into(),
            path,
            name,
            mime: "application/octet-stream".to_owned(),
            contents: None,
        }
    }

    /// 直接使用内容创建文件，对齐 PHP `File::from($contents)`
    pub fn from_bytes<S: Into<String>>(field: S, name: S, contents: Vec<u8>) -> File {
        File {
            field: field.into(),
            path: String::new(),
            name: name.into(),
            mime: "application/octet-stream".to_owned(),
            contents: Some(contents),
        }
    }

    /// 设置文件名
    pub fn with_name<S: Into<String>>(mut self, name: S) -> Self {
        self.name = name.into();
        self
    }

    /// 设置 MIME 类型
    pub fn with_mime<S: Into<String>>(mut self, mime: S) -> Self {
        self.mime = mime.into();
        self
    }

    /// 读取文件内容
    pub fn contents(&self) -> Result<Vec<u8>> {
        if let Some(contents) = &self.contents {
            return Ok(contents.clone());
        }

        fs::read(&self.path).map_err(|e| {
            error! {
                code: -1,
                msg: format!("Read file `{}` failed: {}", self.path, e),
            }
        })
    }
}

/// multipart/form-data 表单
#[derive(Debug, Clone, Default)]
pub struct Form {
    fields: Vec<(String, String)>,
    files: Vec<File>,
    /// 带 content-type 的数据段，对齐 PHP DataPart（如 pay 的 meta JSON 段）
    parts: Vec<(String, String, String)>,
    boundary: Option<String>,
}

impl Form {
    /// 创建表单
    pub fn new() -> Form {
        Form::default()
    }

    /// 添加普通字段
    pub fn field<S: Into<String>>(mut self, name: S, value: S) -> Self {
        self.fields.push((name.into(), value.into()));
        self
    }

    /// 添加文件
    pub fn file(mut self, file: File) -> Self {
        self.files.push(file);
        self
    }

    /// 添加带 content-type 的数据段，对齐 PHP `new DataPart($body, null, 'application/json')`
    pub fn content_part<S: Into<String>, V: Into<String>, C: Into<String>>(
        mut self,
        name: S,
        value: V,
        content_type: C,
    ) -> Self {
        self.parts
            .push((name.into(), value.into(), content_type.into()));
        self
    }

    /// 数据段
    pub fn parts(&self) -> &[(String, String, String)] {
        &self.parts
    }

    /// 设置分隔符
    pub fn with_boundary<S: Into<String>>(mut self, boundary: S) -> Self {
        self.boundary = Some(boundary.into());
        self
    }

    /// 分隔符，默认随机生成
    pub fn boundary(&self) -> String {
        match &self.boundary {
            Some(boundary) => boundary.clone(),
            None => format!("----wechat-rs-{}", crate::strutil::random(16)),
        }
    }

    /// 普通字段
    pub fn fields(&self) -> &[(String, String)] {
        &self.fields
    }

    /// 文件字段
    pub fn files(&self) -> &[File] {
        &self.files
    }

    /// 生成 content-type 与请求体
    pub fn build(&self) -> Result<(String, Vec<u8>)> {
        let boundary = self.boundary();
        let mut body: Vec<u8> = Vec::new();

        for (name, value) in &self.fields {
            body.extend_from_slice(
                format!("--{}\r\nContent-Disposition: form-data; name=\"{}\"\r\n\r\n", boundary, name)
                    .as_bytes(),
            );
            body.extend_from_slice(value.as_bytes());
            body.extend_from_slice(b"\r\n");
        }

        for (name, value, content_type) in &self.parts {
            body.extend_from_slice(
                format!(
                    "--{}\r\nContent-Disposition: form-data; name=\"{}\"\r\nContent-Type: {}\r\n\r\n",
                    boundary, name, content_type
                )
                .as_bytes(),
            );
            body.extend_from_slice(value.as_bytes());
            body.extend_from_slice(b"\r\n");
        }

        for file in &self.files {
            let contents = file.contents()?;
            body.extend_from_slice(
                format!(
                    "--{}\r\nContent-Disposition: form-data; name=\"{}\"; filename=\"{}\"\r\nContent-Type: {}\r\n\r\n",
                    boundary, file.field, file.name, file.mime
                )
                .as_bytes(),
            );
            body.extend_from_slice(&contents);
            body.extend_from_slice(b"\r\n");
        }

        body.extend_from_slice(format!("--{}--\r\n", boundary).as_bytes());

        Ok((format!("multipart/form-data; boundary={}", boundary), body))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn builds_fields_only() {
        let (content_type, body) = Form::new()
            .field("type", "image")
            .with_boundary("BOUNDARY")
            .build()
            .unwrap();

        assert_eq!(content_type, "multipart/form-data; boundary=BOUNDARY");

        let text = String::from_utf8(body).unwrap();
        assert!(text.contains("name=\"type\""));
        assert!(text.contains("image"));
        assert!(text.trim_end().ends_with("--BOUNDARY--"));
    }

    #[test]
    fn builds_with_file() {
        let path = std::env::temp_dir().join("wechat_rs_form_test.txt");
        {
            let mut file = std::fs::File::create(&path).unwrap();
            file.write_all(b"hello").unwrap();
        }

        let file = File::new(
            "media",
            path.to_str().unwrap(),
        )
        .with_mime("text/plain");

        let (_, body) = Form::new()
            .field("type", "file")
            .file(file)
            .with_boundary("BOUNDARY")
            .build()
            .unwrap();

        let text = String::from_utf8_lossy(&body).to_string();
        assert!(text.contains("filename=\"wechat_rs_form_test.txt\""));
        assert!(text.contains("Content-Type: text/plain"));
        assert!(text.contains("hello"));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn missing_file_reports_error() {
        let form = Form::new().file(File::new("media", "/nonexistent/path.png"));
        assert!(form.build().is_err());
    }
}
