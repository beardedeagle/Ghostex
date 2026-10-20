//! Chat visual pages: self-contained HTML pages an agent publishes with `ghostex show`, kept under
//! the gxserver state directory and served at `/visual/<id>` so a chat card can open them in a
//! browser page. `prepare` readies the page on the CLI side (local images, theme, CSP, the question
//! kit), `publish` is the authenticated route that stores it, `serve` answers the page itself.

mod answers;
mod drafts;
mod kit;
mod prepare;
mod publish;
mod questions;
mod serve;
mod store;
mod theme;
mod thumbnail;

pub(crate) use answers::{is_answers_route, submit as submit_answers};
pub(crate) use drafts::{is_draft_route, save as save_draft};
pub(crate) use prepare::{page_too_large, prepare_page};
pub(crate) use publish::{
    handle_publish_visual_page_http, PUBLISH_VISUAL_PAGE_BODY_LIMIT_BYTES,
    PUBLISH_VISUAL_PAGE_ENDPOINT,
};
pub(crate) use questions::{page_answered, page_questions};
pub(crate) use serve::{serve, VISUAL_PAGE_ROUTE_PREFIX};
pub(crate) use store::VISUAL_PAGE_MAX_BYTES;
pub(crate) use thumbnail::{
    handle_read_http as handle_read_visual_page_thumbnail_http,
    handle_store_http as handle_store_visual_page_thumbnail_http,
    READ_VISUAL_PAGE_THUMBNAIL_ENDPOINT, STORE_VISUAL_PAGE_THUMBNAIL_ENDPOINT,
};
