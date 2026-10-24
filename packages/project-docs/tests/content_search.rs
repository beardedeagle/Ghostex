//! The Search sidebar's engine: which files it searches (ignore files, the always-skipped
//! dependency and build folders, nested repositories, binaries, the size cap), the match cap, and
//! the query toggles.

use std::{
    fs,
    path::{Path, PathBuf},
    sync::{atomic::AtomicBool, Mutex},
};

use ghostex_docs::content_search::{
    search_content, ContentSearchOptions, ContentSearchSummary, FileMatches, MAX_FILE_SIZE,
};

struct Project(PathBuf);

impl Project {
    fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "ghostex-content-search-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        Self(root)
    }

    fn file(&self, relative: &str, text: impl AsRef<[u8]>) -> &Self {
        let path = self.0.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
        self
    }

    fn git_repo(&self, relative: &str) -> &Self {
        fs::create_dir_all(self.0.join(relative).join(".git")).unwrap();
        self
    }

    fn root(&self) -> &Path {
        &self.0
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn options(query: &str) -> ContentSearchOptions {
    ContentSearchOptions {
        query: query.to_string(),
        use_ignore_files: true,
        max_matches: 10_000,
        ..Default::default()
    }
}

fn run(root: &Path, options: &ContentSearchOptions) -> (Vec<FileMatches>, ContentSearchSummary) {
    let found = Mutex::new(Vec::new());
    let summary = search_content(root, options, &AtomicBool::new(false), |file| {
        found.lock().unwrap().push(file);
    })
    .unwrap();
    let mut found = found.into_inner().unwrap();
    found.sort_by(|a, b| a.path.cmp(&b.path));
    (found, summary)
}

fn paths(root: &Path, options: &ContentSearchOptions) -> Vec<String> {
    run(root, options)
        .0
        .into_iter()
        .map(|file| file.path)
        .collect()
}

#[test]
fn skips_dependency_and_build_folders_without_a_gitignore() {
    let project = Project::new("skips");
    for folder in [
        "node_modules/pkg",
        "target/debug",
        "build",
        "dist",
        "out",
        ".next",
        ".turbo",
        ".cache",
        "__pycache__",
        ".venv/lib",
        "venv",
        ".gradle",
        "ios/Pods/Lib",
        "DerivedData",
        ".dart_tool",
        "vendor/bundle/ruby",
        ".zig-cache",
        "zig-out",
        ".git",
    ] {
        project.file(&format!("{folder}/hit.txt"), "needle");
    }
    project
        .file("src/main.rs", "fn needle() {}")
        .file(".github/workflows/ci.yml", "needle: true")
        .file("vendor/keep.txt", "needle");
    assert_eq!(
        paths(project.root(), &options("needle")),
        vec![".github/workflows/ci.yml", "src/main.rs", "vendor/keep.txt"]
    );
}

#[test]
fn honours_gitignore_and_ignore_files() {
    let project = Project::new("gitignore");
    project
        .git_repo("")
        .file(".gitignore", "generated/\n*.log\n")
        .file(".ignore", "secret.txt\n")
        .file("generated/out.rs", "needle")
        .file("debug.log", "needle")
        .file("secret.txt", "needle")
        .file("kept.rs", "needle");
    assert_eq!(paths(project.root(), &options("needle")), vec!["kept.rs"]);
}

#[test]
fn turning_ignore_files_off_searches_everything_but_git() {
    let project = Project::new("toggle-off");
    project
        .git_repo("")
        .file(".gitignore", "generated/\n")
        .file("generated/out.rs", "needle")
        .file("node_modules/pkg/index.js", "needle")
        .file(".git/config", "needle")
        .file("kept.rs", "needle");
    let mut everything = options("needle");
    everything.use_ignore_files = false;
    assert_eq!(
        paths(project.root(), &everything),
        vec!["generated/out.rs", "kept.rs", "node_modules/pkg/index.js"]
    );
}

#[test]
fn nested_repositories_in_dot_folders_need_an_include_pattern() {
    let project = Project::new("nested");
    project
        .git_repo("")
        .git_repo(".dependencies/zmx")
        .file(".dependencies/zmx/src/main.zig", "needle")
        .file(".dependencies/zmx/README.md", "needle")
        .git_repo("apps/mobile/app")
        .file("apps/mobile/app/src/chat.ts", "needle")
        .file("app.rs", "needle");
    // Only nested repositories inside a dot-folder are skipped.
    assert_eq!(
        paths(project.root(), &options("needle")),
        vec!["app.rs", "apps/mobile/app/src/chat.ts"]
    );

    let mut inside = options("needle");
    inside.include = ".dependencies/zmx/src".to_string();
    assert_eq!(
        paths(project.root(), &inside),
        vec![".dependencies/zmx/src/main.zig"]
    );

    let mut everything = options("needle");
    everything.use_ignore_files = false;
    assert_eq!(paths(project.root(), &everything).len(), 4);
}

#[test]
fn skips_binary_and_oversized_files() {
    let project = Project::new("binary");
    let mut big = b"needle\n".to_vec();
    big.resize(MAX_FILE_SIZE as usize + 1, b'x');
    project
        .file("image.bin", b"needle\0\x01\x02")
        .file("big.txt", big)
        .file("small.txt", "needle");
    assert_eq!(paths(project.root(), &options("needle")), vec!["small.txt"]);
}

#[test]
fn stops_at_the_match_cap() {
    let project = Project::new("cap");
    for index in 0..30 {
        project.file(&format!("file{index}.txt"), "needle\n".repeat(10));
    }
    let mut capped = options("needle");
    capped.max_matches = 25;
    let (found, summary) = run(project.root(), &capped);
    let delivered: usize = found.iter().map(|file| file.matches.len()).sum();
    assert!(summary.capped);
    assert_eq!(summary.matches, 25);
    assert_eq!(delivered, 25);

    let (found, summary) = run(project.root(), &options("needle"));
    assert!(!summary.capped);
    assert_eq!(summary.matches, 300);
    assert_eq!(summary.files, 30);
    assert_eq!(found.len(), 30);
}

#[test]
fn match_case_whole_word_regex_and_globs() {
    let project = Project::new("toggles");
    project
        .file("a.rs", "let Needle = needles;\n  needle(x)\n")
        .file("b.ts", "needle")
        .file("docs/c.md", "needle");

    let (found, _) = run(project.root(), &options("needle"));
    let a = &found[0];
    assert_eq!(a.path, "a.rs");
    assert_eq!(a.matches.len(), 3);
    let second = &a.matches[2];
    assert_eq!((second.line, second.start, second.end), (1, 2, 8));
    assert_eq!(
        (
            second.before.as_str(),
            second.matched.as_str(),
            second.after.as_str()
        ),
        ("", "needle", "(x)")
    );

    let mut case = options("Needle");
    case.match_case = true;
    assert_eq!(run(project.root(), &case).1.matches, 1);

    let mut word = options("needle");
    word.whole_word = true;
    assert_eq!(run(project.root(), &word).1.matches, 4);

    let mut regex = options(r"need\w+s");
    regex.is_regex = true;
    assert_eq!(run(project.root(), &regex).1.matches, 1);
    regex.query = "(".to_string();
    assert!(search_content(project.root(), &regex, &AtomicBool::new(false), |_| {}).is_err());

    let mut include = options("needle");
    include.include = "*.ts, docs".to_string();
    assert_eq!(paths(project.root(), &include), vec!["b.ts", "docs/c.md"]);

    let mut exclude = options("needle");
    exclude.exclude = "*.rs".to_string();
    assert_eq!(paths(project.root(), &exclude), vec!["b.ts", "docs/c.md"]);
}
