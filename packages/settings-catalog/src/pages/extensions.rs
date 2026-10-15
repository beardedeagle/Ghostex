use crate::hotkey_label::hotkey_label;
use crate::rows::{row, section, Page, Section};
use crate::Platform;

/// CDXC:Extensions 2026-08-30:
/// One Extensions page covers the built-in features (searchable by their own
/// names, straight from the shared descriptor list) and the extension store.
pub(crate) fn page(platform: Platform) -> Page {
    Page {
        id: "extensions",
        title: "Extensions",
        sections: vec![
            view_order(platform),
            official(platform),
            store(),
            custom_views(),
        ],
    }
}

pub(crate) fn view_order(platform: Platform) -> Section {
    section(
        "viewOrder",
        "Views",
        vec![
            row("titlebarViewOrder", "Arrange views", format!("Reorder built-in, extension, and custom views. New tabs open in this order, and {} through {} follow the tabs in the view panel.", hotkey_label("alt+1", platform), hotkey_label("alt+9", platform))),
        ],
    )
}

pub(crate) fn official(platform: Platform) -> Section {
    let mut rows = vec![
        row("linear", "Linear", "Open your team's issues and projects. Choose a home URL for each project or worktree."),
        row("jira", "Jira", "Keep your team's board beside your work. Choose a home URL for each project or worktree."),
        row("github", "GitHub", "Open this project's GitHub repository automatically from its origin remote."),
        row("sentry", "Sentry", "Investigate errors and stack traces beside your code. Choose a home URL for each project or worktree."),
        row("figma", "Figma", "Keep designs and component specs beside your implementation. Choose a home URL for each project or worktree."),
        row("vercel", "Vercel", "Check deployments and preview your changes. Choose a home URL for each project or worktree."),
        row("supabase", "Supabase", "Browse your database, authentication, and project logs. Choose a home URL for each project or worktree."),
        row("github-actions", "GitHub Actions", "Follow workflow runs, build results, and job logs. Choose a home URL for each project or worktree."),
        row("posthog", "PostHog", "Explore product analytics and session replays. Choose a home URL for each project or worktree."),
        row("custom-website", "Custom Website", "Keep any website beside your work. Choose a home URL for each project or worktree."),
        row("storybook", "Storybook", "Build, browse, and annotate your project’s components without a persistent development server. Appears only in projects with Storybook."),
        row("code", "Code editor", "Explore, edit, and search your project in a familiar, full-featured workspace without ever leaving Ghostex."),
        row("browser", "Browser", "Open websites alongside your project and keep useful pages organized without leaving Ghostex."),
        row("kanban", "Kanban", "Plan upcoming work and track task progress at a glance."),
        row("automate", "Automate", "Turn repeatable project routines into simple workflows you can run whenever you need them."),
        row("bots", "Bots", "Swap the sidebar to your Hermes agents: one row each, with a new session one click away. Needs the Hermes CLI."),
        row("botAutomations", "Bot automations", "A feed of every Hermes cron run, one channel per job, opened from the Bots sidebar. Needs Bots."),
        row("docs", "Files", "Browse your project’s notes, plans, and reference files together in one focused reading space."),
        row("terminal", "Terminal", "A command terminal beside your sessions, with its own tabs and splits, that works like the Commands pane but lives in the view panel."),
        row("tips", "Tips & Tricks", "A panel of short tips for getting more out of Ghostex, opened from the ⋯ menu."),
        row("notifications", "Notifications", "A bell in the sidebar's top row that lists what your agents finished or need from you."),
        row("help", "Ghostex Help", "A ⋯ menu entry with sample questions that start a Ghostex Help chat: an agent explains the app or changes settings for you."),
        row("devServers", "Dev servers", "A ⋯ menu panel listing development servers running on this computer. A new Browser tab shows the same list."),
        row("resources", "Resources", "A ⋯ menu panel listing what Ghostex is running right now, with the CPU and memory each part is using."),
        row("gitActions", "Git actions", "A work area header button for commit, branch, and worktree helpers on the active project."),
        row("quickActions", "Actions", "Saved terminal commands and web pages you start in one click from the Start button, a hotkey, Quick Access or a project row."),
        row("openIn", "Open In", "Open the active project or a session's folder in your editor, terminal or file manager from the Open button and the Open In menus."),
        row("spaces", "Spaces", "Group projects into Spaces and switch between them from a row of icons at the top of the sidebar, or by swiping."),
        row("cloudBoxes", "Cloud Boxes", "Run agent sessions in isolated boxes, in Docker on this computer or in the cloud, from New Thread's Run on choice or Run in a Box in the Select Agent menu."),
        row("workspaces", "Workspaces", "Keep each company's projects, Spaces, browser sign-ins and Claude account in its own workspace, and turn on work mode: Linear tickets and GitHub pull requests on your sessions, the Work view and your team's Slack flow."),
        row("extensionsButton", "Extensions", "An entry in the work area header’s ⋯ menu that opens this Extensions page."),
        row("cef", "Chromium runtime (CEF)", "Install, reinstall or uninstall the optional web runtime used by the Browser, the Code view, website and extension views, and HTML files in Files."),
    ];
    rows.retain(|row| crate::built_in_extensions::available_on(row.key, platform));
    section("official", "Built-in", rows)
}

pub(crate) fn store() -> Section {
    section(
        "store",
        "Extensions Store",
        vec![row(
            "store",
            "Extension store",
            "Browse audited extensions, install them, and manage what is already installed.",
        )],
    )
}

pub(crate) fn custom_views() -> Section {
    section(
        "customViews",
        "Your views",
        vec![
            row("customViews", "Your views", "Project views, templates, Storybook, Linear, GitHub Issues, dev server commands, and HTML reports."),
        ],
    )
}
