pub fn get_url_encode(c:&str)->String{
    use percent_encoding::{utf8_percent_encode, AsciiSet, CONTROLS};
    const FRAGMENT: &AsciiSet = &CONTROLS
        .add(b' ')
        .add(b'"')
        .add(b'<')
        .add(b'>')
        .add(b'`')
        .add(b'+')
        .add(b'=')
        // .add(b'/')
        ;
        utf8_percent_encode(c, FRAGMENT).to_string()
}
/// 对齐 PHP `rawurlencode`：除 `A-Z a-z 0-9 - _ . ~` 外全部做百分号编码
///
/// 授权页的 `redirect_uri` 必须用它（`https://a.b/cb` → `https%3A%2F%2Fa.b%2Fcb`），
/// 否则回调地址里的 `?` / `&` 会破坏外层查询串。
pub fn raw_url_encode(input: &str) -> String {
    use percent_encoding::{utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};

    const RAW_URL_ENCODE_SET: &AsciiSet = &NON_ALPHANUMERIC
        .remove(b'-')
        .remove(b'.')
        .remove(b'_')
        .remove(b'~');

    utf8_percent_encode(input, RAW_URL_ENCODE_SET).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_url_encode_matches_php_rawurlencode() {
        assert_eq!(
            raw_url_encode("https://a.b/cb?x=1&y=2"),
            "https%3A%2F%2Fa.b%2Fcb%3Fx%3D1%26y%3D2"
        );
        assert_eq!(raw_url_encode("a-b_c.d~e"), "a-b_c.d~e");
        assert_eq!(raw_url_encode("中文"), "%E4%B8%AD%E6%96%87");
    }
}
