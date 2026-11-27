//! The engine's frame: one socket, six requests and a terminal's bytes, the record's file, and the machine behind them.

// The frame knows no act, fact or effect by name. It fixes who a connection acts as ([`peer`]), keeps the record's
// lines in order on disk ([`file`]), carries requests to the machine ([`docket`]) and its new lines to every watcher
// ([`serve`]), and waits on the linked drivers ([`link`]). What a line means is docket-v2's; what an act is, a library's.

// A driver is a crate linked into the daemon: it implements docket-v2's `Driver` and the frame's `Waits`. The socket
// brings parties' acts, queries, watches and terminal bytes, and answers a helper process gives for a driver.

pub mod act;
pub mod ask;
pub mod codec;
pub mod delta;
pub mod docket;
pub mod file;
pub mod hand;
pub mod head;
pub mod hooks;
pub mod link;
pub mod machine;
pub mod peer;
pub mod serve;
pub mod start;
pub mod sync;
pub mod texts;
pub mod turns;
pub mod why;
