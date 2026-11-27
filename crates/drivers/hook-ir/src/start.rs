//! The switches set for one agent and what a harness's start gives for them, shared by every driver.

/// What is set for one agent of a harness: the check as the program decided it, the rest its three levels.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Switches {
    /// The harness's own safety check kept on.
    pub safety_check: bool,
    /// Commands run without asking first, as the program decided: Claude Code's bypass, which its check, kept on,
    /// still asks under; the others take none.
    pub unasked: bool,
    pub tools_off: Vec<String>,
    /// Parts of its own system prompt left out, each by its exact text.
    pub parts_off: Vec<String>,
}

impl Default for Switches {
    fn default() -> Self {
        Switches { safety_check: true, unasked: false, tools_off: vec![], parts_off: vec![] }
    }
}

/// How its system prompt file is given: after the harness's own, or in place of its main text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Prompt {
    Append,
    Replace,
}

/// The arguments and environment a start gives the harness for the switches.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Launch {
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    /// Files to write in the harness's own home before it starts, each by its name there and its text.
    pub files: Vec<(String, String)>,
    /// Each tool asked off that the harness keeps on, in words: such a start is not made.
    pub tools_on: Vec<String>,
    /// Each other switch the harness has no way to take, in words, for whoever spawns it to say.
    pub not_taken: Vec<String>,
}
