// SPDX-License-Identifier: GPL-3.0-only
//! The command fixtures' seed records: the assist, documents, and brief
//! pulls' answers and their ask logs, split from the port module so that
//! file stays under the line cap beside the trait impl that serves them.

use astra_app::{
    BriefDocumentError, DigestView, DocumentError, DocumentView, ExtractionSummary, KnowledgeView,
    MeetingBriefView, Snippet, SplitEntry,
};
use engine_api::{AccountId, CalendarDate};

/// The assist pulls' seeds and their record: every field the A3 command
/// tests seed or assert, in one place.
#[derive(Default)]
pub(crate) struct AssistSeeds {
    /// The knowledge listing's answer, the rows the fake holds.
    pub(crate) knowledge: Vec<KnowledgeView>,
    /// The accounts the knowledge listing was asked about.
    pub(crate) knowledge_into: Vec<String>,
    /// The extraction pull's answer.
    pub(crate) extraction: ExtractionSummary,
    /// The (account, limit) pairs the extraction pull was asked for.
    pub(crate) extracted_into: Vec<(String, usize)>,
    /// The ids the forget was asked to remove.
    pub(crate) forgotten_into: Vec<String>,
    /// The snippet listing's and the search's answer.
    pub(crate) snippets: Vec<Snippet>,
    /// The search texts the search pull was asked for.
    pub(crate) searched_into: Vec<String>,
    /// The autocomplete pull's answer.
    pub(crate) suggestion: String,
    /// The (before, after) pairs the autocomplete pull was asked for.
    pub(crate) suggested_into: Vec<(String, String)>,
    /// The translate action's answer.
    pub(crate) translation: String,
    /// The (account, key, lang) triples the translate action was asked
    /// for, the lang as its wire word.
    pub(crate) translated_into: Vec<(String, String, String)>,
}

/// The A4 documents pulls' seeds and their record: every field the
/// documents command tests seed or assert, in one place.
pub(crate) struct DocsSeeds {
    /// The generate pulls' answer, an `Err` standing in for the typed
    /// refusal a test maps onto the command error.
    pub(crate) generated: Result<DigestView, DocumentError>,
    /// The (account, kind word, days) triples the generate pulls were
    /// asked for, the days as the boundary clamped them.
    pub(crate) generated_into: Vec<(String, String, u32)>,
    /// The latest read's answer.
    pub(crate) latest: Option<DocumentView>,
    /// The (account, kind word) pairs the latest read was asked for.
    pub(crate) latest_into: Vec<(String, String)>,
    /// The split pull's answer.
    pub(crate) split: Vec<SplitEntry>,
    /// The accounts the split pull was asked about.
    pub(crate) split_into: Vec<String>,
}

impl Default for DocsSeeds {
    fn default() -> Self {
        Self {
            generated: Ok(DigestView {
                period_start: 0,
                period_end: 0,
                content: String::new(),
                regenerated: false,
            }),
            generated_into: Vec::new(),
            latest: None,
            latest_into: Vec::new(),
            split: Vec::new(),
            split_into: Vec::new(),
        }
    }
}

/// The calendar brief pull's seeds and their record: the answer (an
/// `Err` standing in for the typed refusal a test maps onto the command
/// error) and the (account, day) pairs the pull was asked for, the day
/// as its wire word.
pub(crate) struct BriefSeeds {
    /// The brief pull's answer.
    pub(crate) brief: Result<MeetingBriefView, BriefDocumentError>,
    /// The (account, day word) pairs the brief pull was asked for.
    pub(crate) brief_into: Vec<(String, String)>,
}

impl Default for BriefSeeds {
    fn default() -> Self {
        Self {
            brief: Ok(MeetingBriefView {
                account: AccountId::try_from("work").expect("an account id"),
                day: CalendarDate::new(1970, 1, 1).expect("the epoch day"),
                state: astra_app::MeetingBriefState::NoMeetings,
                brief: String::new(),
                meetings: Vec::new(),
            }),
            brief_into: Vec::new(),
        }
    }
}
