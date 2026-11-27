//! An agent's permissions as one choice, Safe, Guarded or Yolo, kept by coordd as two settings: `agent_mode` (safe or
//! yolo) and `safety_check` (Claude Code's own check on dangerous commands).

/// The three choices, in the order every control shows them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Permission {
    Safe,
    Guarded,
    Yolo,
}

pub const ALL: [Permission; 3] = [Permission::Safe, Permission::Guarded, Permission::Yolo];

/// The settings key the Settings page gives the one row.
pub const KEY: &str = "permissions";

/// What the choices mean, the row's hover help.
pub const HELP: &str = "Safe: asks before it runs a command.\nGuarded: runs commands, keeping Claude Code's check on dangerous ones.\nYolo: runs everything with no prompts, for agents in a box.";

impl Permission {
    /// The choice that mode YOLO with the check CHECK gives; under Safe the check changes nothing.
    pub fn of(yolo: bool, check: bool) -> Self {
        match (yolo, check) {
            (false, _) => Permission::Safe,
            (true, true) => Permission::Guarded,
            (true, false) => Permission::Yolo,
        }
    }

    /// The choice the words MODE and CHECK of coordd's settings give.
    pub fn of_words(mode: &str, check: &str) -> Self {
        Self::of(mode == "yolo", check != "false")
    }

    /// Its value for `agent_mode`.
    pub fn mode(self) -> &'static str {
        if self == Permission::Safe { "safe" } else { "yolo" }
    }

    /// Its value for `safety_check`.
    pub fn check(self) -> bool {
        self != Permission::Yolo
    }

    pub fn word(self) -> &'static str {
        match self {
            Permission::Safe => "Safe",
            Permission::Guarded => "Guarded",
            Permission::Yolo => "Yolo",
        }
    }

    /// Its value as the Settings page's one row writes it.
    pub fn wire(self) -> &'static str {
        match self {
            Permission::Safe => "safe",
            Permission::Guarded => "guarded",
            Permission::Yolo => "yolo",
        }
    }

    pub fn from_wire(w: &str) -> Option<Self> {
        ALL.into_iter().find(|p| p.wire() == w)
    }

    /// Whether a harness that sets the mode only if TAKES_MODE and the check only if TAKES_CHECK can be put in this
    /// choice while it runs in mode YOLO with the check CHECK.
    pub fn reachable(self, takes_mode: bool, takes_check: bool, yolo: bool, check: bool) -> bool {
        (takes_mode || (self != Permission::Safe) == yolo) && (takes_check || self == Permission::Safe || self.check() == check)
    }
}

/// Whether KIND's harness has a check of its own for Guarded to keep (coord-types' harness table): Claude Code's. agy
/// has none, so its Yolo runs everything and reads Yolo whatever the check setting says (N-1956).
pub fn has_check(kind: &str) -> bool {
    coord_types::harness::HarnessCapabilities::of(kind, false).is_some_and(|c| c.safety_check_off.taken())
}

/// Agent NAME of project PI put in PICK from NOW (its mode yolo, its check): the check switched first where its
/// harness TAKES_CHECK, so a restart for the mode starts with it, then the mode.
pub(crate) fn send(s: &mut crate::App, pi: usize, name: &str, pick: Permission, now: (bool, bool), takes_check: bool) {
    use coord_types::Request;
    // the session panel's kind for each change it sends, so coordd's word shows under its rows
    let pending = || crate::Pending::Effort { agent: name.to_string() };
    s.streams.said.remove(&(pi, name.to_string()));
    if takes_check && pick.check() != now.1 {
        s.request_in(pi, Request::HarnessSwitches { name: name.into(), safety_check: Some(pick.check()), tools_off: None, parts_off: None }, pending());
    }
    if (pick != Permission::Safe) != now.0 {
        s.request_in(pi, Request::Mode { name: name.into(), mode: pick.mode().into() }, pending());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_choice_maps_onto_the_two_settings_and_back() {
        let want = [(Permission::Safe, "safe", true), (Permission::Guarded, "yolo", true), (Permission::Yolo, "yolo", false)];
        for (p, mode, check) in want {
            assert_eq!((p.mode(), p.check()), (mode, check), "{p:?}");
            assert_eq!(Permission::of_words(mode, &check.to_string()), p);
            assert_eq!(Permission::from_wire(p.wire()), Some(p));
        }
        assert_eq!(Permission::of_words("safe", "false"), Permission::Safe, "Safe asks whatever the check");
        assert!(Permission::from_wire("").is_none());
        // a harness with no switch for the check keeps the one it has: Safe, or the yolo choice that check gives
        let can: Vec<_> = ALL.into_iter().filter(|p| p.reachable(true, false, false, true)).collect();
        assert_eq!(can, [Permission::Safe, Permission::Guarded]);
        let can: Vec<_> = ALL.into_iter().filter(|p| p.reachable(false, true, true, true)).collect();
        assert_eq!(can, [Permission::Guarded, Permission::Yolo], "no mode switch: stays yolo");
    }

    /// The Session panel of a Claude Code agent whose harness switches its check: one Permissions row of three, no
    /// safety switch beside it, the header naming Guarded; Yolo switches the check off alone, Safe from Yolo switches
    /// it on and asks for the mode.
    #[test]
    #[cfg(not(windows))]
    fn the_session_panel_offers_the_three_and_a_pick_sends_the_check_then_the_mode() {
        use crate::home::tests::{settle, texts};
        let (_held, mut s) = crate::home::tests::made();
        let (child, file) = crate::map::deep::tests::piped(&mut s, 0, "permission-pick");
        let a = &mut s.projects[0].live.agents[0];
        (a.mode, a.capabilities.safety_toggle, a.switches.safety_check) = (Some("yolo".into()), true, true);
        assert_eq!(crate::settings::button_words(a), "Session · Guarded");
        s.sel = crate::Sel::Agent("try-1".into());
        s.streams.open.insert("session|0|try-1".into());
        let mut rt = crate::view_cost::window(s);
        settle(&mut rt);
        let t = texts(&rt);
        for w in ["Permissions", "Safe", "Guarded", "Yolo"] {
            assert!(t.iter().any(|x| x == w), "{w}: {t:?}");
        }
        assert!(!t.iter().any(|x| x.contains("Safety check") || x.contains("check on dangerous commands")), "no second control: {t:?}");
        rt.update(|s, _| send(s, 0, "try-1", Permission::Yolo, (true, true), true));
        rt.update(|s, _| send(s, 0, "try-1", Permission::Safe, (true, false), true));
        rt.update(|s, _| send(s, 0, "try-1", Permission::Guarded, (false, true), false));
        rt.state.projects[0].link = None;
        let sent: Vec<serde_json::Value> = crate::map::deep::tests::sent(child, &file).into_iter().map(|v| v["req"].clone()).filter(|r| r["op"] == "harnessswitches" || r["op"] == "mode").collect();
        assert_eq!(sent, [
            serde_json::json!({"op": "harnessswitches", "name": "try-1", "safety_check": false}),
            serde_json::json!({"op": "harnessswitches", "name": "try-1", "safety_check": true}),
            serde_json::json!({"op": "mode", "name": "try-1", "mode": "safe"}),
            serde_json::json!({"op": "mode", "name": "try-1", "mode": "yolo"}),
        ]);
    }
}
