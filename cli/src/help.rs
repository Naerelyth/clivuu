//! Localized help and documentation renderers driven by [`crate::spec`].
use crate::spec::{COMMANDS, Category, CommandSpec, ConflictGroup, SubcommandSpec, WORKER_OPS};
use clivuu_core::i18n::{self, t, tf};
use std::{
    env,
    fmt::Write,
    path::{Path, PathBuf},
};

/// Help target resolved from `--help`, `-h`, or `clivuu help ...`.
#[derive(Clone, Copy, Debug)]
pub enum HelpTarget {
    Root,
    Command(&'static CommandSpec),
    Subcommand(&'static CommandSpec, &'static SubcommandSpec),
}

/// Render help for the resolved [`HelpTarget`].
pub fn render(target: HelpTarget) -> String {
    match target {
        HelpTarget::Root => render_root(),
        HelpTarget::Command(cmd) => render_command(cmd),
        HelpTarget::Subcommand(cmd, sub) => render_subcommand(cmd, sub),
    }
}

fn push_section(out: &mut String, title: String) {
    if !out.is_empty() && !out.ends_with("\n\n") {
        out.push('\n');
    }
    let _ = writeln!(out, "{title}");
}

/// Render the top-level `clivuu --help` overview grouped by [`Category`].
pub fn render_root() -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Clivuu {}", env!("CARGO_PKG_VERSION"));
    let _ = writeln!(
        out,
        "{}\n",
        t(
            "Wayland/X11 screenshot, annotation, floating pin, scrolling capture, and screen recording tool."
        )
    );

    let _ = writeln!(out, "{}", t("Usage:"));
    let _ = writeln!(out, "  clivuu");
    let _ = writeln!(out, "  clivuu <COMMAND> [OPTIONS] [ARGS]");

    push_section(&mut out, t("Global options:"));
    let _ = writeln!(
        out,
        "  {:<26}{}",
        "-h, --help",
        t("Print general overview or help for a command")
    );
    let _ = writeln!(
        out,
        "  {:<26}{}",
        "-V, --version",
        t("Print version information")
    );

    for category in Category::ALL {
        push_section(&mut out, tf("{0}:", &[&category.label()]));
        for cmd in COMMANDS.iter().filter(|c| c.category == category) {
            let _ = writeln!(out, "  {:<26}{}", cmd.name, (cmd.summary)());
        }
    }

    out.push('\n');
    let _ = writeln!(
        out,
        "{}",
        t(
            "Run \"clivuu <COMMAND> --help\" for command options and examples, or \"clivuu docs\" for guide paths."
        )
    );
    out.trim_end().to_owned()
}

/// Render detailed localized help for a single command specification.
pub fn render_command(cmd: &'static CommandSpec) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "clivuu {} — {}", cmd.name, (cmd.summary)());

    push_section(&mut out, t("Usage:"));
    if cmd.name == "start" {
        let _ = writeln!(out, "  clivuu");
    }
    if !cmd.subcommands.is_empty() {
        let _ = writeln!(out, "  clivuu {} <SUBCOMMAND> [ARGS]", cmd.name);
    } else if cmd.name == "worker" {
        let _ = writeln!(out, "  clivuu worker <OP> [ARGS...]");
    } else {
        let mut line = format!("  clivuu {}", cmd.name);
        if !cmd.options.is_empty() {
            line.push_str(" [OPTIONS]");
        }
        if let Some(pos) = cmd.positional {
            let _ = write!(line, " {}", pos.syntax());
        }
        let _ = writeln!(out, "{line}");
    }

    if let Some(details) = cmd.details {
        out.push('\n');
        let _ = writeln!(out, "{}", details());
    }

    if let Some(pos) = cmd.positional {
        push_section(&mut out, t("Arguments:"));
        let mut desc = (pos.summary)();
        if !pos.choices.is_empty() {
            let values = pos.choices.join(", ");
            desc.push(' ');
            desc.push_str(&tf("[possible values: {0}]", &[&values]));
        }
        let _ = writeln!(out, "  {:<26}{}", pos.syntax(), desc);
    }

    if !cmd.subcommands.is_empty() {
        push_section(&mut out, t("Subcommands:"));
        for sub in cmd.subcommands {
            let _ = writeln!(out, "  {:<26}{}", sub.usage, (sub.summary)());
        }
    }

    if cmd.name == "worker" {
        push_section(&mut out, t("Operations:"));
        for &(op, signature) in WORKER_OPS {
            let _ = writeln!(out, "  {op:<20}{signature}");
        }
    }

    push_section(&mut out, t("Options:"));
    for opt in cmd.options {
        let _ = writeln!(out, "  {:<26}{}", opt.usage_label(), (opt.summary)());
    }
    let _ = writeln!(
        out,
        "  {:<26}{}",
        "-h, --help",
        t("Print help information for this command")
    );

    let mut conflict_lines = Vec::new();
    for group in ConflictGroup::ALL {
        let flags: Vec<&str> = cmd
            .options
            .iter()
            .filter(|opt| opt.conflict_group == Some(group))
            .map(|opt| opt.long)
            .collect();
        if flags.len() >= 2 {
            let joined = flags.join(", ");
            conflict_lines.push(format!("  {}", tf("{0}: {1}", &[&group.label(), &joined])));
        }
    }
    if !conflict_lines.is_empty() {
        push_section(&mut out, t("Mutually exclusive:"));
        for line in conflict_lines {
            let _ = writeln!(out, "{line}");
        }
    }

    if !cmd.examples.is_empty() {
        push_section(&mut out, t("Examples:"));
        for example in cmd.examples {
            let _ = writeln!(out, "  {example}");
        }
    }

    if !cmd.subcommands.is_empty() {
        out.push('\n');
        let _ = writeln!(
            out,
            "{}",
            tf(
                "Run \"clivuu {0} <SUBCOMMAND> --help\" for subcommand details.",
                &[cmd.name]
            )
        );
    }

    out.trim_end().to_owned()
}

fn render_subcommand(cmd: &'static CommandSpec, sub: &'static SubcommandSpec) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "clivuu {} {} — {}",
        cmd.name,
        sub.name,
        (sub.summary)()
    );
    push_section(&mut out, t("Usage:"));
    let _ = writeln!(out, "  clivuu {} {}", cmd.name, sub.usage);
    if let Some(details) = sub.details {
        out.push('\n');
        let _ = writeln!(out, "{}", details());
    }
    out.push('\n');
    let _ = writeln!(
        out,
        "{}",
        tf(
            "Run \"clivuu {0} --help\" to list all subcommands.",
            &[cmd.name]
        )
    );
    out.trim_end().to_owned()
}

fn preferred_docs_lang() -> &'static str {
    if i18n::current_language().id().starts_with("zh") {
        "zh-CN"
    } else {
        "en"
    }
}

fn discover_doc_index(lang: &str) -> Option<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = env::current_exe()
        && let Some(prefix) = exe.parent().and_then(Path::parent)
    {
        roots.push(prefix.join("share/doc/clivuu/docs"));
    }
    roots.push(PathBuf::from("/usr/share/doc/clivuu/docs"));
    roots.push(PathBuf::from("/usr/local/share/doc/clivuu/docs"));
    roots.push(PathBuf::from("docs"));

    for root in roots {
        let localized = root.join(lang).join("README.md");
        if localized.is_file() {
            return Some(localized);
        }
        let english = root.join("en/README.md");
        if english.is_file() {
            return Some(english);
        }
    }
    None
}

/// Render the concise `clivuu docs` manual overview and discovered file locations.
pub fn render_docs() -> String {
    let lang = preferred_docs_lang();
    let index_path = discover_doc_index(lang);
    let doc_dir = index_path
        .as_deref()
        .and_then(Path::parent)
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| format!("docs/{lang}"));

    let mut out = String::new();
    let _ = writeln!(out, "{}", t("Clivuu User & Developer Documentation"));
    let _ = writeln!(
        out,
        "{}\n",
        t(
            "Detailed guides for shortcuts, canvas annotations, pins, scrolling, recording, OCR, and CLI control live in the documentation tree."
        )
    );

    let _ = writeln!(out, "{}", t("Local manuals:"));
    for (label, rel) in [
        (t("Overview index"), "README.md"),
        (t("Shortcuts reference"), "guide/shortcuts.md"),
        (t("Preferences guide"), "guide/preferences.md"),
        (t("CLI & D-Bus reference"), "development/cli.md"),
        (t("Configuration reference"), "development/configuration.md"),
    ] {
        let path = format!("{doc_dir}/{rel}");
        let _ = writeln!(out, "  {}", tf("{0}: {1}", &[&label, &path]));
    }

    push_section(&mut out, t("Interactive discovery:"));
    let _ = writeln!(
        out,
        "  {:<30}{}",
        "clivuu --help",
        t("Categorized overview of all CLI commands")
    );
    let _ = writeln!(
        out,
        "  {:<30}{}",
        "clivuu <COMMAND> --help",
        t("Detailed options, mutual exclusions, and examples")
    );
    let _ = writeln!(
        out,
        "  {:<30}{}",
        "clivuu completions <SHELL>",
        t("Generate bash, zsh, or fish tab completions")
    );
    let _ = writeln!(
        out,
        "  {:<30}{}",
        "clivuu config defaults",
        t("Inspect all built-in configuration defaults as JSON")
    );
    let _ = writeln!(
        out,
        "  {:<30}{}",
        "clivuu status",
        t("Inspect active compositor backend, config path, and tools")
    );

    out.trim_end().to_owned()
}
