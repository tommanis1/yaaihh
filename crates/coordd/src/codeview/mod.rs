//! The code view: one openvscode-server per coordd on 127.0.0.1, started at the first ask (the window's, once it shows
//! the project: `CodeTarget::Workspace`), with one workspace of the project's Files roots (its repositories first),
//! and the opens it is asked for; stopped when no window has been on it for `server::IDLE`.

mod install;
mod server;
mod settings;
mod target;
mod workbench;

use coord_types::codeview::{CodeLooks, CodeTarget, CodeViewAnswer};
use std::path::PathBuf;

/// What an open needs from coordd: its folder, HOME, agent-base and the project's Files roots that are folders here,
/// by name (`files::roots`).
pub struct Ctx {
    pub coord: PathBuf,
    pub home: String,
    pub agent_base: PathBuf,
    pub roots: Vec<(String, String)>,
}

impl Ctx {
    fn dir(&self) -> PathBuf {
        self.coord.join("codeview")
    }
}

/// Open TARGET: the workbench made ready in THEME and LOOKS, the server found or started, the open queued for its
/// window, the window's URL answered.
pub fn open(ctx: &Ctx, target: &CodeTarget, theme: Option<&str>, looks: Option<&CodeLooks>) -> Result<CodeViewAnswer, String> {
    if let Some(t) = theme.filter(|t| !["dark", "light"].contains(t)) {
        return Err(format!("coord codeview: theme {t} is neither dark nor light"));
    }
    let open = target::resolve(&ctx.roots, std::path::Path::new(&ctx.home), target)?;
    let w = workbench::prepare(ctx, theme, looks)?;
    let s = server::ensure(ctx)?;
    if let Some(o) = &open {
        workbench::queue(&ctx.dir(), o)?;
    }
    let origin = format!("http://127.0.0.1:{}", s.port);
    let url = format!("{origin}/?workspace={}&tkn={}", workbench::query(&w.display().to_string()), s.token);
    Ok(CodeViewAnswer { url, origin, folder: w.display().to_string() })
}

/// The window has left the code view of folder COORD: its server stopped as soon as no window is connected to it,
/// looked at for up to [`server::LEFT`]; one still connected after that is left to `watch`.
pub fn left(coord: PathBuf) {
    std::thread::spawn(move || {
        let dir = coord.join("codeview");
        let until = std::time::Instant::now() + server::LEFT;
        while !server::end_if_alone(&dir) && std::time::Instant::now() < until {
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
    });
}

/// coordd ending: the server of folder COORD stopped, whoever is on it.
pub fn end(coord: &std::path::Path) {
    server::end(&coord.join("codeview"));
}

/// coordd's watch on the server of its folder COORD: looked at each minute, let go once idle (`server::let_go`).
pub fn watch(coord: PathBuf) {
    let dir = coord.join("codeview");
    let mut quiet = None;
    loop {
        std::thread::sleep(std::time::Duration::from_secs(60));
        server::let_go(&dir, &mut quiet, std::time::Instant::now());
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
