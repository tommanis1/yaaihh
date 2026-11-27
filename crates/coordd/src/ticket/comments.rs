//! His comments on passages as data (`Request::Msg`'s `comments`, `Request::CommentResolve`): given to the post his
//! message made, each with an id of its own, and resolved or reopened later.

use super::ticket_of;
use crate::core::{err, now, ok, Core, Who};
use coord_types::Reply;
use docket_machine::{Comment, Input};

impl Core {
    /// COMMENTS given to the post the request just made (`Tickets::posted`), by its writer, numbered `c1`, `c2`… after
    /// those it has.
    pub fn ticket_comments(&mut self, comments: Vec<Comment>) -> Result<String, String> {
        let (id, n) = self.tickets.posted.clone().ok_or("its comments: no post was made")?;
        let t = ticket_of(&self.tickets.m, &id)?;
        let p = self.tickets.m.tickets.get(&t).and_then(|x| x.posts.iter().find(|p| p.n == n)).ok_or_else(|| format!("its comments: {id}.p{n} is not there"))?;
        let (by, had, at) = (p.by.clone(), p.comments.len(), now());
        let comments = comments.into_iter().enumerate().map(|(k, c)| Comment { id: format!("c{}", had + k + 1), author: by.clone(), at: at.clone(), ..c }).collect();
        self.ticket_input(Input::Comments { t, n, by, comments })
    }

    /// WHO resolves comment ID of post N of T (RESOLVED), or reopens it.
    pub fn ticket_comment_resolve(&mut self, who: &Who, t: &str, n: u32, id: &str, resolved: bool) -> Reply {
        let t = match ticket_of(&self.tickets.m, t) {
            Ok(t) => t,
            Err(e) => return err(e),
        };
        match self.ticket_input(Input::CommentResolved { t, n, id: id.to_string(), resolved, by: who.name() }) {
            Ok(r) => ok(r),
            Err(e) => err(e),
        }
    }
}
