//! The policy language's surface: its parser lives in docket-machine (`docket_machine::policy_syntax`), pure, so the
//! language server reads policy files with the same parser coordd loads them with (N-101 p646).

pub use docket_machine::policy_syntax::*;
