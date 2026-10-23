//! The truth table of `empty_tab_list_confirmed`: when the store lets an empty workspace tab list
//! clear a project's tabs, and when it refuses.
//!
//! **What this guards.** The old runtime posts the tab list of the project it holds active, and an
//! empty one makes the desktop workspace drop every restored tab, split and session mapping (the
//! "different session after restart" failure of 2026-09-04). The store is a second reader of the
//! same daemon, so the clear is only believed when the store's own rows agree, and the rule must
//! be able to refuse MORE lists than the old one, never fewer.
//!
//! **The case that is easy to get wrong.** A project's own tab list leaves out the sessions of its
//! user-made groups, so the two lists below are not the same question: a user-made group with no
//! members (what closing the last row of a group leaves behind) is empty while the project it
//! belongs to still has every one of its own rows. Judging by the group alone confirmed a clear
//! the store's rows contradicted, which is why `P-kept` is in the table twice over: once as the
//! answer the rule must give, and once as the answer the mutation gives instead.
//!
//! The table is asserted here rather than compared with TypeScript: this decision has no
//! TypeScript half, it is the Rust store's own guard over the old runtime's payload.
//!
//!   cargo run --release --example empty_tab_list_guard             # from packages/gx-core
//!   cargo run --release --example empty_tab_list_guard -- <out-dir>            # with a dump
//!   EMPTY_TAB_LIST_MUTATION=judge-the-group-alone cargo run --release --example empty_tab_list_guard
//!   EMPTY_TAB_LIST_MUTATION=believe-a-missing-list cargo run --release --example empty_tab_list_guard

use std::process::ExitCode;

use ghostex_gx_core::{
    empty_tab_list_confirmed, ActiveGroup, Core, Event, Intent, Loadable, MachineId,
    PresentationStore, ProjectKey, SideStateUpdate,
};
use serde_json::{json, Value};

const NOW_MS: u64 = 1_790_000_000_000;
/// The machine that mirrors this computer's projects, so the rule is asserted for a remote project
/// as well: the same code decides both, and the user-made groups of a remote project live in THIS
/// computer's document under the machine-scoped project id.
const MACHINE: &str = "remote-ab12";
/// A machine no snapshot ever arrived for: every list of it is `NotLoaded`.
const OFFLINE_MACHINE: &str = "remote-ef56";

fn main() -> ExitCode {
    let out_dir = std::env::args().nth(1).unwrap_or_default();
    let mutation = std::env::var("EMPTY_TAB_LIST_MUTATION").unwrap_or_default();
    if !matches!(
        mutation.as_str(),
        "" | "judge-the-group-alone" | "believe-a-missing-list"
    ) {
        eprintln!("unknown EMPTY_TAB_LIST_MUTATION: {mutation}");
        return ExitCode::from(2);
    }
    let core = seeded_core();
    let store = core.presentation();

    let mut rows = Vec::new();
    let mut failures = Vec::new();
    let mut confirmed = 0usize;
    let mut refused = 0usize;
    for (name, group, expected) in cases() {
        let answer = decide(store, &group, &mutation);
        match answer {
            true => confirmed += 1,
            false => refused += 1,
        }
        if answer != expected {
            failures.push(format!(
                "{name}: expected {expected}, got {answer} (group {})",
                group.to_sidebar_group_id()
            ));
        }
        rows.push(json!({
            "case": name,
            "group": group.to_sidebar_group_id(),
            "groupTabs": list_shape(store, &group),
            "projectTabs": project_list_shape(store, &group),
            "expected": expected,
            "answer": answer,
        }));
    }

    if !out_dir.is_empty() {
        let path = std::path::Path::new(&out_dir).join("empty-tab-list.json");
        if let Err(error) = std::fs::create_dir_all(&out_dir).and_then(|()| {
            std::fs::write(
                &path,
                serde_json::to_string(&json!({ "mutation": mutation, "cases": rows }))
                    .expect("serialize"),
            )
        }) {
            eprintln!("could not write {}: {error}", path.display());
            return ExitCode::FAILURE;
        }
        println!("table written to {}", path.display());
    }

    println!(
        "empty tab list guard: {} cases, {confirmed} confirmed, {refused} refused{}",
        rows.len(),
        match mutation.is_empty() {
            true => String::new(),
            false => format!(" (mutation {mutation})"),
        }
    );
    // A run in which nothing is ever confirmed proves nothing: the rule would pass the table by
    // refusing everything, and the guard it stands for would have stopped working.
    if confirmed == 0 || refused == 0 {
        eprintln!("the table no longer holds both answers: it would pass on a rule that always says the same thing");
        return ExitCode::FAILURE;
    }
    if failures.is_empty() {
        return ExitCode::SUCCESS;
    }
    for failure in &failures {
        eprintln!("DIFFERENCE {failure}");
    }
    eprintln!("{} case(s) disagree with the table", failures.len());
    ExitCode::FAILURE
}

/// The decision under test, with the two ways of getting it wrong that this table exists to fail
/// on: judging a user-made group without its project's own list, and reading a list that is not
/// `Loaded` as "no tabs".
fn decide(store: &PresentationStore, group: &ActiveGroup, mutation: &str) -> bool {
    let empty = |group: &ActiveGroup| matches!(store.tab_sessions(group), Loadable::Loaded(tabs) if tabs.is_empty());
    match mutation {
        "judge-the-group-alone" => empty(group),
        "believe-a-missing-list" => {
            !matches!(store.tab_sessions(group), Loadable::Loaded(tabs) if !tabs.is_empty())
        }
        _ => empty_tab_list_confirmed(store, group),
    }
}

/// Every case, for this computer and for the remote machine that mirrors it: the group the store
/// judges by, and whether an empty tab list for its project may clear the workspace.
fn cases() -> Vec<(String, ActiveGroup, bool)> {
    let mut cases = Vec::new();
    for machine in [MachineId::Local, MachineId::Remote(MACHINE.to_string())] {
        let project = |project_id: &str| ProjectKey {
            machine: machine.clone(),
            project_id: project_id.to_string(),
        };
        let label = |name: &str| match &machine {
            MachineId::Local => format!("local/{name}"),
            MachineId::Remote(id) => format!("{id}/{name}"),
        };
        let subgroup = |project_id: &str, group_id: &str| ActiveGroup::Subgroup {
            project: project(project_id),
            group_id: group_id.to_string(),
        };
        cases.extend([
            // The project's own group, which is the group almost every list is judged by.
            (
                label("project-with-rows"),
                ActiveGroup::Project(project("P-rows")),
                false,
            ),
            (
                label("project-with-no-rows"),
                ActiveGroup::Project(project("P-bare")),
                true,
            ),
            // Every row of the project sits in the user-made group, so the project's own list is
            // empty while the group's is not: the list the old runtime posts is the project's, and
            // an empty one is still refused, because the store holds rows for it either way.
            (
                label("subgroup-holds-every-row"),
                subgroup("P-sub-only", "g-all"),
                false,
            ),
            // THE CASE THE RULE EXISTS FOR: the group was drained and kept, the project still has
            // its own rows. `judge-the-group-alone` confirms the clear here.
            (
                label("subgroup-drained-project-keeps-rows"),
                subgroup("P-kept", "g-drained"),
                false,
            ),
            // Both lists empty: the one shape in which a user-made group confirms.
            (
                label("subgroup-and-project-empty"),
                subgroup("P-empty", "g-empty"),
                true,
            ),
            // A project that does not exist, and a user-made group that does not: `Missing`, which
            // `believe-a-missing-list` reads as "no tabs".
            (
                label("project-missing"),
                ActiveGroup::Project(project("P-gone")),
                false,
            ),
            (
                label("subgroup-missing"),
                subgroup("P-bare", "g-never-made"),
                false,
            ),
        ]);
    }
    cases.push((
        "offline/project-not-loaded".to_string(),
        ActiveGroup::Project(ProjectKey::remote(OFFLINE_MACHINE, "P-rows")),
        false,
    ));
    cases.push((
        "offline/subgroup-not-loaded".to_string(),
        ActiveGroup::Subgroup {
            project: ProjectKey::remote(OFFLINE_MACHINE, "P-kept"),
            group_id: "g-drained".to_string(),
        },
        false,
    ));
    cases
}

/// What a list is, in the three words the guard turns on.
fn list_shape(store: &PresentationStore, group: &ActiveGroup) -> &'static str {
    match store.tab_sessions(group) {
        Loadable::NotLoaded => "notLoaded",
        Loadable::Missing => "missing",
        Loadable::Loaded(tabs) if tabs.is_empty() => "empty",
        Loadable::Loaded(_) => "rows",
    }
}

/// The same, for the project's own group behind a user-made one; `null` for every other group.
fn project_list_shape(store: &PresentationStore, group: &ActiveGroup) -> Value {
    match group {
        ActiveGroup::Subgroup { project, .. } => {
            Value::from(list_shape(store, &ActiveGroup::Project(project.clone())))
        }
        _ => Value::Null,
    }
}

/// Two machines holding the same five projects. The user-made groups of both live in THIS
/// computer's workspace-groups document.
fn seeded_core() -> Core {
    let mut core = Core::new();
    core.handle_raw_frame(
        MachineId::Local,
        &snapshot("server-local").to_string(),
        NOW_MS,
    )
    .expect("the local frame parses");
    core.handle_raw_frame(
        MachineId::Remote(MACHINE.to_string()),
        &snapshot("server-remote").to_string(),
        NOW_MS,
    )
    .expect("the remote frame parses");
    let state = serde_json::from_value::<ghostex_gx_protocol::WorkspaceSessionGroupsState>(
        workspace_groups_json(),
    )
    .expect("the document parses");
    core.handle(
        Event::Intent(Intent::SetSideState {
            machine: MachineId::Local,
            update: Box::new(SideStateUpdate::WorkspaceGroups(state)),
        }),
        NOW_MS,
    );
    core
}

/// `P-sub-only` keeps every row in `g-all`; `g-drained` is the group whose last member was closed;
/// `g-empty` belongs to a project with no rows of its own either.
fn workspace_groups_json() -> Value {
    let document = |project: ProjectKey| {
        (
            project.to_workspace_project_id(),
            json!({
                "groups": [
                    { "groupId": "g-all", "title": "Mine", "sessionIds": ["A", "B"] },
                    { "groupId": "g-drained", "title": "Drained", "sessionIds": [] },
                    { "groupId": "g-empty", "title": "Empty", "sessionIds": [] },
                ],
                "nextGroupNumber": 4,
            }),
        )
    };
    let mut projects = serde_json::Map::new();
    for machine in [MachineId::Local, MachineId::Remote(MACHINE.to_string())] {
        for project_id in ["P-sub-only", "P-kept", "P-empty"] {
            let (key, value) = document(ProjectKey {
                machine: machine.clone(),
                project_id: project_id.to_string(),
            });
            projects.insert(key, value);
        }
    }
    json!({ "projectOrder": [], "projects": Value::Object(projects) })
}

fn snapshot(server_id: &str) -> Value {
    frame(
        server_id,
        &[
            ("P-sub-only", vec!["A", "B"]),
            ("P-kept", vec!["D", "E"]),
            ("P-empty", vec![]),
            ("P-rows", vec!["F"]),
            ("P-bare", vec![]),
        ],
    )
}

fn frame(server_id: &str, projects: &[(&str, Vec<&str>)]) -> Value {
    let sessions: Vec<Value> = projects
        .iter()
        .flat_map(|(project_id, session_ids)| {
            session_ids.iter().map(move |session_id| {
                json!({
                    "sessionId": session_id,
                    "projectId": project_id,
                    "groupId": format!("{project_id}:active"),
                    "kind": "agent",
                    "surface": "workspace",
                    "zmxName": format!("S90-{project_id}-{session_id}"),
                    "sortKey": format!("000{session_id}"),
                    "visibleInSidebarByDefault": true,
                    "alias": format!("Session {session_id}"),
                    "activity": "idle",
                    "pendingQuestionCount": 0,
                    "lifecycleState": "running",
                    "isPinned": false,
                    "lastInteractionAt": "2026-09-15T01:18:43.055Z",
                    "createdAt": "2026-09-01T01:00:00.000Z",
                    "updatedAt": "2026-09-15T01:18:43.055Z",
                })
            })
        })
        .collect();
    json!({
        "type": "presentationSnapshot",
        "protocolVersion": ghostex_gx_core::protocol::GXSERVER_PROTOCOL_VERSION,
        "serverId": server_id,
        "revision": 1,
        "snapshot": {
            "revision": 1,
            "generatedAt": "2026-09-21T00:00:00.000Z",
            "projects": projects.iter().map(|(project_id, _)| json!({
                "projectId": project_id,
                "title": project_id,
                "path": format!("/tmp/{project_id}"),
                "pathState": "available",
                "groupIds": [format!("{project_id}:active")],
                "sortKey": format!("1:{project_id}"),
                "createdAt": "2026-06-29T13:10:42.091Z",
                "updatedAt": "2026-09-15T01:18:43.055Z",
            })).collect::<Vec<_>>(),
            "groups": projects.iter().map(|(project_id, session_ids)| json!({
                "groupId": format!("{project_id}:active"),
                "projectId": project_id,
                "title": "Active",
                "sessionIds": session_ids,
                "sortKey": format!("1:{project_id}:active"),
            })).collect::<Vec<_>>(),
            "sessions": sessions,
        }
    })
}
