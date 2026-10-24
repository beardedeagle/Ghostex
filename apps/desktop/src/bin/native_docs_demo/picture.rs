//! A picture in the Files view: the header row with the zoom buttons and the zoomable picture
//! (`native_docs/picture_zoom.rs`), drawn without the app. Opened for an image file.
//!
//!     GHOSTEX_NATIVE_DOCS_DEMO_PICTURE="in in actual ..."   (zoom steps run once laid out: in,
//!                                                            out, opening, whole, actual, and
//!                                                            double-click or pinch at (300, 300))

use std::sync::Arc;

use gpui::{
    App, AppContext as _, Bounds, Context, FontWeight, IntoElement, ParentElement as _, Render,
    SharedString, Styled as _, Window, WindowBounds, WindowOptions, div, point, px, size,
};
use gpui_component::Root;

use crate::app::native_docs::palette::DocsPalette;
use crate::app::native_docs::picture_zoom::{DocsPictureHost, DocsPictureView, PictureZoomCommand};

struct PictureDemo {
    path: String,
    picture: Arc<gpui::Image>,
    view: DocsPictureView,
    palette: DocsPalette,
    steps: Vec<PictureZoomCommand>,
}

impl DocsPictureHost for PictureDemo {
    fn docs_active_picture(&self) -> Option<(String, Arc<gpui::Image>)> {
        Some((self.path.clone(), self.picture.clone()))
    }

    fn docs_picture_view(&mut self, path: &str) -> Option<&mut DocsPictureView> {
        (path == self.path).then_some(&mut self.view)
    }

    fn docs_open_picture_viewer(&mut self, path: &str, _: &mut Context<Self>) {
        eprintln!("open the picture viewer: {path}");
    }
}

impl Render for PictureDemo {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.steps.is_empty() {
            let steps = std::mem::take(&mut self.steps);
            let mut ran = true;
            for step in &steps {
                ran &= self.native_docs_zoom_picture(*step, cx);
            }
            if !ran {
                // Not laid out yet: try again on the next frame.
                self.steps = steps;
                window.request_animation_frame();
            }
        }
        let p = self.palette.clone();
        let tiles = self.render_native_docs_picture_tiles(&p, cx);
        let path = self.path.clone();
        let body = self.render_native_docs_picture(&path, self.picture.clone(), cx);
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(p.page)
            .font_family(p.font.clone())
            .text_color(p.text)
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(px(9.0))
                    .h(px(35.0))
                    .pl(px(13.0))
                    .pr(px(9.0))
                    .border_b_1()
                    .border_color(p.border)
                    .bg(p.chrome)
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_size(px(13.0))
                            .font_weight(FontWeight::MEDIUM)
                            .child(SharedString::from(path)),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .flex()
                            .flex_none()
                            .items_center()
                            .gap(px(2.0))
                            .children(tiles),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .relative()
                    .child(div().absolute().inset_0().child(body)),
            )
    }
}

pub(crate) fn run(file: &std::path::Path, light: bool, width: f32, height: f32) {
    let bytes = std::fs::read(file).expect("read the demo picture");
    let format = match file
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("jpg" | "jpeg") => gpui::ImageFormat::Jpeg,
        Some("gif") => gpui::ImageFormat::Gif,
        Some("webp") => gpui::ImageFormat::Webp,
        Some("svg") => gpui::ImageFormat::Svg,
        _ => gpui::ImageFormat::Png,
    };
    let picture = Arc::new(gpui::Image::from_bytes(format, bytes));
    let path = file
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default();
    let steps: Vec<PictureZoomCommand> = crate::env("GHOSTEX_NATIVE_DOCS_DEMO_PICTURE")
        .split_whitespace()
        .filter_map(|step| match step {
            "in" => Some(PictureZoomCommand::In),
            "out" => Some(PictureZoomCommand::Out),
            "opening" => Some(PictureZoomCommand::Opening),
            "whole" => Some(PictureZoomCommand::Whole),
            "actual" => Some(PictureZoomCommand::Actual),
            // A double-click and a 2x pinch at window point (300, 300).
            "double-click" => Some(PictureZoomCommand::Toggle(point(px(300.0), px(300.0)))),
            "pinch" => Some(PictureZoomCommand::By(2.0, point(px(300.0), px(300.0)))),
            _ => None,
        })
        .collect();
    gpui_platform::application()
        .with_assets(crate::assets::GhostexAssets)
        .run(move |cx: &mut App| {
            gpui_component::init(cx);
            crate::ui_fonts::register(cx);
            let window_size = size(px(width), px(height));
            let bounds = cx
                .primary_display()
                .map(|display| Bounds::centered_at(display.bounds().center(), window_size))
                .unwrap_or_else(|| Bounds::new(point(px(120.0), px(80.0)), window_size));
            let background = crate::env("GHOSTEX_NATIVE_MODAL_DEMO_BACKGROUND") == "1";
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    focus: !background,
                    show: true,
                    ..Default::default()
                },
                move |window, cx| {
                    window.set_window_title("Native Docs picture demo");
                    let palette = DocsPalette::resolve(
                        false,
                        light,
                        crate::ui_fonts::UI_FONT.to_string(),
                        gpui::rgb(if light { 0xffffff } else { 0x0e0e0e }).into(),
                    );
                    let view = cx.new(|_| PictureDemo {
                        path,
                        picture,
                        view: DocsPictureView::default(),
                        palette,
                        steps,
                    });
                    cx.new(|cx| Root::new(view, window, cx))
                },
            )
            .expect("open the demo window");
            cx.activate(!background);
        });
}
