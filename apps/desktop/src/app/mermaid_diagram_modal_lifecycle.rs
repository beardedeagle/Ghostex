//! Open and close plumbing for the native Mermaid diagram popup.
//! SEE-ALSO: apps/desktop/src/app/window/mermaid_diagram_modal.rs (the window entity), apps/desktop/src/app/native_app_modal_lifecycle.rs (the shared window path).
use crate::app::window::*;
use crate::*;

impl GhostexGpuiApp {
    /// Opens the popup for the `mermaidDiagram` open message (its `source` is the diagram's
    /// Mermaid text), from the Docs page and from Markdown in Settings > Extensions.
    pub(crate) fn open_gpui_mermaid_diagram_modal(
        &mut self,
        message: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(source) = message
            .get("source")
            .and_then(serde_json::Value::as_str)
            .filter(|source| !source.trim().is_empty())
            .map(str::to_string)
        else {
            return;
        };
        let palette = self.gpui_native_modal_palette();
        let host = self.native_app_modal_host(cx, |app, command, cx| {
            app.handle_gpui_mermaid_diagram_modal_command(command, cx);
        });
        let kind = GpuiAppModalKind::MermaidDiagram;
        let (width, height) = self.chat_preview_modal_size(cx);
        self.open_native_app_modal(
            kind,
            width,
            height,
            move |window, cx| {
                cx.new(|cx| GpuiMermaidDiagramModalWindow::new(source, palette, host, window, cx))
            },
            cx,
        );
    }

    fn handle_gpui_mermaid_diagram_modal_command(
        &mut self,
        command: MermaidDiagramModalCommand,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.native_app_modal_kind() != Some(GpuiAppModalKind::MermaidDiagram) {
            return;
        }
        match command {
            MermaidDiagramModalCommand::Close => {
                self.close_native_app_modal_from_bridge(cx);
            }
        }
    }
}
