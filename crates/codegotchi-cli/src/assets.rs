#[derive(Clone, Copy, Debug)]
pub struct EmbeddedAsset {
    pub path: &'static str,
    pub content_type: &'static str,
    pub bytes: &'static [u8],
}

include!(concat!(env!("OUT_DIR"), "/web_assets.rs"));

pub fn find(path: &str) -> Option<&'static EmbeddedAsset> {
    EMBEDDED_ASSETS.iter().find(|asset| asset.path == path)
}

pub fn index() -> &'static EmbeddedAsset {
    find("/index.html").expect("web-dist must contain index.html")
}
