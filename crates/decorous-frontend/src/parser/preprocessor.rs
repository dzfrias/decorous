pub trait Preprocessor {
    fn preprocess_js(&self, _lang: &str, _body: &str) -> Option<String> {
        None
    }
    fn preprocess_css(&self, _lang: &str, _body: &str) -> Option<String> {
        None
    }
}

impl<T> Preprocessor for &T
where
    T: Preprocessor,
{
    fn preprocess_css(&self, lang: &str, body: &str) -> Option<String> {
        (*self).preprocess_css(lang, body)
    }
    fn preprocess_js(&self, lang: &str, body: &str) -> Option<String> {
        (*self).preprocess_js(lang, body)
    }
}
