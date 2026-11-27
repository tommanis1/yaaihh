//! Docket v2 (docs/specs/docket-v2-spec.md): a text is read (`syntax`), checked whole when it is loaded
//! (`check`), and run by a machine whose normative steps an eFLINT evaluator takes behind `run::norms::Norms`.

pub mod check;
pub mod norm;
pub mod run;
pub mod syntax;
