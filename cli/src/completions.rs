//! Shell completion script generator for Bash, Zsh, and Fish driven by [`crate::spec`] and config schema.
use crate::spec::{self, COMMANDS, CommandSpec, OptionKind, OptionSpec, SHELL_CHOICES, WORKER_OPS};
use clivuu_core::Result;
use clivuu_core::config;
use clivuu_core::i18n::{t, tf};
use serde_json::Value;
use std::fmt::Write;

/// Target shell syntax for [`generate`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shell {
    Bash,
    Zsh,
    Fish,
}

impl Shell {
    /// Parse a CLI shell selector (`bash`, `zsh`, or `fish`).
    pub fn parse(name: &str) -> Result<Self> {
        match name {
            "bash" => Ok(Self::Bash),
            "zsh" => Ok(Self::Zsh),
            "fish" => Ok(Self::Fish),
            _ => Err(tf(
                "Unsupported shell: {0}; choose one of: bash, zsh, fish",
                &[name],
            )
            .into()),
        }
    }
}

struct SchemaEntry {
    key: String,
    values: Vec<String>,
}

fn schema_entries() -> Vec<SchemaEntry> {
    config::schema()
        .into_iter()
        .map(|(key, spec)| {
            let values = if let Some(choices) = spec.get("choices").and_then(Value::as_array) {
                choices
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            } else if spec.get("default").is_some_and(Value::is_boolean) {
                vec!["true".to_owned(), "false".to_owned()]
            } else {
                Vec::new()
            };
            SchemaEntry { key, values }
        })
        .collect()
}

fn single_line(text: &str) -> String {
    text.replace(['\r', '\n'], " ")
}

fn escape_posix_single_quoted(text: &str) -> String {
    single_line(text).replace('\'', "'\\''")
}

fn escape_zsh_opt_desc(text: &str) -> String {
    escape_posix_single_quoted(text)
        .replace('[', "\\[")
        .replace(']', "\\]")
}

fn escape_fish_single_quoted(text: &str) -> String {
    single_line(text).replace('\\', "\\\\").replace('\'', "\\'")
}

fn command_flags_list(cmd: &CommandSpec) -> Vec<&'static str> {
    let mut flags = Vec::new();
    for opt in cmd.options {
        if let Some(short) = opt.short {
            flags.push(short);
        }
        flags.push(opt.long);
    }
    flags.push("-h");
    flags.push("--help");
    flags
}

/// Generate a complete tab-completion script for `shell`.
pub fn generate(shell: Shell) -> String {
    let schema = schema_entries();
    match shell {
        Shell::Bash => generate_bash(&schema),
        Shell::Zsh => generate_zsh(&schema),
        Shell::Fish => generate_fish(&schema),
    }
}

fn generate_bash(schema: &[SchemaEntry]) -> String {
    let mut out = String::new();
    let commands_list: Vec<&str> = COMMANDS.iter().map(|c| c.name).collect();
    let config_cmd = spec::find_command("config").expect("config spec");
    let config_subs: Vec<&str> = config_cmd.subcommands.iter().map(|s| s.name).collect();
    let worker_ops: Vec<&str> = WORKER_OPS.iter().map(|&(op, _)| op).collect();
    let all_keys: Vec<&str> = schema.iter().map(|e| e.key.as_str()).collect();

    let _ = writeln!(out, "# bash completion for clivuu (generated)");
    let _ = writeln!(out, "_clivuu() {{");
    let _ = writeln!(out, "    local cur prev cmd sub");
    let _ = writeln!(out, "    COMPREPLY=()");
    let _ = writeln!(out, "    cur=\"${{COMP_WORDS[COMP_CWORD]}}\"");
    let _ = writeln!(
        out,
        "    prev=\"\"; (( COMP_CWORD > 0 )) && prev=\"${{COMP_WORDS[COMP_CWORD-1]}}\""
    );
    let _ = writeln!(out, "    if (( COMP_CWORD == 1 )); then");
    let _ = writeln!(
        out,
        "        COMPREPLY=($(compgen -W \"{} -h --help -V --version\" -- \"$cur\"))",
        commands_list.join(" ")
    );
    let _ = writeln!(out, "        return 0");
    let _ = writeln!(out, "    fi");
    let _ = writeln!(out, "    cmd=\"${{COMP_WORDS[1]}}\"");
    let _ = writeln!(out, "    case \"$cmd\" in");

    let _ = writeln!(out, "        help)");
    let _ = writeln!(out, "            if (( COMP_CWORD == 2 )); then");
    let _ = writeln!(
        out,
        "                COMPREPLY=($(compgen -W \"{} -h --help\" -- \"$cur\"))",
        commands_list.join(" ")
    );
    let _ = writeln!(
        out,
        "            elif (( COMP_CWORD == 3 )) && [[ \"${{COMP_WORDS[2]}}\" == \"config\" ]]; then"
    );
    let _ = writeln!(
        out,
        "                COMPREPLY=($(compgen -W \"{}\" -- \"$cur\"))",
        config_subs.join(" ")
    );
    let _ = writeln!(out, "            fi");
    let _ = writeln!(out, "            ;;");

    let _ = writeln!(out, "        completions)");
    let _ = writeln!(out, "            if (( COMP_CWORD == 2 )); then");
    let _ = writeln!(
        out,
        "                COMPREPLY=($(compgen -W \"{} -h --help\" -- \"$cur\"))",
        SHELL_CHOICES.join(" ")
    );
    let _ = writeln!(out, "            fi");
    let _ = writeln!(out, "            ;;");

    let _ = writeln!(out, "        worker)");
    let _ = writeln!(out, "            if (( COMP_CWORD == 2 )); then");
    let _ = writeln!(
        out,
        "                COMPREPLY=($(compgen -W \"{} -h --help\" -- \"$cur\"))",
        worker_ops.join(" ")
    );
    let _ = writeln!(out, "            else");
    let _ = writeln!(out, "                COMPREPLY=($(compgen -f -- \"$cur\"))");
    let _ = writeln!(out, "            fi");
    let _ = writeln!(out, "            ;;");

    let _ = writeln!(out, "        config)");
    let _ = writeln!(out, "            if (( COMP_CWORD == 2 )); then");
    let _ = writeln!(
        out,
        "                COMPREPLY=($(compgen -W \"{} -h --help\" -- \"$cur\"))",
        config_subs.join(" ")
    );
    let _ = writeln!(out, "                return 0");
    let _ = writeln!(out, "            fi");
    let _ = writeln!(out, "            sub=\"${{COMP_WORDS[2]}}\"");
    let _ = writeln!(out, "            case \"$sub\" in");
    let _ = writeln!(out, "                get)");
    let _ = writeln!(out, "                    if (( COMP_CWORD == 3 )); then");
    let _ = writeln!(
        out,
        "                        COMPREPLY=($(compgen -W \"{} -h --help\" -- \"$cur\"))",
        all_keys.join(" ")
    );
    let _ = writeln!(out, "                    fi");
    let _ = writeln!(out, "                    ;;");
    let _ = writeln!(out, "                set)");
    let _ = writeln!(out, "                    if (( COMP_CWORD == 3 )); then");
    let _ = writeln!(
        out,
        "                        COMPREPLY=($(compgen -W \"{} -h --help\" -- \"$cur\"))",
        all_keys.join(" ")
    );
    let _ = writeln!(out, "                    elif (( COMP_CWORD == 4 )); then");
    let _ = writeln!(
        out,
        "                        case \"${{COMP_WORDS[3]}}\" in"
    );
    for entry in schema.iter().filter(|e| !e.values.is_empty()) {
        let _ = writeln!(
            out,
            "                            {}) COMPREPLY=($(compgen -W \"{}\" -- \"$cur\")) ;;",
            entry.key,
            entry.values.join(" ")
        );
    }
    let _ = writeln!(out, "                        esac");
    let _ = writeln!(out, "                    fi");
    let _ = writeln!(out, "                    ;;");
    let _ = writeln!(out, "            esac");
    let _ = writeln!(out, "            ;;");

    for cmd in COMMANDS
        .iter()
        .filter(|c| !matches!(c.name, "help" | "completions" | "worker" | "config"))
    {
        let _ = writeln!(out, "        {})", cmd.name);
        let flags = command_flags_list(cmd).join(" ");
        let output_opt = cmd.options.iter().find(|o| o.kind == OptionKind::Output);
        if let Some(opt) = output_opt {
            let _ = writeln!(
                out,
                "            if [[ \"$prev\" == \"-o\" || \"$prev\" == \"--output\" ]]; then"
            );
            if opt.completion_choices.is_empty() {
                let _ = writeln!(out, "                COMPREPLY=($(compgen -f -- \"$cur\"))");
            } else {
                let _ = writeln!(
                    out,
                    "                COMPREPLY=($(compgen -W \"{}\" -- \"$cur\") $(compgen -f -- \"$cur\"))",
                    opt.completion_choices.join(" ")
                );
            }
            let _ = writeln!(out, "                return 0");
            let _ = writeln!(out, "            fi");
        }
        if cmd.name == "paste" {
            let _ = writeln!(
                out,
                "            if [[ \"$cur\" != -* ]]; then\n                local idx in_files=0\n                for (( idx=2; idx < COMP_CWORD; idx++ )); do\n                    if [[ \"${{COMP_WORDS[idx]}}\" == \"--files\" ]]; then\n                        in_files=1\n                    elif [[ \"${{COMP_WORDS[idx]}}\" == --* ]]; then\n                        in_files=0\n                    fi\n                done\n                if (( in_files == 1 )); then\n                    COMPREPLY=($(compgen -f -- \"$cur\"))\n                    return 0\n                fi\n            fi"
            );
        }
        if let Some(pos) = cmd.positional
            && pos.accepts_files
        {
            let _ = writeln!(out, "            if [[ \"$cur\" == -* ]]; then");
            let _ = writeln!(
                out,
                "                COMPREPLY=($(compgen -W \"{flags}\" -- \"$cur\"))"
            );
            let _ = writeln!(out, "            else");
            let _ = writeln!(out, "                COMPREPLY=($(compgen -f -- \"$cur\"))");
            let _ = writeln!(out, "            fi");
        } else {
            let _ = writeln!(
                out,
                "            COMPREPLY=($(compgen -W \"{flags}\" -- \"$cur\"))"
            );
        }
        let _ = writeln!(out, "            ;;");
    }

    let _ = writeln!(out, "    esac");
    let _ = writeln!(out, "}}");
    let _ = writeln!(out, "complete -F _clivuu clivuu");
    out
}

fn zsh_output_spec(cmd_name: &str, opt: &OptionSpec) -> String {
    let desc = escape_zsh_opt_desc(&(opt.summary)());
    if opt.completion_choices.is_empty() {
        format!(
            "'(-o --output)'{{-o,--output}}'[{desc}]:{}:_files'",
            opt.placeholder
        )
    } else {
        format!(
            "'(-o --output)'{{-o,--output}}'[{desc}]:{}:->output_{cmd_name}'",
            opt.placeholder
        )
    }
}

fn generate_zsh(schema: &[SchemaEntry]) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "#compdef clivuu");
    let _ = writeln!(out, "# zsh completion for clivuu (generated)\n");

    let _ = writeln!(out, "_clivuu_config_keys() {{");
    let _ = writeln!(out, "    local -a keys");
    let _ = writeln!(out, "    keys=(");
    for entry in schema {
        let _ = writeln!(out, "        '{}'", entry.key);
    }
    let _ = writeln!(out, "    )");
    let _ = writeln!(out, "    _describe -t config-keys 'configuration key' keys");
    let _ = writeln!(out, "}}\n");

    let _ = writeln!(out, "_clivuu_config_values() {{");
    let _ = writeln!(out, "    case \"$1\" in");
    for entry in schema.iter().filter(|e| !e.values.is_empty()) {
        let _ = writeln!(
            out,
            "        {}) _values 'value' {} ;;",
            entry.key,
            entry.values.join(" ")
        );
    }
    let _ = writeln!(out, "    esac");
    let _ = writeln!(out, "}}\n");

    let _ = writeln!(out, "_clivuu() {{");
    let _ = writeln!(out, "    local context state state_descr line");
    let _ = writeln!(out, "    typeset -A opt_args");
    let _ = writeln!(out, "    local -a cmds");
    let _ = writeln!(out, "    cmds=(");
    for cmd in COMMANDS {
        let _ = writeln!(
            out,
            "        '{}:{}'",
            cmd.name,
            escape_posix_single_quoted(&(cmd.summary)())
        );
    }
    let _ = writeln!(out, "    )");
    let _ = writeln!(out, "    _arguments -C \\");
    let _ = writeln!(
        out,
        "        '(-h --help)'{{-h,--help}}'[{}]' \\",
        escape_zsh_opt_desc(&t("Print general overview or help for a command"))
    );
    let _ = writeln!(
        out,
        "        '(-V --version)'{{-V,--version}}'[{}]' \\",
        escape_zsh_opt_desc(&t("Print version information"))
    );
    let _ = writeln!(out, "        '1:command:->cmd' \\");
    let _ = writeln!(out, "        '*::arg:->args'");
    let _ = writeln!(out, "    case \"$state\" in");
    let _ = writeln!(out, "        cmd)");
    let _ = writeln!(
        out,
        "            _describe -t commands 'clivuu command' cmds"
    );
    let _ = writeln!(out, "            ;;");
    let _ = writeln!(out, "        args)");
    let _ = writeln!(out, "            case \"$line[1]\" in");

    let help_flag = format!(
        "'(-h --help)'{{-h,--help}}'[{}]'",
        escape_zsh_opt_desc(&t("Print help information for this command"))
    );

    let _ = writeln!(out, "                config)");
    let _ = writeln!(out, "                    local -a subcmds");
    let _ = writeln!(out, "                    subcmds=(");
    let config_cmd = spec::find_command("config").expect("config spec");
    for sub in config_cmd.subcommands {
        let _ = writeln!(
            out,
            "                        '{}:{}'",
            sub.name,
            escape_posix_single_quoted(&(sub.summary)())
        );
    }
    let _ = writeln!(out, "                    )");
    let _ = writeln!(out, "                    _arguments -C \\");
    let _ = writeln!(out, "                        {help_flag} \\");
    let _ = writeln!(out, "                        '1:subcommand:->subcmd' \\");
    let _ = writeln!(out, "                        '*::subarg:->subargs'");
    let _ = writeln!(out, "                    case \"$state\" in");
    let _ = writeln!(
        out,
        "                        subcmd) _describe -t subcommands 'config subcommand' subcmds ;;"
    );
    let _ = writeln!(out, "                        subargs)");
    let _ = writeln!(out, "                            case \"$line[1]\" in");
    let _ = writeln!(
        out,
        "                                get) _arguments '1:key:_clivuu_config_keys' ;;"
    );
    let _ = writeln!(
        out,
        "                                set) _arguments '1:key:_clivuu_config_keys' '2:value:_clivuu_config_values \"$line[1]\"' ;;"
    );
    let _ = writeln!(out, "                            esac");
    let _ = writeln!(out, "                            ;;");
    let _ = writeln!(out, "                    esac");
    let _ = writeln!(out, "                    ;;");

    let _ = writeln!(out, "                completions)");
    let _ = writeln!(
        out,
        "                    _arguments {help_flag} '1:shell:({})'",
        SHELL_CHOICES.join(" ")
    );
    let _ = writeln!(out, "                    ;;");

    let _ = writeln!(out, "                help)");
    let _ = writeln!(
        out,
        "                    _arguments {help_flag} '1:command:_describe -t commands \"clivuu command\" cmds'"
    );
    let _ = writeln!(out, "                    ;;");

    let _ = writeln!(out, "                worker)");
    let _ = writeln!(out, "                    local -a ops");
    let _ = writeln!(
        out,
        "                    ops=({})",
        WORKER_OPS
            .iter()
            .map(|&(op, sig)| format!("'{op}:{}'", escape_posix_single_quoted(sig)))
            .collect::<Vec<_>>()
            .join(" ")
    );
    let _ = writeln!(
        out,
        "                    _arguments {help_flag} '1:operation:_describe -t operations \"worker operation\" ops' '*:arg:_files'"
    );
    let _ = writeln!(out, "                    ;;");

    for cmd in COMMANDS
        .iter()
        .filter(|c| !matches!(c.name, "config" | "completions" | "help" | "worker"))
    {
        let _ = writeln!(out, "                {})", cmd.name);
        let mut arg_specs = vec![help_flag.clone()];
        let mut output_choices: Option<&[&str]> = None;
        for opt in cmd.options {
            let desc = escape_zsh_opt_desc(&(opt.summary)());
            let spec = match opt.kind {
                OptionKind::Output => {
                    if !opt.completion_choices.is_empty() {
                        output_choices = Some(opt.completion_choices);
                    }
                    zsh_output_spec(cmd.name, opt)
                }
                OptionKind::Area => format!("'{}[{desc}]:X: :Y: :W: :H: '", opt.long),
                OptionKind::Size => format!("'{}[{desc}]:W: :H: '", opt.long),
                OptionKind::Position => format!("'{}[{desc}]:X: :Y: '", opt.long),
                OptionKind::Delay => format!("'{}[{desc}]:SECONDS: '", opt.long),
                OptionKind::PlainText | OptionKind::HtmlText | OptionKind::TextValue => {
                    format!("'{}[{desc}]:{}: '", opt.long, opt.placeholder)
                }
                OptionKind::Files => format!("'{}[{desc}]:*path:_files'", opt.long),
                OptionKind::Flag => format!("'{}[{desc}]'", opt.long),
            };
            arg_specs.push(spec);
        }
        if let Some(pos) = cmd.positional {
            let pos_action = if pos.accepts_files {
                "_files".to_owned()
            } else {
                " ".to_owned()
            };
            arg_specs.push(format!("'1:{}:{pos_action}'", pos.name));
        }
        if cmd.name == "paste" {
            arg_specs.push("'*:file:_files'".to_owned());
        }
        let _ = writeln!(out, "                    state=\"\"");
        let _ = writeln!(out, "                    _arguments -C \\");
        for (idx, item) in arg_specs.iter().enumerate() {
            let cont = if idx + 1 < arg_specs.len() { " \\" } else { "" };
            let _ = writeln!(out, "                        {item}{cont}");
        }
        if let Some(targets) = output_choices {
            let _ = writeln!(
                out,
                "                    if [[ \"$state\" == \"output_{}\" ]]; then",
                cmd.name
            );
            let _ = writeln!(
                out,
                "                        _alternative 'targets:target:({})' 'files:file:_files'",
                targets.join(" ")
            );
            let _ = writeln!(out, "                    fi");
        }
        let _ = writeln!(out, "                    ;;");
    }

    let _ = writeln!(out, "            esac");
    let _ = writeln!(out, "            ;;");
    let _ = writeln!(out, "    esac");
    let _ = writeln!(out, "}}");
    let _ = writeln!(out, "_clivuu \"$@\"");
    out
}

fn generate_fish(schema: &[SchemaEntry]) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# fish completion for clivuu (generated)");

    let all_cmd_names: Vec<&str> = COMMANDS.iter().map(|c| c.name).collect();
    let no_subcmd = "__fish_use_subcommand";

    let _ = writeln!(
        out,
        "complete -c clivuu -n '{no_subcmd}' -s h -l help -f -d '{}'",
        escape_fish_single_quoted(&t("Print general overview or help for a command"))
    );
    let _ = writeln!(
        out,
        "complete -c clivuu -n '{no_subcmd}' -s V -l version -f -d '{}'",
        escape_fish_single_quoted(&t("Print version information"))
    );

    for cmd in COMMANDS {
        let _ = writeln!(
            out,
            "complete -c clivuu -n '{no_subcmd}' -f -a '{}' -d '{}'",
            cmd.name,
            escape_fish_single_quoted(&(cmd.summary)())
        );
    }

    let help_desc = escape_fish_single_quoted(&t("Print help information for this command"));
    let all_names_list = all_cmd_names.join(" ");

    for cmd in COMMANDS {
        let cond = format!("__fish_seen_subcommand_from {}", cmd.name);
        let _ = writeln!(
            out,
            "complete -c clivuu -n '{cond}' -s h -l help -f -d '{help_desc}'"
        );
        for opt in cmd.options {
            let short = opt
                .short
                .map(|s| format!(" -s {}", s.trim_start_matches('-')))
                .unwrap_or_default();
            let long = format!(" -l {}", opt.long.trim_start_matches("--"));
            let desc = escape_fish_single_quoted(&(opt.summary)());
            let mut extra = String::new();
            if !opt.completion_choices.is_empty() {
                let _ = write!(extra, " -x -a '{}'", opt.completion_choices.join(" "));
            } else if opt.accepts_files {
                extra.push_str(" -r -F");
            } else if opt.kind == OptionKind::Flag {
                extra.push_str(" -f");
            } else {
                extra.push_str(" -x");
            }
            let _ = writeln!(
                out,
                "complete -c clivuu -n '{cond}'{short}{long}{extra} -d '{desc}'"
            );
        }
        if let Some(pos) = cmd.positional {
            if pos.accepts_files {
                let _ = writeln!(out, "complete -c clivuu -n '{cond}' -F");
            } else if !pos.choices.is_empty() {
                let _ = writeln!(
                    out,
                    "complete -c clivuu -n '{cond}' -f -a '{}' -d '{}'",
                    pos.choices.join(" "),
                    escape_fish_single_quoted(&(pos.summary)())
                );
            }
        }
        if cmd.name == "help" {
            let _ = writeln!(
                out,
                "complete -c clivuu -n '__fish_seen_subcommand_from help; and not __fish_seen_subcommand_from {all_names_list}' -f -a '{all_names_list}'"
            );
        }
    }

    let config_cmd = spec::find_command("config").expect("config spec");
    let config_subs_list: Vec<&str> = config_cmd.subcommands.iter().map(|s| s.name).collect();
    let config_subs_joined = config_subs_list.join(" ");
    for sub in config_cmd.subcommands {
        let _ = writeln!(
            out,
            "complete -c clivuu -n '__fish_seen_subcommand_from config; and not __fish_seen_subcommand_from {config_subs_joined}' -f -a '{}' -d '{}'",
            sub.name,
            escape_fish_single_quoted(&(sub.summary)())
        );
    }

    let all_keys_joined: Vec<&str> = schema.iter().map(|e| e.key.as_str()).collect();
    let all_keys_space = all_keys_joined.join(" ");
    let _ = writeln!(
        out,
        "complete -c clivuu -n '__fish_seen_subcommand_from config; and __fish_seen_subcommand_from get set; and not __fish_seen_subcommand_from {all_keys_space}' -f -a '{all_keys_space}'"
    );
    for entry in schema.iter().filter(|e| !e.values.is_empty()) {
        let _ = writeln!(
            out,
            "complete -c clivuu -n '__fish_seen_subcommand_from config; and __fish_seen_subcommand_from set; and __fish_seen_subcommand_from {}' -f -a '{}'",
            entry.key,
            entry.values.join(" ")
        );
    }

    let worker_ops_list: Vec<&str> = WORKER_OPS.iter().map(|&(op, _)| op).collect();
    let worker_ops_joined = worker_ops_list.join(" ");
    for &(op, sig) in WORKER_OPS {
        let _ = writeln!(
            out,
            "complete -c clivuu -n '__fish_seen_subcommand_from worker; and not __fish_seen_subcommand_from {worker_ops_joined}' -f -a '{op}' -d '{}'",
            escape_fish_single_quoted(sig)
        );
    }

    out
}
