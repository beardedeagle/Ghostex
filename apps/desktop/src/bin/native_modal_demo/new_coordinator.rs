//! New Orchestrator preview. States (`GHOSTEX_NATIVE_MODAL_DEMO_STATE`): default (Claude
//! selected, Opus 5.5 at XHigh), `codex`, `cursor` (an agent whose launch takes no model),
//! `antigravity` (no XHigh: Medium and High), `none` (no agent an orchestrator can run on).
use super::new_coordinator_modal::*;
use gpui::{App, AppContext as _};
use std::rc::Rc;

fn efforts(values: &[&str]) -> Vec<(String, String)> {
    values
        .iter()
        .map(|value| {
            let label = match *value {
                "xhigh" => "XHigh".to_string(),
                other => {
                    let mut chars = other.chars();
                    chars.next().map_or_else(String::new, |first| {
                        first.to_uppercase().collect::<String>() + chars.as_str()
                    })
                }
            };
            (value.to_string(), label)
        })
        .collect()
}

fn model(value: &str, label: &str, levels: &[&str], default: bool) -> NewCoordinatorModel {
    NewCoordinatorModel {
        value: value.to_string(),
        label: label.to_string(),
        efforts: efforts(levels),
        default,
    }
}

fn agent(id: &str, name: &str, models: Vec<NewCoordinatorModel>) -> NewCoordinatorAgent {
    NewCoordinatorAgent {
        agent_id: id.to_string(),
        name: name.to_string(),
        models,
        skill_line: ghostex_gx_chat_core::composer::skill_invocation::orchestrator_skill_line(
            Some(id),
        ),
    }
}

pub(super) fn open(demo: &super::DemoEnv, cx: &mut App) {
    let claude_levels = ["low", "medium", "high", "xhigh", "max", "ultracode"];
    let agents = if demo.state == "none" {
        Vec::new()
    } else {
        vec![
            agent(
                "claude",
                "Claude",
                vec![
                    model("fable", "Fable 5.1", &claude_levels, false),
                    model("opus[1m]", "Opus 5.5", &claude_levels, true),
                    model("sonnet", "Sonnet 5.5", &claude_levels, false),
                ],
            ),
            agent(
                "codex",
                "Codex",
                vec![model(
                    "gpt-6.1-sol",
                    "GPT-6.1 Sol",
                    &["low", "medium", "high", "xhigh", "max", "ultra"],
                    true,
                )],
            ),
            agent(
                "zcode",
                "ZCode",
                vec![model("zai/glm-5.3", "GLM 5.3", &[], true)],
            ),
            agent("cursor", "Cursor CLI", Vec::new()),
            agent("grok", "Grok Build", Vec::new()),
            agent(
                "antigravity",
                "Antigravity CLI",
                vec![model(
                    "gemini-4-argon",
                    "Gemini 4 Argon",
                    &["low", "medium", "high"],
                    true,
                )],
            ),
            agent("pi", "Pi Agent", Vec::new()),
            agent("omp", "OMP", Vec::new()),
        ]
    };
    let selected_agent = match demo.state.as_str() {
        "codex" => 1,
        "cursor" => 3,
        "antigravity" => 5,
        _ => 0,
    };
    let host: NewCoordinatorModalHost = Rc::new(|command, cx: &mut App| {
        match command {
            NewCoordinatorModalCommand::Create {
                agent_id,
                name,
                model,
                effort,
            } => eprintln!("create: {agent_id} {name:?} {model:?} {effort:?}"),
            NewCoordinatorModalCommand::Cancel => eprintln!("cancel"),
        }
        cx.quit();
    });
    let config = NewCoordinatorModalConfig {
        project_name: "Ghostex".to_string(),
        agents,
        selected_agent,
        palette: demo.palette,
    };
    let _ = super::open_modal_window(
        NEW_COORDINATOR_MODAL_WIDTH,
        NEW_COORDINATOR_MODAL_INITIAL_HEIGHT,
        move |window, cx| cx.new(|cx| GpuiNewCoordinatorModalWindow::new(config, host, window, cx)),
        cx,
    );
}
