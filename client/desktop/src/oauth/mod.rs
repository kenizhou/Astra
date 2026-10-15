// SPDX-License-Identifier: GPL-3.0-only
//! The host's OAuth halves: the system-browser sign-in session, the
//! credential transaction (the design's D2 and D3), and the wizard's
//! sign-in command surface over them. The pure client lives in
//! `astra-oauth`; everything here is the platform half that crate
//! deliberately does not own: opening the browser, capturing the
//! deep-link redirect, and moving the resulting credential through the
//! OS secure store.
//!
//! # Where the transaction lives, and why
//!
//! The reference put the credential transaction in the core, under its
//! "the core owns when, the host owns where" split. Astra's split already
//! answers differently, and this module records that honestly rather than
//! porting the shape: **the transaction is host-side**, wrapping the
//! [`AccountCredentialStore`] port, because
//!
//! - the port itself is host-side: the store over Windows Credential
//!   Manager, the keychain, and the secret service is the desktop
//!   crate's own, and `astra-app` sees it only through the
//!   account-operations seam, which exposes no generic persist;
//! - the app core carries no OAuth type at all: no token, no provider
//!   config, and no dependency on `astra-oauth`, so an app-side
//!   transaction would first have to widen the core's surface for a
//!   consumer that does not exist until the provider slices (SP2m,
//!   SP2n) wire sign-in;
//! - the account-add flow already keeps its persist discipline here:
//!   `add_account` persists after the dial, rolls the whole add back on
//!   a refusal, and the settings seam answers a failed removal with the
//!   error. The OAuth transaction is the same discipline named once,
//!   beside the flow that will call it.
//!
//! The rules bind wherever the transaction lives: the add rolls back on
//! failure, the rotation logs at error what the next launch faces, the
//! removal answers the error rather than reverting. They are pinned by
//! the transaction's tests against a scripted store.
//!
//! # The callback scheme
//!
//! The callback arrives on the app's own custom scheme, the identifier
//! `tauri.conf.json` already carries ([`OAUTH_SCHEME`],
//! `com.kenizhou.astra`), registered beside `mailto` in the deep-link
//! block. The redirect URI the provider configs register is
//! [`OAUTH_REDIRECT_URI`] (`com.kenizhou.astra://oauth`). The OS delivers
//! only registered schemes, and the capture filters by the same scheme
//! again, so a `mailto` delivery never reaches a sign-in flow.
//!
//! # The seams, and what is wired when
//!
//! The capture's production stream is `tauri-plugin-deep-link`'s
//! `on_open_url` event, whose payload is a batch of parsed URLs;
//! [`oauth_callback`] is the filter that keeps the OAuth-scheme URL and
//! hands it on **whole**, never parsed by this host (the `state` CSRF
//! check stands in `astra-oauth`'s `parse_callback`). The runtime wiring
//! lives in the sign-in commands module and the app setup:
//! [`forward_oauth_callbacks`] is registered in the boot hook, the
//! single-instance callback's argv is handed to the deep-link plugin on
//! Windows (without which a running app on this platform never sees the
//! redirect), and the opener-backed [`SystemBrowser`] is the browser the
//! sign-in commands install. The host refresh path holds the credential
//! transaction: the account half (`graph_add`) persists grants through
//! its add and rotation paths, and the dial-table token sink answers
//! every rotation a Graph token source reports.
//!
//! [`AccountCredentialStore`]: crate::credential_store::AccountCredentialStore
//! [`oauth_callback`]: session::oauth_callback
//! [`forward_oauth_callbacks`]: session::forward_oauth_callbacks
//! [`SystemBrowser`]: session::SystemBrowser

pub(crate) mod commands;
/// The completion's account half: what a Microsoft sign-in's token set
/// becomes, and the sink every Graph token source reports rotations
/// through.
pub(crate) mod graph_add;
/// The dial-table token sink: the host half that lands rotations and moved
/// scope sets in the dial table's entry and the secure store.
pub(crate) mod graph_sink;
pub(crate) mod session;
pub(crate) mod transaction;
