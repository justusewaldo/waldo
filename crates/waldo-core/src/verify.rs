//! An act is not done until its result is seen.
//!
//! [`ladder`] tries the ways to cause an effect one by one, checks the
//! result after each, and stops at the first one that worked. A way that
//! returns Ok but changes nothing does not count. It knows nothing about
//! macOS, so the tests below drive it with fakes.

use crate::apps::Seen;
use std::thread::sleep;
use std::time::{Duration, Instant};

#[derive(Debug, PartialEq)]
pub enum Outcome {
    /// The check passed. `by` names the way that did it.
    Done { by: &'static str, ms: u64 },
    /// Every way was tried and the check kept saying no.
    NoEffect { tried: Vec<&'static str> },
    /// The check could not see the state, so success is not known.
    Unobservable { tried: Vec<&'static str>, why: String },
}

/// One way to cause the effect: its name, how long to wait for the check
/// after it runs, and the action itself.
pub type Way<'a> = (&'static str, Duration, Box<dyn FnMut() -> Result<(), String> + 'a>);

/// Tries each way in turn. After a way runs, polls `check` for up to that
/// way's wait. Already true before any way runs counts as done, by "already".
pub fn ladder(ways: Vec<Way<'_>>, mut check: impl FnMut() -> Seen<()>) -> Outcome {
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;
    let mut saw_no = false;
    let mut why = String::new();
    match check() {
        Seen::Yes(()) => return Outcome::Done { by: "already", ms: 0 },
        Seen::No => saw_no = true,
        Seen::Unknown(w) => why = w,
    }
    let mut tried = Vec::new();
    for (name, wait, mut way) in ways {
        tried.push(name);
        if way().is_err() {
            continue;
        }
        let deadline = Instant::now() + wait;
        loop {
            match check() {
                Seen::Yes(()) => return Outcome::Done { by: name, ms: ms(start) },
                Seen::No => saw_no = true,
                Seen::Unknown(w) => why = w,
            }
            if Instant::now() >= deadline {
                break;
            }
            sleep(Duration::from_millis(30));
        }
    }
    if saw_no {
        Outcome::NoEffect { tried }
    } else {
        Outcome::Unobservable { tried, why }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    const W: Duration = Duration::from_millis(1);

    #[test]
    fn a_way_that_says_ok_but_does_nothing_is_not_done() {
        let state = Cell::new(false);
        let ways: Vec<Way> = vec![
            ("ignored", W, Box::new(|| Ok(()))),
            ("works", W, Box::new(|| { state.set(true); Ok(()) })),
        ];
        let out = ladder(ways, || if state.get() { Seen::Yes(()) } else { Seen::No });
        assert!(matches!(out, Outcome::Done { by: "works", .. }));
    }

    #[test]
    fn every_way_failing_is_no_effect_with_the_list() {
        let ways: Vec<Way> = vec![("a", W, Box::new(|| Ok(()))), ("b", W, Box::new(|| Err("refused".into())))];
        assert_eq!(ladder(ways, || Seen::No), Outcome::NoEffect { tried: vec!["a", "b"] });
    }

    #[test]
    fn unknown_is_never_reported_as_no() {
        let ways: Vec<Way> = vec![("a", W, Box::new(|| Ok(())))];
        let out = ladder(ways, || Seen::Unknown("blind".into()));
        assert_eq!(out, Outcome::Unobservable { tried: vec!["a"], why: "blind".into() });
    }

    #[test]
    fn already_true_needs_no_way() {
        let called = Cell::new(false);
        let ways: Vec<Way> = vec![("a", W, Box::new(|| { called.set(true); Ok(()) }))];
        assert_eq!(ladder(ways, || Seen::Yes(())), Outcome::Done { by: "already", ms: 0 });
        assert!(!called.get());
    }
}
