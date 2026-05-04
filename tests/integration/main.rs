//! Integration tests consolidated into a single binary.
//!
//! Each submodule corresponds to a command or behavior under test.
//! Helpers live in `helpers.rs` and are shared by every module.

mod helpers;

mod amend;
mod cleanup;
mod commit;
mod data_preservation;
mod diff_show;
mod drop;
mod log;
mod rebase;
mod revert;
mod reword;
mod seqedit;
mod split;
mod squash;
mod stage;
mod stash;
mod status;
