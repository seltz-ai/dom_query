/// `CommonMark` ordered-list markers have at most nine digits; a longer number
/// turns the marker into plain paragraph text.
pub(super) const MAX_LIST_NUMBER: u32 = 999_999_999;

pub(super) const DEFAULT_SKIP_TAGS: [&str; 6] = ["script", "style", "meta", "head", "title", "svg"];
pub(super) const CODE_LANGUAGE_ATTRIBUTES: [&str; 2] = ["data-lang", "data-language"];
pub(super) const CODE_LANGUAGE_PREFIX: &str = "language-";
