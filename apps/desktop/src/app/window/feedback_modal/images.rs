//! Images pasted or dropped into the feedback message: what the relay accepts, and the thumbnail
//! strip.

use super::state::{FeedbackStep, GpuiFeedbackModalWindow};
use crate::app::window::native_modal_kit::*;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, ClipboardEntry, ClipboardItem, Context, ExternalPaths, Image, ImageFormat,
    InteractiveElement as _, IntoElement, ParentElement as _, StatefulInteractiveElement as _,
    Styled as _, StyledImage as _, WeakEntity, Window, div, img, px,
};
use std::io::Read as _;
use std::path::Path;
use std::sync::Arc;

/// The relay's limits (`LIMITS` in its `src/validate.ts`, checked again by gxserver).
pub(super) const FEEDBACK_MAX_IMAGES: usize = 5;
const FEEDBACK_IMAGE_MAX_BYTES: usize = 5 * 1024 * 1024;
/// The chat composer's thumbnail size.
const TILE_PX: f32 = 48.0;
const TILE_GROUP: &str = "feedback-image";

impl GpuiFeedbackModalWindow {
    /// The message field's paste hook: pasted images are attached, anything else is typed as text.
    /// The chat composer takes images through the same hook, which is how a browser paste reaches
    /// the web build too (native_chat/clipboard.rs).
    pub(super) fn paste_handler(
        modal: WeakEntity<Self>,
    ) -> impl Fn(&ClipboardItem, &mut Window, &mut App) -> bool + 'static {
        move |clipboard, _window, cx| {
            modal
                .update(cx, |modal, cx| modal.paste_images(clipboard, cx))
                .unwrap_or(false)
        }
    }

    fn paste_images(&mut self, clipboard: &ClipboardItem, cx: &mut Context<Self>) -> bool {
        let pasted: Vec<Image> = clipboard
            .entries
            .iter()
            .filter_map(|entry| match entry {
                ClipboardEntry::Image(image) => Some(image.clone()),
                _ => None,
            })
            .collect();
        if pasted.is_empty() {
            return false;
        }
        self.add_images(
            pasted
                .into_iter()
                .map(|image| move || prepare_feedback_image(image))
                .collect(),
            cx,
        );
        true
    }

    /// Image files dropped on the pop-up while the message is being written are attached like
    /// pasted ones: the same five-image limit, formats and 5 MB size (a larger PNG is scaled
    /// down).
    pub(super) fn drop_paths(&mut self, paths: &ExternalPaths, cx: &mut Context<Self>) {
        if self.busy || !matches!(self.step, FeedbackStep::Compose) {
            return;
        }
        self.add_images(
            paths
                .paths()
                .iter()
                .cloned()
                .map(|path| move || read_dropped_image(&path).and_then(prepare_feedback_image))
                .collect(),
            cx,
        );
    }

    /// Prepares each image off the UI thread, with a dashed tile per image still in the works,
    /// until the five-image limit.
    fn add_images<F>(&mut self, loads: Vec<F>, cx: &mut Context<Self>)
    where
        F: FnOnce() -> Result<Image, String> + Send + 'static,
    {
        self.image_error = None;
        for load in loads {
            if self.images.len() + self.pending_images >= FEEDBACK_MAX_IMAGES {
                self.image_error = Some(format!(
                    "You can attach up to {FEEDBACK_MAX_IMAGES} images."
                ));
                break;
            }
            self.pending_images += 1;
            let prepared = cx.background_executor().spawn(async move { load() });
            cx.spawn(async move |this, cx| {
                let prepared = prepared.await;
                let _ = this.update(cx, |this, cx| {
                    this.pending_images -= 1;
                    match prepared {
                        Ok(image) => this.images.push(Arc::new(image)),
                        Err(error) => this.image_error = Some(error),
                    }
                    cx.notify();
                });
            })
            .detach();
        }
        cx.notify();
    }

    /// The pasted images as 48px tiles, each with a remove button that shows while it is hovered,
    /// plus a dashed tile per image still being prepared.
    pub(super) fn render_images(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.images.is_empty() && self.pending_images == 0 {
            return None;
        }
        let p = self.palette;
        let mut row = div()
            .id("feedback-images")
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(8.0))
            .pt(px(4.0));
        for (index, image) in self.images.iter().enumerate() {
            row = row.child(
                div()
                    .relative()
                    .flex_shrink_0()
                    .group(TILE_GROUP)
                    .size(px(TILE_PX))
                    .child(
                        div()
                            .size_full()
                            .rounded(px(8.0))
                            .overflow_hidden()
                            .border_1()
                            .border_color(hsla(p.hairline))
                            .child(
                                img(image.clone())
                                    .size_full()
                                    .object_fit(gpui::ObjectFit::Cover),
                            ),
                    )
                    .child(
                        div()
                            .id(("feedback-image-remove", index))
                            .role(gpui::Role::Button)
                            .aria_label(format!("Remove image {}", index + 1))
                            .invisible()
                            .group_hover(TILE_GROUP, |style| style.visible())
                            .absolute()
                            .top(px(-5.0))
                            .right(px(-5.0))
                            .size(px(16.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_full()
                            .border_1()
                            .border_color(hsla(p.hairline))
                            .bg(hsla(p.solid_surface))
                            .cursor_pointer()
                            .hover(move |style| style.visible().bg(hsla(p.raised_hover)))
                            .when(!self.busy, |button| {
                                button.on_press(cx, move |this, _window, cx| {
                                    if index < this.images.len() {
                                        this.images.remove(index);
                                        this.image_error = None;
                                        cx.notify();
                                    }
                                })
                            })
                            .child(modal_icon("modals/kit/x.svg", 9.0, p.muted)),
                    ),
            );
        }
        for _ in 0..self.pending_images {
            row = row.child(
                div()
                    .flex_shrink_0()
                    .size(px(TILE_PX))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(8.0))
                    .border_dashed()
                    .border_1()
                    .border_color(hsla(p.hairline))
                    .child(modal_spinner(p.muted)),
            );
        }
        Some(row.into_any_element())
    }
}

/// Larger dropped files are not screenshots; refusing them up front spares reading a video into
/// memory to find that out.
const DROPPED_FILE_MAX_BYTES: u64 = 64 * 1024 * 1024;

/// A dropped file as an image, its type read from the bytes rather than the file name.
fn read_dropped_image(path: &Path) -> Result<Image, String> {
    let unreadable = || format!("{} could not be read.", file_label(path));
    let not_an_image = || "Only PNG, JPEG and WebP images can be attached.".to_string();
    let mut file = std::fs::File::open(path).map_err(|_| unreadable())?;
    let metadata = file.metadata().map_err(|_| unreadable())?;
    if !metadata.is_file() {
        return Err(not_an_image());
    }
    let mut head = [0u8; 16];
    let read = file.read(&mut head).map_err(|_| unreadable())?;
    let format = match image::guess_format(&head[..read]) {
        Ok(image::ImageFormat::Png) => ImageFormat::Png,
        Ok(image::ImageFormat::Jpeg) => ImageFormat::Jpeg,
        Ok(image::ImageFormat::WebP) => ImageFormat::Webp,
        _ => return Err(not_an_image()),
    };
    if metadata.len() > DROPPED_FILE_MAX_BYTES {
        return Err("Images must be 5 MB or smaller.".to_string());
    }
    let bytes = std::fs::read(path).map_err(|_| unreadable())?;
    Ok(Image::from_bytes(format, bytes))
}

fn file_label(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "That file".to_string())
}

/// An image the relay accepts: PNG, JPEG or WebP of at most 5 MB. A larger PNG (a full-screen
/// Retina screenshot is often 6-12 MB) is scaled down until it fits instead of being refused.
fn prepare_feedback_image(image: Image) -> Result<Image, String> {
    match image.format() {
        ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::Webp => {}
        _ => return Err("Only PNG, JPEG and WebP images can be attached.".to_string()),
    }
    if image.bytes().len() <= FEEDBACK_IMAGE_MAX_BYTES {
        return Ok(image);
    }
    let too_large = || "Images must be 5 MB or smaller.".to_string();
    if image.format() != ImageFormat::Png {
        return Err(too_large());
    }
    let decoded = image::load_from_memory_with_format(image.bytes(), image::ImageFormat::Png)
        .map_err(|_| "That image could not be read.".to_string())?;
    // PNG size grows roughly with the pixel count, so the first try aims straight at the limit.
    let mut scale = (FEEDBACK_IMAGE_MAX_BYTES as f64 / image.bytes().len() as f64).sqrt() * 0.9;
    for _ in 0..4 {
        let width = ((decoded.width() as f64) * scale).round().max(1.0) as u32;
        let height = ((decoded.height() as f64) * scale).round().max(1.0) as u32;
        let mut encoded = std::io::Cursor::new(Vec::new());
        decoded
            .resize(width, height, image::imageops::FilterType::Triangle)
            .write_to(&mut encoded, image::ImageFormat::Png)
            .map_err(|_| too_large())?;
        let bytes = encoded.into_inner();
        if bytes.len() <= FEEDBACK_IMAGE_MAX_BYTES {
            return Ok(Image::from_bytes(ImageFormat::Png, bytes));
        }
        scale *= 0.75;
    }
    Err(too_large())
}
