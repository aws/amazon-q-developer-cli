use std::borrow::Cow;
use std::error::Error;

use anyhow::Result;
use image::Rgb;
use wry::http::header::CONTENT_TYPE;
use wry::http::{
    Response,
    StatusCode,
};

pub fn res_404() -> Result<Response<Cow<'static, [u8]>>> {
    Ok(Response::builder()
        .status(StatusCode::NOT_FOUND)
        .header(CONTENT_TYPE, "text/plain")
        .body(b"Not Found".as_ref().into())?)
}

pub fn res_400() -> Result<Response<Cow<'static, [u8]>>> {
    Ok(Response::builder()
        .status(StatusCode::BAD_REQUEST)
        .header(CONTENT_TYPE, "text/plain")
        .body(b"Bad Request".as_ref().into())?)
}

pub fn res_500(err: impl Error) -> Result<Response<Cow<'static, [u8]>>> {
    Ok(Response::builder()
        .status(StatusCode::INTERNAL_SERVER_ERROR)
        .header(CONTENT_TYPE, "text/plain")
        .body(err.to_string().into_bytes().into())?)
}

pub fn parse_hex_rgb(s: &str) -> Option<Rgb<u8>> {
    if s.len() != 6 {
        return None;
    }
    let radix = 16;
    Some(Rgb([
        u8::from_str_radix(&s[0..2], radix).ok()?,
        u8::from_str_radix(&s[2..4], radix).ok()?,
        u8::from_str_radix(&s[4..6], radix).ok()?,
    ]))
}

pub const fn scale_u8(a: u8, b: u8) -> u8 {
    ((a as u16 * b as u16) / 256) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hex_color() {
        assert_eq!(parse_hex_rgb("000000"), Some(Rgb([0, 0, 0])));
        assert_eq!(parse_hex_rgb("ffffff"), Some(Rgb([255, 255, 255])));
        assert_eq!(parse_hex_rgb("ff0000"), Some(Rgb([255, 0, 0])));
        assert_eq!(parse_hex_rgb("00ff00"), Some(Rgb([0, 255, 0])));
        assert_eq!(parse_hex_rgb("0000ff"), Some(Rgb([0, 0, 255])));
        assert_eq!(parse_hex_rgb("00000f"), Some(Rgb([0, 0, 15])));
        assert_eq!(parse_hex_rgb("00000"), None);
        assert_eq!(parse_hex_rgb("0000000"), None);
        assert_eq!(parse_hex_rgb("00000g"), None);
    }

    #[test]
    fn test_scale() {
        assert_eq!(scale_u8(0, 0), 0);
        assert_eq!(scale_u8(128, 0), 0);
        assert_eq!(scale_u8(255, 0), 0);
        assert_eq!(scale_u8(128, 128), 64);
        assert_eq!(scale_u8(255, 255), 254);
    }
}
