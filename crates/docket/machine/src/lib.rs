//! Docket's machine: tickets, kinds and patterns, as a pure function of state and input (owner m613, change 4).
//! No IO, no clock, no threads, no lock: coordd's `ticket` module reads the files, owns the log, and carries out the
//! effects.

pub mod answers;
pub mod asks;
pub mod check;
pub mod comment;
pub mod given;
pub mod graph;
pub mod graph_git;
mod graph_kept;
pub mod graph_slice;
mod graph_runs;
mod graph_posts;
pub mod insight;
pub mod doc;
pub mod label;
pub mod line;
pub mod machine;
pub mod policy_syntax;
pub mod print;
pub mod runner;
pub mod syntax;
pub mod template;
pub mod types;
pub mod view;

pub use check::CallSig;
pub use comment::{Anchor, Comment};
pub use line::{At, Closing, Delivery, Effect, Fired, Input, Line, NotRead, Post, PostKind, Question, Raised, SplitFrom, What, Worked};
pub use machine::{ALL, AllState, BRAIN, BRAIN_SWITCH, CloseChange, DELIVERED, DESK, DUE_AGAIN, Defs, ERA, ERA_CLAIMS, ERA_CLOSES, ERA_HIS_HANDED, ERA_NAMES, ERA_QUIET, ERA_PUT_IN, all_of, HELD_HANDED, TAKEN_BACK, TOLD, UNTOLD, HIDDEN, Halt, Halting, MAIL_HOLD, MailHold, Machine, Commitment, NOBODY, OWNER_POST, Out, OwedGive, RemindWhy, SCOPES, SHAPE, SIGNED_OFF, TURN_BEGAN, Shaped, Status, Ticket, WATCHER, Running, WINDOW_RELEASE, WORTHS, IN_WINDOW, MERGED, ASKED, DUTIES, Seen, WindowRelease, at_name, at_secs, coordds_own, delivered_post, parse_delivery, parse_ticket, post_id, post_or_ticket, post_ref, step_id, told_post};
pub use types::{Loc, Part, PostRef, Span, Text, Type};
pub use syntax::{Binding, Drawn, Every};
pub use view::{ACT_IDS, ClockView, LiveEdit, Reach, ReachCase, Reloaded, ViewActed, RunNode, RunRound, RunTook, TicketView, ViewAsked, ViewEnded, ViewHalt, ViewMention, ViewOutline, ViewOwner, ViewQuiet, ViewRan, ViewRefused, ViewLapsed, ViewShaped, ViewStar, ViewStatus, ViewUnsent, WaitsOn, Withdrew};
pub use view::{PostWhere, Waiting, Where, WhereAnswer, WhereStep};

#[cfg(test)]
mod tests;
