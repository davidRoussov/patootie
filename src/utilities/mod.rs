use url::{Url, ParseError};

pub fn is_absolute_url(url: &str) -> bool {
     Url::parse(url).map(|u| u.has_host()).unwrap_or(false)
}

pub fn is_relative_url(url: &str) -> bool {
    match Url::parse(url) {
        Ok(_) => false,
        Err(ParseError::RelativeUrlWithoutBase) => true,
        Err(_) => false,
    }
}

pub fn get_base_url(url: &str) -> Result<String, ParseError> {
    let parsed_url = Url::parse(url)?;
    let base = parsed_url.join("/").unwrap();
    Ok(base.to_string())
}
