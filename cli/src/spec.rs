//! Declarative CLI specification shared across argument parsing, localized help, and shell completions.
use clivuu_core::i18n::t;

/// Command grouping displayed in root `--help` output.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    Capture,
    Pins,
    Groups,
    Tools,
    Service,
}

impl Category {
    pub const ALL: [Self; 5] = [
        Self::Capture,
        Self::Pins,
        Self::Groups,
        Self::Tools,
        Self::Service,
    ];

    /// Localized heading for the command category.
    pub fn label(self) -> String {
        match self {
            Self::Capture => t("Capture & Recording"),
            Self::Pins => t("Pins & Clipboard"),
            Self::Groups => t("Pin Groups"),
            Self::Tools => t("History & Utilities"),
            Self::Service => t("Service & Configuration"),
        }
    }
}

/// Mutual-exclusion group enforced during argument parsing and documented in `--help`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConflictGroup {
    CaptureRange,
    PasteSource,
}

impl ConflictGroup {
    pub const ALL: [Self; 2] = [Self::CaptureRange, Self::PasteSource];

    /// Localized label describing the mutually exclusive option set.
    pub fn label(self) -> String {
        match self {
            Self::CaptureRange => t("Capture selection range"),
            Self::PasteSource => t("Pin content source"),
        }
    }

    /// Localized error returned when multiple options in the same group are combined.
    pub fn conflict_error(self) -> String {
        match self {
            Self::CaptureRange => t("Capture range options cannot be combined"),
            Self::PasteSource => t("Choose one paste source: clipboard, text, HTML or files"),
        }
    }
}

/// Value parsing behavior attached to an [`OptionSpec`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OptionKind {
    Flag,
    Area,
    Size,
    Delay,
    Output,
    Position,
    PlainText,
    HtmlText,
    Files,
    TextValue,
}

/// Metadata for one CLI flag accepted by a command.
#[derive(Clone, Copy, Debug)]
pub struct OptionSpec {
    pub short: Option<&'static str>,
    pub long: &'static str,
    pub placeholder: &'static str,
    pub kind: OptionKind,
    pub mode_value: Option<&'static str>,
    pub conflict_group: Option<ConflictGroup>,
    pub accepts_files: bool,
    pub completion_choices: &'static [&'static str],
    pub summary: fn() -> String,
}

impl OptionSpec {
    /// Whether `token` matches this option's short or long spelling.
    pub fn matches(&self, token: &str) -> bool {
        self.long == token || self.short == Some(token)
    }

    /// Canonical flag display label (`-o, --output TARGET` or `--area X Y W H`).
    pub fn usage_label(&self) -> String {
        let mut out = String::new();
        if let Some(short) = self.short {
            out.push_str(short);
            out.push_str(", ");
        }
        out.push_str(self.long);
        if !self.placeholder.is_empty() {
            out.push(' ');
            out.push_str(self.placeholder);
        }
        out
    }
}

/// Positional argument accepted by a command.
#[derive(Clone, Copy, Debug)]
pub struct PositionalSpec {
    pub name: &'static str,
    pub required: bool,
    pub accepts_files: bool,
    pub choices: &'static [&'static str],
    pub summary: fn() -> String,
}

impl PositionalSpec {
    /// Bracketed positional syntax (`<FILE>` when required, `[NAME]` when optional).
    pub fn syntax(&self) -> String {
        if self.required {
            format!("<{}>", self.name)
        } else {
            format!("[{}]", self.name)
        }
    }
}

/// Nested subcommand metadata (used by `clivuu config`).
#[derive(Clone, Copy, Debug)]
pub struct SubcommandSpec {
    pub name: &'static str,
    pub usage: &'static str,
    pub summary: fn() -> String,
    pub details: Option<fn() -> String>,
}

/// Runtime handler route for a command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DispatchKind {
    Desktop,
    LocalPortal,
    LocalConfig,
    LocalCompletions,
    LocalDocs,
    LocalHelp,
    LocalWorker,
}

/// Full specification for a top-level CLI command.
#[derive(Clone, Copy, Debug)]
pub struct CommandSpec {
    pub name: &'static str,
    pub category: Category,
    pub dispatch: DispatchKind,
    pub summary: fn() -> String,
    pub details: Option<fn() -> String>,
    pub options: &'static [OptionSpec],
    pub positional: Option<PositionalSpec>,
    pub subcommands: &'static [SubcommandSpec],
    pub examples: &'static [&'static str],
}

impl CommandSpec {
    /// Find an option accepted by this command by its short or long flag.
    pub fn find_option(&self, token: &str) -> Option<&'static OptionSpec> {
        self.options.iter().find(|opt| opt.matches(token))
    }

    /// Find a nested subcommand by name.
    pub fn find_subcommand(&self, name: &str) -> Option<&'static SubcommandSpec> {
        self.subcommands.iter().find(|sub| sub.name == name)
    }
}

/// Supported shell targets for `clivuu completions`.
pub const SHELL_CHOICES: &[&str] = &["bash", "zsh", "fish"];

/// Known internal operations for `clivuu worker`.
pub const WORKER_OPS: &[(&str, &str)] = &[
    ("pin", "<SRC> <DST>"),
    ("normalize", "<SRC> <DST>"),
    ("crop", "<SRC> <DST> <RECT_JSON>"),
    ("composite-cursor", "<SRC> <CURSOR> <DST> <RECT_JSON>"),
    ("transform", "<SRC> <DST> <OP>"),
    ("compose", "<FRAMES_JSON> <DST> <OPTIONS_JSON>"),
    ("match", "<PREV> <CURR>"),
    ("stitch", "<PATHS_JSON> <DST>"),
    ("ocr", "<IMAGE> <LANGUAGES>"),
    ("ocr-document", ""),
    ("barcode", "<IMAGE>"),
];

const IMAGE_OUTPUT_TARGETS: &[&str] = &[
    "clipboard",
    "file-dialog",
    "quick-save",
    "pin",
    "ocr_clipboard",
    "printer",
    "barcode",
    "success",
    "silent",
    "no-auto-save",
];

const RECORD_OUTPUT_TARGETS: &[&str] = &["quick-save", "file-dialog"];

const OPT_AREA: OptionSpec = OptionSpec {
    short: None,
    long: "--area",
    placeholder: "X Y W H",
    kind: OptionKind::Area,
    mode_value: None,
    conflict_group: Some(ConflictGroup::CaptureRange),
    accepts_files: false,
    completion_choices: &[],
    summary: || {
        t("Capture a fixed rectangular area (supports negative origins; width and height >= 2)")
    },
};

const OPT_SIZE: OptionSpec = OptionSpec {
    short: None,
    long: "--size",
    placeholder: "W H",
    kind: OptionKind::Size,
    mode_value: None,
    conflict_group: Some(ConflictGroup::CaptureRange),
    accepts_files: false,
    completion_choices: &[],
    summary: || {
        t("Preselect a fixed region size before interactive placement (width and height >= 2)")
    },
};

const OPT_FULL: OptionSpec = OptionSpec {
    short: None,
    long: "--full",
    placeholder: "",
    kind: OptionKind::Flag,
    mode_value: Some("full"),
    conflict_group: Some(ConflictGroup::CaptureRange),
    accepts_files: false,
    completion_choices: &[],
    summary: || t("Capture the combined virtual desktop area"),
};

const OPT_ACTIVE_SCREEN: OptionSpec = OptionSpec {
    short: None,
    long: "--active-screen",
    placeholder: "",
    kind: OptionKind::Flag,
    mode_value: Some("active-screen"),
    conflict_group: Some(ConflictGroup::CaptureRange),
    accepts_files: false,
    completion_choices: &[],
    summary: || t("Capture the monitor holding the pointer or focus"),
};

const OPT_LAST: OptionSpec = OptionSpec {
    short: None,
    long: "--last",
    placeholder: "",
    kind: OptionKind::Flag,
    mode_value: Some("last"),
    conflict_group: Some(ConflictGroup::CaptureRange),
    accepts_files: false,
    completion_choices: &[],
    summary: || t("Reuse the previous successful capture region"),
};

const OPT_WINDOW: OptionSpec = OptionSpec {
    short: None,
    long: "--window",
    placeholder: "",
    kind: OptionKind::Flag,
    mode_value: Some("window"),
    conflict_group: Some(ConflictGroup::CaptureRange),
    accepts_files: false,
    completion_choices: &[],
    summary: || t("Pick a single window via ScreenCast or X11"),
};

const OPT_ACTIVE_WINDOW: OptionSpec = OptionSpec {
    short: None,
    long: "--active-window",
    placeholder: "",
    kind: OptionKind::Flag,
    mode_value: Some("active-window"),
    conflict_group: Some(ConflictGroup::CaptureRange),
    accepts_files: false,
    completion_choices: &[],
    summary: || t("Select a single window via ScreenCast or active-window fallback"),
};

const OPT_DELAY: OptionSpec = OptionSpec {
    short: None,
    long: "--delay",
    placeholder: "SECONDS",
    kind: OptionKind::Delay,
    mode_value: None,
    conflict_group: None,
    accepts_files: false,
    completion_choices: &[],
    summary: || t("Wait 0 to 3600 seconds before capturing"),
};

const OPT_IMAGE_OUTPUT: OptionSpec = OptionSpec {
    short: Some("-o"),
    long: "--output",
    placeholder: "TARGET",
    kind: OptionKind::Output,
    mode_value: None,
    conflict_group: None,
    accepts_files: true,
    completion_choices: IMAGE_OUTPUT_TARGETS,
    summary: || {
        t(
            "Destination: clipboard, pin, file-dialog, quick-save, ocr_clipboard, printer, barcode, success, silent, no-auto-save, or .png/.jpg path",
        )
    },
};

const OPT_RECORD_OUTPUT: OptionSpec = OptionSpec {
    short: Some("-o"),
    long: "--output",
    placeholder: "TARGET",
    kind: OptionKind::Output,
    mode_value: None,
    conflict_group: None,
    accepts_files: true,
    completion_choices: RECORD_OUTPUT_TARGETS,
    summary: || t("Destination: .mp4, .webm, or .gif file path, quick-save, or file-dialog"),
};

const OPT_PORTAL_OUTPUT: OptionSpec = OptionSpec {
    short: Some("-o"),
    long: "--output",
    placeholder: "FILE",
    kind: OptionKind::Output,
    mode_value: None,
    conflict_group: None,
    accepts_files: true,
    completion_choices: &[],
    summary: || {
        t("Required destination .png or .jpg file path (existing files are never overwritten)")
    },
};

const OPT_CLIPBOARD: OptionSpec = OptionSpec {
    short: None,
    long: "--clipboard",
    placeholder: "",
    kind: OptionKind::Flag,
    mode_value: None,
    conflict_group: Some(ConflictGroup::PasteSource),
    accepts_files: false,
    completion_choices: &[],
    summary: || t("Pin content from the current clipboard (default when no source flag is given)"),
};

const OPT_PLAIN: OptionSpec = OptionSpec {
    short: None,
    long: "--plain",
    placeholder: "TEXT",
    kind: OptionKind::PlainText,
    mode_value: None,
    conflict_group: Some(ConflictGroup::PasteSource),
    accepts_files: false,
    completion_choices: &[],
    summary: || t("Pin literal plain text up to 40,000 characters without color-card parsing"),
};

const OPT_HTML: OptionSpec = OptionSpec {
    short: None,
    long: "--html",
    placeholder: "HTML",
    kind: OptionKind::HtmlText,
    mode_value: None,
    conflict_group: Some(ConflictGroup::PasteSource),
    accepts_files: false,
    completion_choices: &[],
    summary: || t("Pin HTML markup rendered as rich text (up to 40,000 characters)"),
};

const OPT_FILES: OptionSpec = OptionSpec {
    short: None,
    long: "--files",
    placeholder: "PATH...",
    kind: OptionKind::Files,
    mode_value: None,
    conflict_group: Some(ConflictGroup::PasteSource),
    accepts_files: true,
    completion_choices: &[],
    summary: || {
        t(
            "Pin 1 to 32 image files; directory arguments switch the base folder for subsequent relative paths",
        )
    },
};

const OPT_POS: OptionSpec = OptionSpec {
    short: None,
    long: "--pos",
    placeholder: "X Y",
    kind: OptionKind::Position,
    mode_value: None,
    conflict_group: None,
    accepts_files: false,
    completion_choices: &[],
    summary: || t("Initial logical screen coordinates for the created pin"),
};

const SNIP_OPTIONS: &[OptionSpec] = &[
    OPT_AREA,
    OPT_SIZE,
    OPT_FULL,
    OPT_ACTIVE_SCREEN,
    OPT_LAST,
    OPT_WINDOW,
    OPT_ACTIVE_WINDOW,
    OPT_DELAY,
    OPT_IMAGE_OUTPUT,
];

const SCROLL_OPTIONS: &[OptionSpec] = &[OPT_AREA, OPT_IMAGE_OUTPUT];
const RECORD_OPTIONS: &[OptionSpec] = &[OPT_AREA, OPT_RECORD_OUTPUT];
const PORTAL_OPTIONS: &[OptionSpec] = &[OPT_PORTAL_OUTPUT];
const PASTE_OPTIONS: &[OptionSpec] = &[OPT_CLIPBOARD, OPT_PLAIN, OPT_HTML, OPT_FILES, OPT_POS];

const OPT_GROUP_COLOR: OptionSpec = OptionSpec {
    short: None,
    long: "--color",
    placeholder: "COLOR",
    kind: OptionKind::TextValue,
    mode_value: None,
    conflict_group: None,
    accepts_files: false,
    completion_choices: &["none"],
    summary: || t("Optional #RRGGBB color tag for the pin group, or none"),
};

const OPT_NO_SWITCH: OptionSpec = OptionSpec {
    short: None,
    long: "--no-switch",
    placeholder: "",
    kind: OptionKind::Flag,
    mode_value: None,
    conflict_group: None,
    accepts_files: false,
    completion_choices: &[],
    summary: || t("Create the group without switching away from the active group"),
};

const OPT_GROUP_PROMPT: OptionSpec = OptionSpec {
    short: None,
    long: "--prompt",
    placeholder: "",
    kind: OptionKind::Flag,
    mode_value: None,
    conflict_group: None,
    accepts_files: false,
    completion_choices: &[],
    summary: || t("Open the graphical Create Pin Group dialog"),
};

const OPT_GROUP_TO: OptionSpec = OptionSpec {
    short: None,
    long: "--to",
    placeholder: "NEW_NAME",
    kind: OptionKind::TextValue,
    mode_value: None,
    conflict_group: None,
    accepts_files: false,
    completion_choices: &[],
    summary: || t("New name for the pin group"),
};

const OPT_GROUP_MOVE_TO: OptionSpec = OptionSpec {
    short: None,
    long: "--move-to",
    placeholder: "GROUP",
    kind: OptionKind::TextValue,
    mode_value: None,
    conflict_group: None,
    accepts_files: false,
    completion_choices: &["default"],
    summary: || t("Move pins to another group instead of destroying them"),
};

const CREATE_GROUP_OPTIONS: &[OptionSpec] = &[OPT_GROUP_COLOR, OPT_NO_SWITCH, OPT_GROUP_PROMPT];
const RENAME_GROUP_OPTIONS: &[OptionSpec] = &[OPT_GROUP_TO];
const DELETE_GROUP_OPTIONS: &[OptionSpec] = &[OPT_GROUP_MOVE_TO];

const CONFIG_SUBCOMMANDS: &[SubcommandSpec] = &[
    SubcommandSpec {
        name: "path",
        usage: "path",
        summary: || t("Print the active config.ini file path"),
        details: Some(|| {
            t(
                "Resolves XDG_CONFIG_HOME/clivuu/config.ini (falling back to ~/.config/clivuu/config.ini).",
            )
        }),
    },
    SubcommandSpec {
        name: "dump",
        usage: "dump",
        summary: || t("Print the full merged configuration as JSON"),
        details: Some(|| {
            t(
                "Combines built-in schema defaults with non-default overrides currently stored in config.ini.",
            )
        }),
    },
    SubcommandSpec {
        name: "defaults",
        usage: "defaults",
        summary: || t("Print built-in default values as JSON"),
        details: Some(|| {
            t("Outputs every key from core/config-defaults.json with its factory default value.")
        }),
    },
    SubcommandSpec {
        name: "get",
        usage: "get <KEY>",
        summary: || t("Read a single effective configuration value"),
        details: Some(|| {
            t(
                "Prints the active JSON value for KEY after merging user overrides onto schema defaults.",
            )
        }),
    },
    SubcommandSpec {
        name: "set",
        usage: "set <KEY> <VALUE>",
        summary: || t("Validate and save a single configuration key"),
        details: Some(|| {
            t(
                "Validates KEY and VALUE against the schema, normalizes #rrggbb colors to #rrggbbaa, and removes overrides equal to default.",
            )
        }),
    },
    SubcommandSpec {
        name: "apply",
        usage: "apply <JSON>",
        summary: || t("Validate and merge a JSON object of settings"),
        details: Some(|| {
            t(
                "Applies multiple configuration entries atomically after validating each key against core/config-defaults.json.",
            )
        }),
    },
];

/// Canonical registry of all Clivuu CLI commands.
pub const COMMANDS: &[CommandSpec] = &[
    // Capture & Recording
    CommandSpec {
        name: "snip",
        category: Category::Capture,
        dispatch: DispatchKind::Desktop,
        summary: || t("Capture a screenshot or open the interactive selector"),
        details: Some(|| {
            t(
                "With no range flag, opens the frozen selection overlay. Output paths ending in .png, .jpg, or .jpeg write directly without opening a file dialog; combined targets containing ';' or 'exec(' are rejected.",
            )
        }),
        options: SNIP_OPTIONS,
        positional: None,
        subcommands: &[],
        examples: &[
            "clivuu snip",
            "clivuu snip --full -o /tmp/desktop.png",
            "clivuu snip --area -1920 0 800 600 -o pin",
            "clivuu snip --delay 2 --active-screen -o clipboard",
        ],
    },
    CommandSpec {
        name: "scroll",
        category: Category::Capture,
        dispatch: DispatchKind::Desktop,
        summary: || t("Capture a long scrolling screenshot"),
        details: Some(|| {
            t(
                "Captures frames vertically or horizontally and stitches overlaps cleanly. Pass --area to skip interactive region selection.",
            )
        }),
        options: SCROLL_OPTIONS,
        positional: None,
        subcommands: &[],
        examples: &[
            "clivuu scroll",
            "clivuu scroll --area 100 100 800 600 -o /tmp/long.png",
        ],
    },
    CommandSpec {
        name: "record",
        category: Category::Capture,
        dispatch: DispatchKind::Desktop,
        summary: || t("Record a screen region to MP4, WebM, or GIF"),
        details: Some(|| {
            t(
                "Interactive runs without -o open the setup bar and playback editor in Normal mode; passing --area or a file path enters Quick mode.",
            )
        }),
        options: RECORD_OPTIONS,
        positional: None,
        subcommands: &[],
        examples: &[
            "clivuu record",
            "clivuu record -o /tmp/demo.mp4",
            "clivuu record --area 0 0 1280 720 -o /tmp/demo.webm",
        ],
    },
    CommandSpec {
        name: "pause-recording",
        category: Category::Capture,
        dispatch: DispatchKind::Desktop,
        summary: || t("Pause or resume the active recording"),
        details: None,
        options: &[],
        positional: None,
        subcommands: &[],
        examples: &["clivuu pause-recording"],
    },
    CommandSpec {
        name: "stop-recording",
        category: Category::Capture,
        dispatch: DispatchKind::Desktop,
        summary: || t("Stop the active recording and finalize output"),
        details: None,
        options: &[],
        positional: None,
        subcommands: &[],
        examples: &["clivuu stop-recording"],
    },
    CommandSpec {
        name: "portal",
        category: Category::Capture,
        dispatch: DispatchKind::LocalPortal,
        summary: || {
            t("Capture directly via the desktop Screenshot portal without the background service")
        },
        details: Some(|| {
            t(
                "Calls org.freedesktop.portal.Screenshot non-interactively, saves to -o FILE.png/.jpg without overwriting existing files, and exits without starting GTK.",
            )
        }),
        options: PORTAL_OPTIONS,
        positional: None,
        subcommands: &[],
        examples: &["clivuu portal -o /tmp/portal.png"],
    },
    // Pins & Clipboard
    CommandSpec {
        name: "paste",
        category: Category::Pins,
        dispatch: DispatchKind::Desktop,
        summary: || t("Create a floating pin from the clipboard, text, HTML, or image files"),
        details: Some(|| {
            t(
                "During an active capture selection, running 'clivuu paste' triggers the Pin action cleanly. Normally, matching content brings an existing pin forward, and an empty clipboard reopens the last closed pin.",
            )
        }),
        options: PASTE_OPTIONS,
        positional: None,
        subcommands: &[],
        examples: &[
            "clivuu paste",
            "clivuu paste --plain 'Literal text' --pos 120 80",
            "clivuu paste --html '<b>Rich note</b>'",
            "clivuu paste --files ~/Pictures shot.png nested/two.jpg",
        ],
    },
    CommandSpec {
        name: "reopen-pin",
        category: Category::Pins,
        dispatch: DispatchKind::Desktop,
        summary: || t("Reopen the most recently closed pin"),
        details: None,
        options: &[],
        positional: None,
        subcommands: &[],
        examples: &[],
    },
    CommandSpec {
        name: "reopen-pins",
        category: Category::Pins,
        dispatch: DispatchKind::Desktop,
        summary: || t("Reopen all closed pins kept in history"),
        details: None,
        options: &[],
        positional: None,
        subcommands: &[],
        examples: &[],
    },
    CommandSpec {
        name: "close-pins",
        category: Category::Pins,
        dispatch: DispatchKind::Desktop,
        summary: || t("Close visible pins in the active group (recoverable)"),
        details: None,
        options: &[],
        positional: None,
        subcommands: &[],
        examples: &[],
    },
    CommandSpec {
        name: "destroy-closed-pins",
        category: Category::Pins,
        dispatch: DispatchKind::Desktop,
        summary: || t("Permanently clear closed-pin history"),
        details: None,
        options: &[],
        positional: None,
        subcommands: &[],
        examples: &[],
    },
    CommandSpec {
        name: "show-images",
        category: Category::Pins,
        dispatch: DispatchKind::Desktop,
        summary: || t("Show all pins in the active group"),
        details: None,
        options: &[],
        positional: None,
        subcommands: &[],
        examples: &[],
    },
    CommandSpec {
        name: "hide-images",
        category: Category::Pins,
        dispatch: DispatchKind::Desktop,
        summary: || t("Hide all pins in the active group"),
        details: None,
        options: &[],
        positional: None,
        subcommands: &[],
        examples: &[],
    },
    CommandSpec {
        name: "toggle-images",
        category: Category::Pins,
        dispatch: DispatchKind::Desktop,
        summary: || t("Toggle pin visibility in the active group"),
        details: None,
        options: &[],
        positional: None,
        subcommands: &[],
        examples: &[],
    },
    CommandSpec {
        name: "toggle-click-through",
        category: Category::Pins,
        dispatch: DispatchKind::Desktop,
        summary: || t("Toggle pointer click-through for all pins"),
        details: None,
        options: &[],
        positional: None,
        subcommands: &[],
        examples: &[],
    },
    CommandSpec {
        name: "no-click-through",
        category: Category::Pins,
        dispatch: DispatchKind::Desktop,
        summary: || t("Disable click-through across all pins"),
        details: None,
        options: &[],
        positional: None,
        subcommands: &[],
        examples: &[],
    },
    // Pin Groups
    CommandSpec {
        name: "create-group",
        category: Category::Groups,
        dispatch: DispatchKind::Desktop,
        summary: || t("Create and switch to a pin group"),
        details: None,
        options: CREATE_GROUP_OPTIONS,
        positional: Some(PositionalSpec {
            name: "NAME",
            required: false,
            accepts_files: false,
            choices: &[],
            summary: || t("Optional group name (generated automatically if omitted)"),
        }),
        subcommands: &[],
        examples: &[
            "clivuu create-group review",
            "clivuu create-group review --color '#bd93f9' --no-switch",
            "clivuu create-group --prompt",
        ],
    },
    CommandSpec {
        name: "switch-group",
        category: Category::Groups,
        dispatch: DispatchKind::Desktop,
        summary: || t("Switch to a pin group or cycle to the next group"),
        details: None,
        options: &[],
        positional: Some(PositionalSpec {
            name: "NAME",
            required: false,
            accepts_files: false,
            choices: &[],
            summary: || t("Target group name; cycles to the next group when omitted"),
        }),
        subcommands: &[],
        examples: &["clivuu switch-group", "clivuu switch-group review"],
    },
    CommandSpec {
        name: "switch-group-prev",
        category: Category::Groups,
        dispatch: DispatchKind::Desktop,
        summary: || t("Switch back to the previous pin group"),
        details: None,
        options: &[],
        positional: None,
        subcommands: &[],
        examples: &["clivuu switch-group-prev"],
    },
    CommandSpec {
        name: "switch-groups",
        category: Category::Groups,
        dispatch: DispatchKind::Desktop,
        summary: || t("Open the interactive pin group switcher"),
        details: None,
        options: &[],
        positional: None,
        subcommands: &[],
        examples: &["clivuu switch-groups"],
    },
    CommandSpec {
        name: "show-group-manager",
        category: Category::Groups,
        dispatch: DispatchKind::Desktop,
        summary: || t("Open the pin group manager window"),
        details: None,
        options: &[],
        positional: None,
        subcommands: &[],
        examples: &["clivuu show-group-manager"],
    },
    CommandSpec {
        name: "rename-group",
        category: Category::Groups,
        dispatch: DispatchKind::Desktop,
        summary: || t("Rename an existing pin group"),
        details: None,
        options: RENAME_GROUP_OPTIONS,
        positional: Some(PositionalSpec {
            name: "NAME",
            required: true,
            accepts_files: false,
            choices: &[],
            summary: || t("Existing custom pin group name"),
        }),
        subcommands: &[],
        examples: &[
            "clivuu rename-group review --to approved",
            "clivuu rename-group review approved",
        ],
    },
    CommandSpec {
        name: "delete-group",
        category: Category::Groups,
        dispatch: DispatchKind::Desktop,
        summary: || t("Delete a custom pin group and destroy or migrate its pins"),
        details: None,
        options: DELETE_GROUP_OPTIONS,
        positional: Some(PositionalSpec {
            name: "NAME",
            required: true,
            accepts_files: false,
            choices: &[],
            summary: || t("Custom pin group name to delete"),
        }),
        subcommands: &[],
        examples: &[
            "clivuu delete-group review --move-to default",
            "clivuu delete-group scratch",
        ],
    },
    CommandSpec {
        name: "remove-empty-groups",
        category: Category::Groups,
        dispatch: DispatchKind::Desktop,
        summary: || t("Remove inactive empty pin groups"),
        details: None,
        options: &[],
        positional: None,
        subcommands: &[],
        examples: &["clivuu remove-empty-groups"],
    },
    CommandSpec {
        name: "empty-group",
        category: Category::Groups,
        dispatch: DispatchKind::Desktop,
        summary: || t("Destroy every pin in the active or named group"),
        details: None,
        options: &[],
        positional: Some(PositionalSpec {
            name: "NAME",
            required: false,
            accepts_files: false,
            choices: &[],
            summary: || t("Optional target group name; defaults to the active group"),
        }),
        subcommands: &[],
        examples: &["clivuu empty-group", "clivuu empty-group review"],
    },
    CommandSpec {
        name: "groups",
        category: Category::Groups,
        dispatch: DispatchKind::Desktop,
        summary: || t("Return pin groups and status as JSON"),
        details: None,
        options: &[],
        positional: None,
        subcommands: &[],
        examples: &["clivuu groups"],
    },
    // History & Utilities
    CommandSpec {
        name: "capture-history",
        category: Category::Tools,
        dispatch: DispatchKind::Desktop,
        summary: || t("List in-memory captures from newest to oldest as JSON"),
        details: None,
        options: &[],
        positional: None,
        subcommands: &[],
        examples: &[],
    },
    CommandSpec {
        name: "reopen-capture",
        category: Category::Tools,
        dispatch: DispatchKind::Desktop,
        summary: || t("Pin a capture from in-memory history by index"),
        details: None,
        options: &[],
        positional: Some(PositionalSpec {
            name: "INDEX",
            required: false,
            accepts_files: false,
            choices: &[],
            summary: || t("Zero-based history index (defaults to 0, the newest capture)"),
        }),
        subcommands: &[],
        examples: &["clivuu reopen-capture", "clivuu reopen-capture 1"],
    },
    CommandSpec {
        name: "clear-snip-history",
        category: Category::Tools,
        dispatch: DispatchKind::Desktop,
        summary: || t("Clear in-memory captures, saved areas, and the last region"),
        details: None,
        options: &[],
        positional: None,
        subcommands: &[],
        examples: &[],
    },
    CommandSpec {
        name: "pick-color",
        category: Category::Tools,
        dispatch: DispatchKind::Desktop,
        summary: || t("Pick a screen pixel color and copy its HEX code"),
        details: None,
        options: &[],
        positional: None,
        subcommands: &[],
        examples: &["clivuu pick-color"],
    },
    CommandSpec {
        name: "barcode-scan",
        category: Category::Tools,
        dispatch: DispatchKind::Desktop,
        summary: || t("Select a screen region and decode a QR or barcode with zbarimg"),
        details: None,
        options: &[],
        positional: None,
        subcommands: &[],
        examples: &["clivuu barcode-scan"],
    },
    CommandSpec {
        name: "barcode-decode",
        category: Category::Tools,
        dispatch: DispatchKind::Desktop,
        summary: || t("Decode a QR or barcode from a local image file"),
        details: None,
        options: &[],
        positional: Some(PositionalSpec {
            name: "FILE",
            required: true,
            accepts_files: true,
            choices: &[],
            summary: || t("Path to a local image containing a QR or barcode"),
        }),
        subcommands: &[],
        examples: &["clivuu barcode-decode ./qr.png"],
    },
    // Service & Configuration
    CommandSpec {
        name: "start",
        category: Category::Service,
        dispatch: DispatchKind::Desktop,
        summary: || t("Start or contact the background desktop service"),
        details: Some(|| t("Automatically invoked when running 'clivuu' with no arguments.")),
        options: &[],
        positional: None,
        subcommands: &[],
        examples: &["clivuu", "clivuu start"],
    },
    CommandSpec {
        name: "status",
        category: Category::Service,
        dispatch: DispatchKind::Desktop,
        summary: || t("Print session capabilities, backend info, and paths as JSON"),
        details: Some(|| {
            t(
                "Queries desktop environment, compositor backend, config path, and external tool availability without opening UI windows.",
            )
        }),
        options: &[],
        positional: None,
        subcommands: &[],
        examples: &["clivuu status"],
    },
    CommandSpec {
        name: "open-preferences",
        category: Category::Service,
        dispatch: DispatchKind::Desktop,
        summary: || t("Open the Preferences window"),
        details: None,
        options: &[],
        positional: None,
        subcommands: &[],
        examples: &["clivuu open-preferences"],
    },
    CommandSpec {
        name: "config",
        category: Category::Service,
        dispatch: DispatchKind::LocalConfig,
        summary: || t("Read, validate, or modify persistent INI settings"),
        details: Some(|| {
            t(
                "All writes validate keys and values against core/config-defaults.json without initializing GTK.",
            )
        }),
        options: &[],
        positional: None,
        subcommands: CONFIG_SUBCOMMANDS,
        examples: &[
            "clivuu config path",
            "clivuu config dump",
            "clivuu config get theme",
            "clivuu config set theme dark",
            "clivuu config apply '{\"theme\":\"dark\",\"accent-color\":\"#bd93f9ff\"}'",
        ],
    },
    CommandSpec {
        name: "completions",
        category: Category::Service,
        dispatch: DispatchKind::LocalCompletions,
        summary: || t("Generate shell completion scripts (bash, zsh, fish)"),
        details: Some(|| {
            t(
                "Prints a completion script to stdout including all commands, flags, and configuration schema keys/choices.",
            )
        }),
        options: &[],
        positional: Some(PositionalSpec {
            name: "SHELL",
            required: true,
            accepts_files: false,
            choices: SHELL_CHOICES,
            summary: || t("Target shell: bash, zsh, or fish"),
        }),
        subcommands: &[],
        examples: &[
            "clivuu completions bash > ~/.local/share/bash-completion/completions/clivuu",
            "clivuu completions zsh > ~/.zfunc/_clivuu",
            "clivuu completions fish > ~/.config/fish/completions/clivuu.fish",
        ],
    },
    CommandSpec {
        name: "docs",
        category: Category::Service,
        dispatch: DispatchKind::LocalDocs,
        summary: || t("Print a local manual overview and documentation paths"),
        details: Some(|| {
            t(
                "Shows available guide topics, discovered documentation file locations on disk, and helpful CLI entry points.",
            )
        }),
        options: &[],
        positional: None,
        subcommands: &[],
        examples: &["clivuu docs"],
    },
    CommandSpec {
        name: "help",
        category: Category::Service,
        dispatch: DispatchKind::LocalHelp,
        summary: || t("Show root overview or help for a specific command"),
        details: None,
        options: &[],
        positional: Some(PositionalSpec {
            name: "COMMAND",
            required: false,
            accepts_files: false,
            choices: &[],
            summary: || t("Command name to inspect (or 'config <subcommand>')"),
        }),
        subcommands: &[],
        examples: &["clivuu help", "clivuu help snip", "clivuu help config set"],
    },
    CommandSpec {
        name: "enable-hotkeys",
        category: Category::Service,
        dispatch: DispatchKind::Desktop,
        summary: || t("Report global shortcut setup guidance as JSON"),
        details: None,
        options: &[],
        positional: None,
        subcommands: &[],
        examples: &[],
    },
    CommandSpec {
        name: "disable-hotkeys",
        category: Category::Service,
        dispatch: DispatchKind::Desktop,
        summary: || t("Report global shortcut setup guidance as JSON"),
        details: None,
        options: &[],
        positional: None,
        subcommands: &[],
        examples: &[],
    },
    CommandSpec {
        name: "toggle-hotkeys",
        category: Category::Service,
        dispatch: DispatchKind::Desktop,
        summary: || t("Report global shortcut setup guidance as JSON"),
        details: None,
        options: &[],
        positional: None,
        subcommands: &[],
        examples: &[],
    },
    CommandSpec {
        name: "restart",
        category: Category::Service,
        dispatch: DispatchKind::Desktop,
        summary: || t("Save state and restart the background desktop service"),
        details: None,
        options: &[],
        positional: None,
        subcommands: &[],
        examples: &[],
    },
    CommandSpec {
        name: "exit",
        category: Category::Service,
        dispatch: DispatchKind::Desktop,
        summary: || t("Save state and stop the background desktop service"),
        details: None,
        options: &[],
        positional: None,
        subcommands: &[],
        examples: &[],
    },
    CommandSpec {
        name: "worker",
        category: Category::Service,
        dispatch: DispatchKind::LocalWorker,
        summary: || t("Internal image, OCR, and stitching worker invoked by clivuu-desktop"),
        details: Some(|| {
            t(
                "Runs compute-intensive image, Tesseract OCR, zbarimg, and scroll-stitching tasks out-of-process without initializing GTK.",
            )
        }),
        options: &[],
        positional: None,
        subcommands: &[],
        examples: &[],
    },
];

/// Look up a top-level command specification by exact name.
pub fn find_command(name: &str) -> Option<&'static CommandSpec> {
    COMMANDS.iter().find(|cmd| cmd.name == name)
}

/// Check whether `token` matches an option flag on any command in [`COMMANDS`].
pub fn is_known_flag_anywhere(token: &str) -> bool {
    COMMANDS.iter().any(|cmd| cmd.find_option(token).is_some())
}
