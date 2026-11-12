// SPDX-License-Identifier: GPL-3.0-only
//! The `completions <shell>` generator: one completion script per shell,
//! built from the CLI's own vocabulary. The verb and subverb words come
//! from [`Verb::name()`], so a new verb cannot ship without appearing in
//! every script; the per-verb flags come from [`descriptor`], an
//! exhaustive match over [`Verb`]. The flag vocabulary is spelled twice
//! (the parse arms and the descriptor), the closed action set's own
//! precedent; a drift test in `completions_tests` pins the two together.
//! The scripts are generated from the vocabulary, not clap_complete: the
//! parser is hand-rolled and clap-free (the recorded deviation in cli.md).

mod shells;

use super::Verb;

/// The shells the generator answers, in the order the usage message names
/// them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Shell {
    /// The Bourne-again shell.
    Bash,
    /// The Z shell.
    Zsh,
    /// The friendly interactive shell.
    Fish,
    /// PowerShell.
    Pwsh,
}

impl Shell {
    /// The word the command line spells for this shell.
    pub(crate) fn word(self) -> &'static str {
        match self {
            Self::Bash => "bash",
            Self::Zsh => "zsh",
            Self::Fish => "fish",
            Self::Pwsh => "pwsh",
        }
    }

    /// The shell a command-line word names, when it is one the generator
    /// answers.
    pub(crate) fn from_word(word: &str) -> Option<Self> {
        shells::ALL
            .iter()
            .copied()
            .find(|shell| shell.word() == word)
    }
}

/// Every shell the generator answers.
pub(crate) fn shells() -> [Shell; 4] {
    shells::ALL
}

/// The shell words the usage message names, in vocabulary order: the
/// parser's "wants a shell" error lists exactly the shells the generator
/// answers, from the one source.
pub(crate) fn shell_words() -> String {
    shells()
        .iter()
        .map(|shell| shell.word())
        .collect::<Vec<_>>()
        .join(", ")
}

/// One flag in a verb's completion vocabulary: a value-taking flag, a
/// repeatable value-taking flag, or a bare switch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Flag {
    /// A flag that takes one value.
    Value(&'static str),
    /// A flag that takes a value and may repeat.
    Repeat(&'static str),
    /// A flag that takes no value.
    Switch(&'static str),
}

impl Flag {
    /// The flag's command-line word.
    pub(crate) fn word(self) -> &'static str {
        match self {
            Self::Value(word) | Self::Repeat(word) | Self::Switch(word) => word,
        }
    }
}

/// One verb's completion entry: the flags its parse arm reads.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Entry {
    /// The flags the verb accepts, in the order the scripts offer them.
    pub(crate) flags: &'static [Flag],
}

/// Generates one shell's completion script from the verb vocabulary.
pub(crate) fn script(shell: Shell) -> String {
    let verbs: Vec<(&'static str, Entry)> = PROBES
        .iter()
        .map(|verb| (verb.name(), descriptor(verb)))
        .collect();
    match shell {
        Shell::Bash => shells::bash(&verbs),
        Shell::Zsh => shells::zsh(&verbs),
        Shell::Fish => shells::fish(&verbs),
        Shell::Pwsh => shells::pwsh(&verbs),
    }
}

/// The completion vocabulary, one entry per verb. The match is exhaustive
/// over [`Verb`], so a new variant cannot compile without its entry: a
/// new verb cannot ship without completion vocabulary. The flags are
/// spelled a second time here (the parse arms spell them first); the
/// duplication cost is the closed action set's own, and the drift test
/// pins the two spellings together.
pub(crate) fn descriptor(verb: &Verb) -> Entry {
    let flags: &[Flag] = match verb {
        Verb::MessagesList { .. } => &[
            Flag::Value("--account"),
            Flag::Value("--folder"),
            Flag::Value("--limit"),
        ],
        Verb::MessagesRead { .. } => &[Flag::Value("--account"), Flag::Repeat("--key")],
        Verb::MessagesSearch { .. } => &[
            Flag::Value("--query"),
            Flag::Value("--account"),
            Flag::Value("--folder"),
            Flag::Value("--limit"),
            Flag::Value("--offset"),
        ],
        Verb::MessagesMarkRead { .. } => &[
            Flag::Value("--account"),
            Flag::Value("--key"),
            Flag::Switch("--unread"),
        ],
        Verb::MessagesSetFlagged { .. } => &[
            Flag::Value("--account"),
            Flag::Value("--key"),
            Flag::Switch("--clear"),
        ],
        Verb::MessagesArchive { .. } | Verb::MessagesTrash { .. } | Verb::DraftsDiscard { .. } => {
            &[Flag::Value("--account"), Flag::Value("--key")]
        }
        Verb::DraftsSave { .. } => &[
            Flag::Value("--account"),
            Flag::Value("--from"),
            Flag::Value("--to"),
            Flag::Value("--cc"),
            Flag::Value("--subject"),
            Flag::Value("--plain"),
            Flag::Value("--file"),
        ],
        Verb::RulesCreate { .. } => &[
            Flag::Value("--name"),
            Flag::Value("--condition"),
            Flag::Repeat("--action"),
        ],
        Verb::SnippetsCreate { .. } => &[
            Flag::Value("--title"),
            Flag::Value("--body"),
            Flag::Value("--body-file"),
        ],
        Verb::FoldersList { .. } | Verb::KnowledgeList { .. } => &[Flag::Value("--account")],
        Verb::RulesDelete { .. } | Verb::KnowledgeForget { .. } | Verb::SnippetsDelete { .. } => {
            &[Flag::Value("--id")]
        }
        Verb::AccountsList
        | Verb::RulesList
        | Verb::KnowledgeExtract
        | Verb::SnippetsList
        | Verb::SnippetsSearch
        | Verb::AiCold { .. }
        | Verb::AiResults { .. }
        | Verb::AiDrafts { .. }
        | Verb::AiSummary { .. }
        | Verb::AiStyle { .. }
        | Verb::AiStyleRebuild
        | Verb::AiDigest { .. }
        | Verb::AiReport { .. }
        | Verb::AiDigestGenerate
        | Verb::AiReportGenerate
        | Verb::Completions { .. } => &[],
    };
    Entry { flags }
}

/// One probe verb per variant, the least arguments its parse arm accepts:
/// the vocabulary table walks these so every script names every verb
/// through [`Verb::name()`] itself.
const PROBES: &[Verb] = &[
    Verb::AccountsList,
    Verb::FoldersList {
        account: String::new(),
    },
    Verb::MessagesList {
        account: String::new(),
        folder: None,
        limit: 0,
    },
    Verb::MessagesRead {
        account: String::new(),
        keys: Vec::new(),
    },
    Verb::MessagesSearch {
        query: String::new(),
        account: None,
        folder: None,
        limit: 0,
        offset: 0,
    },
    Verb::MessagesMarkRead {
        account: String::new(),
        key: String::new(),
        unread: false,
    },
    Verb::MessagesSetFlagged {
        account: String::new(),
        key: String::new(),
        clear: false,
    },
    Verb::MessagesArchive {
        account: String::new(),
        key: String::new(),
    },
    Verb::MessagesTrash {
        account: String::new(),
        key: String::new(),
    },
    Verb::DraftsSave {
        args: super::DraftArgs {
            account: String::new(),
            from: String::new(),
            to: Vec::new(),
            cc: Vec::new(),
            subject: String::new(),
            body: String::new(),
            source: "arg",
        },
    },
    Verb::DraftsDiscard {
        account: String::new(),
        key: String::new(),
    },
    Verb::RulesList,
    Verb::RulesCreate {
        name: String::new(),
        condition: String::new(),
        actions: Vec::new(),
    },
    Verb::RulesDelete { id: String::new() },
    Verb::KnowledgeList {
        account: String::new(),
    },
    Verb::KnowledgeForget { id: String::new() },
    Verb::KnowledgeExtract,
    Verb::SnippetsList,
    Verb::SnippetsCreate {
        args: super::SnippetArgs {
            title: String::new(),
            body: String::new(),
            source: "arg",
        },
    },
    Verb::SnippetsDelete { id: String::new() },
    Verb::SnippetsSearch,
    Verb::AiCold {
        account: String::new(),
        keys: Vec::new(),
    },
    Verb::AiResults {
        account: String::new(),
        keys: Vec::new(),
    },
    Verb::AiDrafts {
        account: String::new(),
        keys: Vec::new(),
    },
    Verb::AiSummary {
        account: String::new(),
        keys: Vec::new(),
    },
    Verb::AiStyle {
        account: String::new(),
    },
    Verb::AiStyleRebuild,
    Verb::AiDigest {
        account: String::new(),
    },
    Verb::AiReport {
        account: String::new(),
    },
    Verb::AiDigestGenerate,
    Verb::AiReportGenerate,
    Verb::Completions { shell: Shell::Bash },
];
