// SPDX-License-Identifier: GPL-3.0-only
//! Pure view-models shared by every Astra Mail surface.
//!
//! Immutable snapshots a host renders, projected from the engine's domain types. They
//! carry no runtime, no engine handle, and no host dependency: state lives in the
//! engine, grouping and ordering live here, so every surface shares one definition of
//! what is on screen.

mod calendar;
mod compose_open;
mod compose_request;
mod drafts;
mod folders;
mod people;
mod reading;
mod rows;
mod search;
mod sending;
mod settings;
mod sync_progress;
pub mod view;

pub use calendar::{
    CalendarRow, CalendarSnapshot, CalendarWriteRow, CalendarWriteState, NowLineData,
    OccurrenceAccuracy, OccurrenceRow, Participation, ReplyPromptRow,
};
pub use compose_open::MailtoOpen;
pub use compose_request::{ComposeKind, ComposeRequestSnapshot};
pub use drafts::{DraftSaveState, DraftStatusSnapshot};
pub use folders::{folder_role, sorted_folder_rows, AccountFolderRow, FolderRole, FolderRow};
pub use people::{
    CreateTargetRow, EditableCardRow, Monogram, PeopleSnapshot, PeopleWriteRow, PeopleWriteState,
    PersonDetail, PersonRow, PersonSourceRow, PersonValueKind, PersonValueRow,
};
pub use reading::{
    AttachmentRow, InvitationBlock, InvitationKind, MessageKey, NotAMessageRow, ReadingSnapshot,
};
pub use rows::{MessageRowView, RowKey, ViewMode};
pub use search::{SearchScope, SearchState};
pub use sending::{queued_row, QueuedKey, QueuedRow, QueuedState, SendingSnapshot};
pub use settings::{
    AboutBlock, AccountKind, AccountLegs, AccountSettingsRow, DavSectionRow, DefaultCalendar,
    GrantState, SettingsBlock, SettingsError, SettingsSnapshot, ThemeMode, WeekdayWire,
};
pub use sync_progress::{AccountSyncProgress, AccountSyncState, SyncProgressSnapshot};
pub use view::MailboxListSnapshot;
