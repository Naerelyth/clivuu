mod completions;
mod desktop;
mod help;
mod spec;

use crate::completions::Shell;
use crate::help::HelpTarget;
use crate::spec::{CommandSpec, ConflictGroup, DispatchKind, OptionKind, OptionSpec};
use clivuu_core::i18n::{t, tf};
use clivuu_core::{Result, image_ops, ocr, protocol, stitch};
use serde_json::{Value, json};
use std::{env, fs, path::PathBuf, process::Command};

#[derive(Debug)]
enum Action {
    Desktop(Value),
    Portal(Value),
    Config(Vec<String>),
    Completions(Shell),
    Docs,
    Help(HelpTarget),
    Version,
    Worker(Vec<String>),
}

fn integer(args: &[String], index: &mut usize) -> Result<i64> {
    *index += 1;
    Ok(args
        .get(*index)
        .ok_or(t("An option is missing its numeric value"))?
        .parse()?)
}

fn validate_min_size(w: i64, h: i64) -> Result<(u32, u32)> {
    if w < 2 || h < 2 {
        return Err(t("Width and height must be at least 2 pixels").into());
    }
    Ok((u32::try_from(w)?, u32::try_from(h)?))
}

fn apply_option(
    opt: &OptionSpec,
    args: &[String],
    index: &mut usize,
    request: &mut Value,
) -> Result<()> {
    match opt.kind {
        OptionKind::Flag => {
            if let Some(mode) = opt.mode_value {
                request["mode"] = json!(mode);
            } else if opt.long == "--clipboard" {
                request["source"] = json!("clipboard");
            } else if opt.long == "--no-switch" {
                request["activate"] = json!(false);
            } else if opt.long == "--prompt" {
                request["prompt"] = json!(true);
            }
        }
        OptionKind::Area => {
            let x = integer(args, index)?;
            let y = integer(args, index)?;
            let w = integer(args, index)?;
            let h = integer(args, index)?;
            let (width, height) = validate_min_size(w, h)?;
            request["rect"] = serde_json::to_value(
                protocol::Rect {
                    x,
                    y,
                    width,
                    height,
                }
                .validate()?,
            )?;
        }
        OptionKind::Size => {
            let w = integer(args, index)?;
            let h = integer(args, index)?;
            let (width, height) = validate_min_size(w, h)?;
            request["size"] = json!([width, height]);
        }
        OptionKind::Delay => {
            *index += 1;
            let seconds: f64 = args
                .get(*index)
                .ok_or(t("--delay requires a value in seconds"))?
                .parse()?;
            if !seconds.is_finite() || !(0.0..=3600.0).contains(&seconds) {
                return Err(t("The delay must be between 0 and 3600 seconds").into());
            }
            request["delay"] = json!(seconds);
        }
        OptionKind::Output => {
            *index += 1;
            let output = args
                .get(*index)
                .ok_or(t("--output requires a destination"))?;
            if output.contains(';') || output.starts_with("exec(") {
                return Err(
                    t("Combined output and external command execution are not supported").into(),
                );
            }
            request["output"] = json!(output);
        }
        OptionKind::Position => {
            let x = integer(args, index)?;
            let y = integer(args, index)?;
            request["position"] = json!([x, y]);
        }
        OptionKind::PlainText | OptionKind::HtmlText => {
            let html = opt.kind == OptionKind::HtmlText;
            *index += 1;
            let text = args.get(*index).ok_or(t("Missing pin content"))?;
            if text.chars().count() > clivuu_core::paste::MAX_TEXT {
                return Err(t("Pin text is too long").into());
            }
            request["source"] = json!(if html { "html" } else { "plain" });
            request["text"] = json!(text);
            request["html"] = json!(html);
        }
        OptionKind::Files => {
            request["source"] = json!("files");
            let mut files = Vec::new();
            let mut base = env::current_dir()?;
            while *index + 1 < args.len()
                && !args[*index + 1].starts_with("--")
                && args[*index + 1] != "-h"
            {
                *index += 1;
                let p = PathBuf::from(&args[*index]);
                let p = if p.is_absolute() { p } else { base.join(p) };
                if p.is_dir() {
                    base = fs::canonicalize(p)?;
                    continue;
                }
                files.push(fs::canonicalize(p)?.to_string_lossy().into_owned());
            }
            if files.is_empty() || files.len() > 32 {
                return Err(t("--files requires 1 to 32 image files").into());
            }
            request["files"] = json!(files);
        }
        OptionKind::TextValue => {
            *index += 1;
            let value = args
                .get(*index)
                .ok_or_else(|| tf("Option {0} requires a value", &[opt.long]))?;
            match opt.long {
                "--color" => {
                    clivuu_core::pins::normalize_group_color(Some(value))?;
                    request["color"] = json!(value);
                }
                "--to" => request["to"] = json!(value),
                "--move-to" => request["move_to"] = json!(value),
                _ => unreachable!("unhandled TextValue option"),
            }
        }
    }
    Ok(())
}

fn parse_help(rest: &[String]) -> Result<Action> {
    match rest {
        [] => Ok(Action::Help(HelpTarget::Root)),
        [flag] if flag == "--help" || flag == "-h" => Ok(Action::Help(HelpTarget::Command(
            spec::find_command("help").expect("help spec"),
        ))),
        [name] => {
            let target = spec::find_command(name)
                .ok_or_else(|| tf("Unknown command: {0}; run clivuu --help for usage", &[name]))?;
            Ok(Action::Help(HelpTarget::Command(target)))
        }
        [cmd_name, sub_name] if cmd_name == "config" => {
            let config_cmd = spec::find_command("config").expect("config spec");
            let sub = config_cmd.find_subcommand(sub_name).ok_or_else(|| {
                tf(
                    "Unknown config subcommand: {0}; run clivuu config --help for usage",
                    &[sub_name],
                )
            })?;
            Ok(Action::Help(HelpTarget::Subcommand(config_cmd, sub)))
        }
        _ => Err(t("Unexpected positional argument").into()),
    }
}

fn parse_spec_command(cmd: &'static CommandSpec, rest: &[String]) -> Result<Action> {
    let mut request = json!({ "command": cmd.name });
    let mut seen_groups: Vec<ConflictGroup> = Vec::new();
    let mut end_of_options = false;
    let mut i = 0;

    while i < rest.len() {
        let token = rest[i].as_str();
        if !end_of_options && token == "--" {
            end_of_options = true;
            i += 1;
            continue;
        }
        if !end_of_options && (token == "--help" || token == "-h") {
            return Ok(Action::Help(HelpTarget::Command(cmd)));
        }
        if !end_of_options && token.starts_with('-') {
            if let Some(opt) = cmd.find_option(token) {
                if let Some(group) = opt.conflict_group {
                    if seen_groups.contains(&group) {
                        return Err(group.conflict_error().into());
                    }
                    seen_groups.push(group);
                }
                apply_option(opt, rest, &mut i, &mut request)?;
                i += 1;
                continue;
            }
            if spec::is_known_flag_anywhere(token) {
                return Err(
                    tf("Command {0} does not accept option {1}", &[cmd.name, token]).into(),
                );
            }
            return Err(tf("Unknown option: {0}", &[token]).into());
        }
        if let Some(pos) = cmd.positional {
            if !request["argument"].is_null() {
                if cmd.name == "rename-group" && request["to"].is_null() {
                    request["to"] = json!(token);
                    i += 1;
                    continue;
                }
                return Err(t("Unexpected positional argument").into());
            }
            if !pos.choices.is_empty() && !pos.choices.contains(&token) {
                return Err(tf("Unsupported value for {0}: {1}", &[pos.name, token]).into());
            }
            request["argument"] = json!(token);
        } else {
            return Err(t("Unexpected positional argument").into());
        }
        i += 1;
    }

    if let Some(pos) = cmd.positional
        && pos.required
        && request["argument"].is_null()
    {
        return match cmd.name {
            "barcode-decode" => Err(t("barcode-decode requires a file path").into()),
            "completions" => Err(t("Missing shell name; choose bash, zsh, or fish").into()),
            _ => Err(tf("{0} requires {1}", &[cmd.name, pos.name]).into()),
        };
    }
    if cmd.name == "rename-group" && request["to"].is_null() {
        return Err(t("rename-group requires a new group name").into());
    }

    match cmd.dispatch {
        DispatchKind::LocalCompletions => {
            let shell = Shell::parse(request["argument"].as_str().unwrap_or(""))?;
            Ok(Action::Completions(shell))
        }
        DispatchKind::LocalDocs => Ok(Action::Docs),
        DispatchKind::LocalPortal => {
            if request["output"].is_null() {
                return Err(t("Portal capture requires -o FILE.png").into());
            }
            Ok(Action::Portal(request))
        }
        DispatchKind::Desktop => {
            if cmd.name == "paste" {
                clivuu_core::paste::Request::parse(&request)?;
            }
            Ok(Action::Desktop(request))
        }
        DispatchKind::LocalConfig | DispatchKind::LocalHelp | DispatchKind::LocalWorker => {
            unreachable!("handled before generic command parsing")
        }
    }
}

fn resolve(args: &[String]) -> Result<Action> {
    let Some(first) = args.first().map(String::as_str) else {
        return Ok(Action::Desktop(json!({ "command": "start" })));
    };
    if first == "--help" || first == "-h" {
        return Ok(Action::Help(HelpTarget::Root));
    }
    if first == "--version" || first == "-V" {
        return Ok(Action::Version);
    }
    let cmd = spec::find_command(first).ok_or_else(|| {
        tf(
            "Unknown command: {0}; run clivuu --help for usage",
            &[first],
        )
    })?;
    let rest = &args[1..];
    match cmd.dispatch {
        DispatchKind::LocalWorker => {
            if rest.first().is_some_and(|a| a == "--help" || a == "-h") {
                Ok(Action::Help(HelpTarget::Command(cmd)))
            } else {
                Ok(Action::Worker(rest.to_vec()))
            }
        }
        DispatchKind::LocalConfig => {
            if matches!(rest, [flag] if flag == "--help" || flag == "-h") {
                return Ok(Action::Help(HelpTarget::Command(cmd)));
            }
            if let [sub_name, flag] = rest
                && (flag == "--help" || flag == "-h")
            {
                let sub = cmd.find_subcommand(sub_name).ok_or_else(|| {
                    tf(
                        "Unknown config subcommand: {0}; run clivuu config --help for usage",
                        &[sub_name],
                    )
                })?;
                return Ok(Action::Help(HelpTarget::Subcommand(cmd, sub)));
            }
            Ok(Action::Config(rest.to_vec()))
        }
        DispatchKind::LocalHelp => parse_help(rest),
        DispatchKind::Desktop
        | DispatchKind::LocalPortal
        | DispatchKind::LocalCompletions
        | DispatchKind::LocalDocs => parse_spec_command(cmd, rest),
    }
}

fn worker(args: &[String]) -> Result<()> {
    let operation = args.first().ok_or(t("Missing worker operation"))?.as_str();
    let values = &args[1..];
    let required = match operation {
        "pin" | "match" | "stitch" => 2,
        "ocr" => 2,
        "ocr-document" => 0,
        "barcode" => 1,
        "normalize" => 2,
        "crop" | "compose" | "transform" => 3,
        "composite-cursor" => 4,
        _ => return Err(t("Unknown worker operation").into()),
    };
    if values.len() != required {
        return Err(t("Incorrect number of worker arguments").into());
    }
    match operation {
        "pin" => println!("{}", image_ops::prepare_pin(&values[0], &values[1])?),
        "normalize" => image_ops::save(&image_ops::load(&values[0])?, &values[1])?,
        "crop" => image_ops::crop(&values[0], &values[1], serde_json::from_str(&values[2])?)?,
        "composite-cursor" => image_ops::composite_cursor(
            &values[0],
            &values[1],
            &values[2],
            serde_json::from_str(&values[3])?,
        )?,
        "transform" => image_ops::transform(&values[0], &values[1], &values[2])?,
        "compose" => {
            let frames: Vec<clivuu_core::compose::Frame> =
                serde_json::from_slice(&fs::read(&values[0])?)?;
            println!(
                "{}",
                clivuu_core::compose::compose(
                    &frames,
                    serde_json::from_str(&values[2])?,
                    &values[1]
                )?
            );
        }
        "match" => {
            let previous = stitch::Profile::new(image_ops::load(&values[0])?);
            let current = stitch::Profile::new(image_ops::load(&values[1])?);
            println!(
                "{}",
                serde_json::to_string(&stitch::find_match(&previous, &current)?)?
            );
        }
        "stitch" => {
            let paths: Vec<String> = serde_json::from_slice(&fs::read(&values[0])?)?;
            println!("{}", stitch::stitch(&paths, &values[1])?);
        }
        "ocr" => print!("{}", ocr::recognize(&values[0], &values[1])?),
        "ocr-document" => ocr::worker::serve_stdio()?,
        "barcode" => {
            image_ops::load(&values[0])?;
            let program = "zbarimg";
            let mut command = Command::new(program);
            command.args(["--quiet", "--raw", &values[0]]);
            let result = command
                .output()
                .map_err(|e| tf("Install {0}: {1}", &[program, &e.to_string()]))?;
            if !result.status.success() {
                return Err(String::from_utf8_lossy(&result.stderr)
                    .trim()
                    .to_string()
                    .into());
            }
            print!("{}", String::from_utf8_lossy(&result.stdout));
        }
        _ => unreachable!(),
    }
    Ok(())
}

fn config(args: &[String]) -> Result<()> {
    use clivuu_core::config;
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["dump"] => println!("{}", serde_json::to_string(&config::read()?)?),
        ["defaults"] => println!(
            "{}",
            serde_json::to_string(
                &config::schema()
                    .into_iter()
                    .map(|(k, v)| (k, v["default"].clone()))
                    .collect::<serde_json::Map<_, _>>()
            )?
        ),
        ["path"] => println!("{}", config::path()?.display()),
        ["get", key] => println!(
            "{}",
            config::read()?
                .get(*key)
                .ok_or_else(|| tf("Unknown configuration key: {0}", &[key]))?
        ),
        ["set", key, value] => config::set(key, value)?,
        ["apply", values] => config::apply_file(&config::path()?, serde_json::from_str(values)?)?,
        [] => {
            return Err(t("Missing config subcommand; run clivuu config --help for usage").into());
        }
        ["get"] => return Err(t("Missing configuration key; usage: clivuu config get KEY").into()),
        ["set"] | ["set", _] => {
            return Err(t("Missing key or value; usage: clivuu config set KEY VALUE").into());
        }
        ["apply"] => return Err(t("Missing JSON payload; usage: clivuu config apply JSON").into()),
        _ => {
            return Err(t("Invalid config arguments; run clivuu config --help for usage").into());
        }
    }
    Ok(())
}

fn portal(request: &Value) -> Result<()> {
    use std::{
        collections::HashMap,
        sync::mpsc,
        time::{Duration, SystemTime, UNIX_EPOCH},
    };
    use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value as ZValue};
    let connection = zbus::blocking::Connection::session()?;
    let token = format!(
        "clivuu_{}_{}",
        std::process::id(),
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
    );
    let sender = connection
        .unique_name()
        .ok_or(t("D-Bus has no unique connection name"))?
        .as_str()
        .trim_start_matches(':')
        .replace('.', "_");
    let path = format!("/org/freedesktop/portal/desktop/request/{sender}/{token}");
    let response = zbus::blocking::Proxy::new(
        &connection,
        "org.freedesktop.portal.Desktop",
        path.as_str(),
        "org.freedesktop.portal.Request",
    )?;
    let signals = response.receive_signal("Response")?;
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for message in signals.take(1) {
            let _ = tx.send(
                message
                    .body()
                    .deserialize::<(u32, HashMap<String, OwnedValue>)>(),
            );
        }
    });
    let proxy = zbus::blocking::Proxy::new(
        &connection,
        "org.freedesktop.portal.Desktop",
        "/org/freedesktop/portal/desktop",
        "org.freedesktop.portal.Screenshot",
    )?;
    let options = HashMap::from([
        ("handle_token", ZValue::from(token.as_str())),
        ("interactive", ZValue::from(false)),
    ]);
    let _: OwnedObjectPath = proxy.call("Screenshot", &("", options))?;
    let (code, results) = rx
        .recv_timeout(Duration::from_secs(120))
        .map_err(|_| t("The system screenshot timed out"))??;
    if code != 0 {
        return Err(t("The system screenshot was cancelled or denied").into());
    }
    let uri = results
        .get("uri")
        .ok_or(t("The system returned no image"))?;
    // GIO resolves escaped file URIs without string slicing or shell expansion.
    let uri = <&str>::try_from(uri)?;
    let destination = request["output"]
        .as_str()
        .ok_or(t("Portal capture requires -o FILE.png"))?;
    if ["pin", "clipboard", "quick-save"].contains(&destination) {
        return Err(t("Choose a PNG or JPEG file name").into());
    }
    let result = Command::new("gio").args(["cat", uri]).output()?;
    if !result.status.success() {
        return Err(t("Failed to save the system screenshot").into());
    }
    use clivuu_core::image::export::{self, Source};
    let path = std::path::Path::new(destination);
    export::Format::from_path(path)?;
    let quality = clivuu_core::config::image_quality(&clivuu_core::config::read()?);
    // Preserve gio copy's no-overwrite policy for explicit CLI destinations.
    let mut output = fs::File::create_new(path)?;
    export::write(Source::Png(&result.stdout), path, quality, &mut output)?;
    println!("{}", json!({"path":destination}));
    Ok(())
}

fn main() {
    let preference = clivuu_core::config::read().ok().and_then(|values| {
        values
            .get("language")
            .and_then(Value::as_str)
            .map(str::to_owned)
    });
    clivuu_core::i18n::initialize(preference.as_deref());
    let args: Vec<String> = env::args().skip(1).collect();
    let result = (|| -> Result<()> {
        match resolve(&args)? {
            Action::Help(target) => println!("{}", help::render(target)),
            Action::Version => println!("clivuu {}", env!("CARGO_PKG_VERSION")),
            Action::Docs => println!("{}", help::render_docs()),
            Action::Completions(shell) => print!("{}", completions::generate(shell)),
            Action::Worker(worker_args) => worker(&worker_args)?,
            Action::Config(config_args) => config(&config_args)?,
            Action::Portal(mut request) => {
                request["protocol"] = json!(protocol::VERSION);
                portal(&request)?;
            }
            Action::Desktop(mut request) => {
                request["protocol"] = json!(protocol::VERSION);
                desktop::execute(request)?;
            }
        }
        Ok(())
    })();
    if let Err(error) = result {
        eprintln!("clivuu: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    fn parse(a: &[&str]) -> Result<Value> {
        match resolve(&args(a))? {
            Action::Desktop(v) | Action::Portal(v) => Ok(v),
            other => Err(format!("non-request action: {other:?}").into()),
        }
    }

    #[test]
    fn paste_sources_conflict_and_position_is_validated() {
        for values in [
            vec!["paste", "--clipboard", "--plain", "x"],
            vec!["paste", "--plain", "x", "--html", "<b>y</b>"],
            vec!["paste", "--clipboard", "--clipboard"],
            vec!["paste", "--pos", "1000001", "0"],
            vec!["paste", "--files"],
        ] {
            assert!(parse(&values).is_err(), "{values:?}");
        }
        assert_eq!(
            parse(&["paste", "--clipboard"]).unwrap()["source"],
            "clipboard"
        );
        assert_eq!(
            parse(&["paste", "--html", "<b>你好</b>", "--pos", "-100", "20"]).unwrap()["position"],
            json!([-100, 20])
        );
    }

    #[test]
    fn paste_plain_preserves_help_flag_as_literal_text() {
        let req = parse(&["paste", "--plain", "--help"]).unwrap();
        assert_eq!(req["source"], "plain");
        assert_eq!(req["text"], "--help");
    }

    #[test]
    fn paste_file_base_resolves_nested_relative_directories() {
        let root = env::temp_dir().join(format!("clivuu-cli-paste-{}", std::process::id()));
        fs::create_dir_all(root.join("nested")).unwrap();
        fs::write(root.join("nested/image.png"), b"fixture").unwrap();
        let root_str = root.to_string_lossy().into_owned();
        let req = parse(&["paste", "--files", &root_str, "nested", "image.png"]).unwrap();
        assert_eq!(
            req["files"],
            json!([root.join("nested/image.png").to_string_lossy()])
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn default_launch_starts_without_capture() {
        assert_eq!(parse(&[]).unwrap()["command"], "start");
    }

    #[test]
    fn invalid_coordinates_and_delay() {
        assert!(parse(&["snip", "--area", "0", "0", "-1", "20"]).is_err());
        assert!(parse(&["snip", "--size", "1", "20"]).is_err());
        assert!(parse(&["snip", "--delay", "NaN"]).is_err());
        assert!(parse(&["snip", "--full", "--last"]).is_err());
        assert!(parse(&["snip", "--full", "--full"]).is_err());
    }

    #[test]
    fn preserves_negative_monitor_coordinates() {
        let r = parse(&["snip", "--area", "-1920", "-100", "800", "600", "-o", "pin"]).unwrap();
        assert_eq!(r["rect"]["x"], -1920);
        assert_eq!(r["output"], "pin");
    }

    #[test]
    fn rejects_shell_execution_and_unknown_or_cross_command_flags() {
        assert!(parse(&["snip", "-o", "pin;exec(rm)"]).is_err());
        assert!(parse(&["snip", "--imaginary"]).is_err());
        assert!(parse(&["stop-recording", "--area", "0", "0", "100", "100"]).is_err());
        assert!(parse(&["scroll", "--full"]).is_err());
        assert!(parse(&["record", "--delay", "2"]).is_err());
        assert!(parse(&["snip", "--clipboard"]).is_err());
        assert!(parse(&["paste", "--full"]).is_err());
        assert!(parse(&["status", "-o", "out.png"]).is_err());
    }

    #[test]
    fn validates_positionals_and_end_of_options_separator() {
        assert!(parse(&["status", "unexpected"]).is_err());
        assert!(parse(&["barcode-decode"]).is_err());
        assert!(parse(&["barcode-decode", "a.png", "b.png"]).is_err());
        assert_eq!(
            parse(&["barcode-decode", "--", "-odd.png"]).unwrap()["argument"],
            "-odd.png"
        );
        assert!(parse(&["create-group"]).is_ok());
        assert_eq!(
            parse(&["create-group", "demo"]).unwrap()["argument"],
            "demo"
        );
        let created =
            parse(&["create-group", "demo", "--color", "#bd93f9", "--no-switch"]).unwrap();
        assert_eq!(created["color"], "#bd93f9");
        assert_eq!(created["activate"], false);
        assert!(parse(&["create-group", "demo", "--color", "not-a-color"]).is_err());
        assert_eq!(
            parse(&["create-group", "--prompt"]).unwrap()["prompt"],
            true
        );
        assert!(parse(&["rename-group"]).is_err());
        assert!(parse(&["rename-group", "old"]).is_err());
        let renamed_pos = parse(&["rename-group", "old", "new"]).unwrap();
        assert_eq!(renamed_pos["argument"], "old");
        assert_eq!(renamed_pos["to"], "new");
        let renamed_flag = parse(&["rename-group", "old", "--to", "new"]).unwrap();
        assert_eq!(renamed_flag["argument"], "old");
        assert_eq!(renamed_flag["to"], "new");
        assert!(parse(&["delete-group"]).is_err());
        let deleted = parse(&["delete-group", "old", "--move-to", "default"]).unwrap();
        assert_eq!(deleted["argument"], "old");
        assert_eq!(deleted["move_to"], "default");
        assert!(parse(&["remove-empty-groups"]).is_ok());
        assert!(parse(&["portal"]).is_err());
        assert_eq!(
            parse(&["portal", "-o", "/tmp/p.png"]).unwrap()["output"],
            "/tmp/p.png"
        );
    }

    #[test]
    fn help_and_subcommand_help_resolution() {
        assert!(matches!(
            resolve(&args(&["--help"])).unwrap(),
            Action::Help(HelpTarget::Root)
        ));
        assert!(matches!(
            resolve(&args(&["-h"])).unwrap(),
            Action::Help(HelpTarget::Root)
        ));
        assert!(matches!(
            resolve(&args(&["help"])).unwrap(),
            Action::Help(HelpTarget::Root)
        ));
        assert!(matches!(
            resolve(&args(&["--version"])).unwrap(),
            Action::Version
        ));
        assert!(matches!(resolve(&args(&["-V"])).unwrap(), Action::Version));
        assert!(matches!(
            resolve(&args(&["snip", "--help"])).unwrap(),
            Action::Help(HelpTarget::Command(c)) if c.name == "snip"
        ));
        assert!(matches!(
            resolve(&args(&["snip", "--full", "-h"])).unwrap(),
            Action::Help(HelpTarget::Command(c)) if c.name == "snip"
        ));
        assert!(matches!(
            resolve(&args(&["help", "paste"])).unwrap(),
            Action::Help(HelpTarget::Command(c)) if c.name == "paste"
        ));
        assert!(matches!(
            resolve(&args(&["config", "--help"])).unwrap(),
            Action::Help(HelpTarget::Command(c)) if c.name == "config"
        ));
        assert!(matches!(
            resolve(&args(&["config", "set", "--help"])).unwrap(),
            Action::Help(HelpTarget::Subcommand(c, s)) if c.name == "config" && s.name == "set"
        ));
        assert!(matches!(
            resolve(&args(&["help", "config", "get"])).unwrap(),
            Action::Help(HelpTarget::Subcommand(c, s)) if c.name == "config" && s.name == "get"
        ));
        assert!(resolve(&args(&["config", "nope", "--help"])).is_err());
        assert!(matches!(
            resolve(&args(&["worker", "--help"])).unwrap(),
            Action::Help(HelpTarget::Command(c)) if c.name == "worker"
        ));
        assert!(matches!(
            resolve(&args(&["worker", "pin", "a", "b"])).unwrap(),
            Action::Worker(_)
        ));
        assert!(matches!(resolve(&args(&["docs"])).unwrap(), Action::Docs));
        assert!(resolve(&args(&["docs", "--full"])).is_err());
    }

    #[test]
    fn completions_include_commands_options_and_dynamic_config_schema() {
        assert!(resolve(&args(&["completions"])).is_err());
        assert!(resolve(&args(&["completions", "pwsh"])).is_err());
        let schema_keys: Vec<String> = clivuu_core::config::schema().keys().cloned().collect();
        for (shell_arg, expected_shell, marker) in [
            ("bash", Shell::Bash, "_clivuu()"),
            ("zsh", Shell::Zsh, "#compdef clivuu"),
            ("fish", Shell::Fish, "complete -c clivuu"),
        ] {
            match resolve(&args(&["completions", shell_arg])).unwrap() {
                Action::Completions(shell) => {
                    assert_eq!(shell, expected_shell);
                    let script = completions::generate(shell);
                    assert!(script.contains(marker), "{shell_arg} missing {marker}");
                    for cmd in spec::COMMANDS {
                        assert!(
                            script.contains(cmd.name),
                            "{shell_arg} missing {}",
                            cmd.name
                        );
                    }
                    for key in &schema_keys {
                        assert!(script.contains(key), "{shell_arg} missing schema key {key}");
                    }
                    assert!(script.contains("dark"));
                    assert!(script.contains("light"));
                }
                other => panic!("unexpected action: {other:?}"),
            }
        }
    }

    #[test]
    fn help_and_docs_render_all_commands_without_panicking() {
        let root = help::render(HelpTarget::Root);
        for cmd in spec::COMMANDS {
            assert!(root.contains(cmd.name), "root help missing {}", cmd.name);
            let cmd_help = help::render(HelpTarget::Command(cmd));
            assert!(cmd_help.contains(cmd.name));
            for sub in cmd.subcommands {
                let sub_help = help::render(HelpTarget::Subcommand(cmd, sub));
                assert!(sub_help.contains(sub.name));
            }
        }
        let docs = help::render_docs();
        assert!(docs.contains("clivuu --help"));
        assert!(docs.contains("clivuu completions <SHELL>"));
    }
}
