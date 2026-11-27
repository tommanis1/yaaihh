//! The frame each harness driver shares (spec 10, 11, 12; std.hooks): a harness's hooks reach `Next-Moment` and get
//! `Apply`'s answer through one relay, and what a harness does natively is the one trait `Native`.
//!
//! A hook of a harness runs `docket-hook-relay NATIVE-NAME` (src/bin), which copies the hook's input to the agent's
//! socket and the answer back, and decides nothing. The socket is the agent's own, made by the driver when the
//! session starts, so the driver vouches for who reached the moment (8.8): the relay says nothing about it.

pub mod action;
pub mod bridge;
pub mod frame;
mod out;
pub mod patch;
pub mod start;
pub mod wire;

pub use action::Action;
pub use frame::{Frame, Native, Out};
