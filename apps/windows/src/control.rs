//! Other programs reaching the mode of the field with the focus, which on
//! Windows lives in whichever application has it: the server listens on the
//! port, and each text service tells it, over a pipe, when its field gains
//! or loses the focus and when its mode changes. The server keeps the field
//! that took the focus last and hands it the modes other programs set.
//!
//! Each process keeps one connection to the server, which carries the
//! reports of all its threads, one line each.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use kanaemi_core::{Event, Mode};

/// How long the server waits for a field to take a mode it was given before
/// answering with the mode it knows. A field that has not taken it by then
/// never does: the program that asked already has its answer.
pub const ANSWER_WITHIN: Duration = Duration::from_millis(500);

/// The longest line either side sends; a longer one ends the connection.
pub const MAX_LINE: usize = 128;

/// How many requests from other programs wait at the server while a field
/// takes a mode; more are left with the port until there is room.
pub const MAX_WAITING: usize = 64;

/// What a text service tells the server of the field of one of its threads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Report {
    /// The field took the focus at `at`, a time every process reads alike.
    Focus { thread: u32, at: u64, mode: Mode },
    /// The field with the focus is in another mode.
    Mode { thread: u32, mode: Mode },
    /// The field lost the focus.
    Blur { thread: u32 },
    /// The thread is done with the server's request `request`, whether or
    /// not its field took the mode.
    Done { thread: u32, request: u64 },
}

/// What the server asks of a text service.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    SetMode {
        thread: u32,
        request: u64,
        mode: Mode,
    },
}

impl Report {
    pub fn encode(self) -> String {
        match self {
            Self::Focus { thread, at, mode } => format!("focus {thread} {at} {}", name(mode)),
            Self::Mode { thread, mode } => format!("mode {thread} {}", name(mode)),
            Self::Blur { thread } => format!("blur {thread}"),
            Self::Done { thread, request } => format!("done {thread} {request}"),
        }
    }

    pub fn decode(line: &str) -> Option<Self> {
        match *words(line).as_slice() {
            ["focus", thread, at, mode] => Some(Self::Focus {
                thread: thread.parse().ok()?,
                at: at.parse().ok()?,
                mode: named(mode)?,
            }),
            ["mode", thread, mode] => Some(Self::Mode {
                thread: thread.parse().ok()?,
                mode: named(mode)?,
            }),
            ["blur", thread] => Some(Self::Blur {
                thread: thread.parse().ok()?,
            }),
            ["done", thread, request] => Some(Self::Done {
                thread: thread.parse().ok()?,
                request: request.parse().ok()?,
            }),
            _ => None,
        }
    }

    /// The report as the server takes it, its clock reading `now`: a field
    /// takes the focus before the server hears of it, so a later time,
    /// which would keep any field that really took it after from being
    /// followed, is taken as now.
    pub fn heard_at(self, now: u64) -> Self {
        match self {
            Self::Focus { thread, at, mode } => Self::Focus {
                thread,
                at: at.min(now),
                mode,
            },
            report => report,
        }
    }
}

impl Command {
    /// The command as sent, to be carried out only before `by`, a time every
    /// process reads alike. The time travels with it, as the text service's
    /// process may be stopped as a whole, the thread that reads the command
    /// too: it reads it only after the server answered without it.
    pub fn encode(self, by: u64) -> String {
        match self {
            Self::SetMode {
                thread,
                request,
                mode,
            } => format!("set-mode {thread} {request} {} {by}", name(mode)),
        }
    }

    /// The command and the time it is to be carried out by.
    pub fn decode(line: &str) -> Option<(Self, u64)> {
        match *words(line).as_slice() {
            ["set-mode", thread, request, mode, by] => Some((
                Self::SetMode {
                    thread: thread.parse().ok()?,
                    request: request.parse().ok()?,
                    mode: named(mode)?,
                },
                by.parse().ok()?,
            )),
            _ => None,
        }
    }
}

fn words(line: &str) -> Vec<&str> {
    line.split(' ').collect()
}

fn name(mode: Mode) -> &'static str {
    match mode {
        Mode::Kana => "kana",
        Mode::Abc => "abc",
    }
}

fn named(name: &str) -> Option<Mode> {
    match name {
        "kana" => Some(Mode::Kana),
        "abc" => Some(Mode::Abc),
        _ => None,
    }
}

/// Splits what comes in on a connection into lines.
#[derive(Default)]
pub struct Lines {
    pending: Vec<u8>,
}

impl Lines {
    /// The lines `bytes` ends, or `None` once a line runs past
    /// [`MAX_LINE`] or is not UTF-8, which ends the connection.
    pub fn push(&mut self, bytes: &[u8]) -> Option<Vec<String>> {
        self.pending.extend_from_slice(bytes);
        let mut lines = Vec::new();
        while let Some(end) = self.pending.iter().position(|&byte| byte == b'\n') {
            let line: Vec<u8> = self.pending.drain(..=end).take(end).collect();
            if line.len() > MAX_LINE {
                return None;
            }
            lines.push(String::from_utf8(line).ok()?);
        }
        (self.pending.len() <= MAX_LINE).then_some(lines)
    }
}

/// Follows the field of one thread, to tell the server what changed.
#[derive(Default)]
pub struct Tracker {
    /// The field's mode while it has the focus.
    focused: Option<Mode>,
}

impl Tracker {
    pub fn focused(&self) -> bool {
        self.focused.is_some()
    }

    /// What to tell the server once the field of `thread` handled `event`
    /// and is in `mode`, `at` being the time now.
    pub fn follow(&mut self, thread: u32, event: Event, mode: Mode, at: u64) -> Option<Report> {
        match event {
            Event::FocusIn { .. } => {
                self.focused = Some(mode);
                Some(Report::Focus { thread, at, mode })
            }
            Event::FocusOut => self.focused.take().map(|_| Report::Blur { thread }),
            _ => match self.focused {
                Some(before) if before != mode => {
                    self.focused = Some(mode);
                    Some(Report::Mode { thread, mode })
                }
                _ => None,
            },
        }
    }
}

/// What the server does next, as the [`Desk`] decides.
#[derive(Debug, PartialEq, Eq)]
pub enum Step<R> {
    /// Sends the command over the connection `link`.
    Send(u64, Command),
    /// Answers the request with the mode of the field with the focus.
    Answer(R, Option<Mode>),
    /// Tells watching programs the mode of the field with the focus.
    Tell(Option<Mode>),
}

/// The server's record of the field with the focus, and the requests of
/// other programs on their way to it, answered in the order they came.
pub struct Desk<R> {
    focus: Option<Focused>,
    /// When the field the focus went to last took it. Connections report
    /// independently, so a field that took the focus before may say so
    /// after; it is not taken for the one with the focus.
    latest: Option<u64>,
    waiting: VecDeque<(R, Option<Mode>)>,
    forwarded: Option<Forwarded<R>>,
    next: u64,
}

#[derive(Clone, Copy)]
struct Focused {
    link: u64,
    thread: u32,
    mode: Mode,
}

struct Forwarded<R> {
    request: R,
    link: u64,
    thread: u32,
    number: u64,
    by: Instant,
}

impl<R> Default for Desk<R> {
    fn default() -> Self {
        Self {
            focus: None,
            latest: None,
            waiting: VecDeque::new(),
            forwarded: None,
            next: 0,
        }
    }
}

impl<R> Desk<R> {
    /// Whether more requests may wait.
    pub fn has_room(&self) -> bool {
        self.waiting.len() < MAX_WAITING
    }

    /// When the request handed to a field is answered whatever it does.
    pub fn deadline(&self) -> Option<Instant> {
        self.forwarded.as_ref().map(|forwarded| forwarded.by)
    }

    /// Takes a request from another program, with the mode it sets if any.
    pub fn request(&mut self, request: R, to_set: Option<Mode>, now: Instant) -> Vec<Step<R>> {
        self.waiting.push_back((request, to_set));
        let mut steps = Vec::new();
        self.serve(now, &mut steps);
        steps
    }

    /// Takes what the text services on the connection `link` reported.
    pub fn report(&mut self, link: u64, report: Report, now: Instant) -> Vec<Step<R>> {
        let mut steps = Vec::new();
        let ours = |focus: &Option<Focused>, thread| {
            focus.is_some_and(|focus| focus.link == link && focus.thread == thread)
        };
        match report {
            Report::Focus { thread, at, mode } => {
                if self.latest.is_none_or(|latest| at >= latest) {
                    self.latest = Some(at);
                    self.focus = Some(Focused { link, thread, mode });
                }
                steps.push(self.tell());
            }
            Report::Mode { thread, mode } if ours(&self.focus, thread) => {
                self.focus = self.focus.map(|focus| Focused { mode, ..focus });
                steps.push(self.tell());
            }
            Report::Blur { thread } if ours(&self.focus, thread) => {
                self.focus = None;
                steps.push(self.tell());
            }
            Report::Done { thread, request } => {
                let answered = self.forwarded.as_ref().is_some_and(|forwarded| {
                    (forwarded.link, forwarded.thread, forwarded.number) == (link, thread, request)
                });
                if answered {
                    self.answer_forwarded(&mut steps);
                    self.serve(now, &mut steps);
                }
            }
            Report::Mode { .. } | Report::Blur { .. } => {}
        }
        steps
    }

    /// The connection `link` ended, and with it every field it reported.
    pub fn gone(&mut self, link: u64, now: Instant) -> Vec<Step<R>> {
        let mut steps = Vec::new();
        if self.focus.is_some_and(|focus| focus.link == link) {
            self.focus = None;
            steps.push(self.tell());
        }
        if self
            .forwarded
            .as_ref()
            .is_some_and(|forwarded| forwarded.link == link)
        {
            self.answer_forwarded(&mut steps);
            self.serve(now, &mut steps);
        }
        steps
    }

    /// Answers the request handed to a field once its time is up.
    pub fn tick(&mut self, now: Instant) -> Vec<Step<R>> {
        let mut steps = Vec::new();
        if self.deadline().is_some_and(|by| by <= now) {
            self.answer_forwarded(&mut steps);
            self.serve(now, &mut steps);
        }
        steps
    }

    fn mode(&self) -> Option<Mode> {
        self.focus.map(|focus| focus.mode)
    }

    fn tell(&self) -> Step<R> {
        Step::Tell(self.mode())
    }

    fn answer_forwarded(&mut self, steps: &mut Vec<Step<R>>) {
        if let Some(forwarded) = self.forwarded.take() {
            steps.push(Step::Answer(forwarded.request, self.mode()));
        }
    }

    /// Answers the requests waiting, in order, until one goes to a field.
    fn serve(&mut self, now: Instant, steps: &mut Vec<Step<R>>) {
        while self.forwarded.is_none() {
            let Some((request, to_set)) = self.waiting.pop_front() else {
                return;
            };
            match (to_set, self.focus) {
                (Some(mode), Some(focus)) => {
                    let number = self.next;
                    self.next += 1;
                    steps.push(Step::Send(
                        focus.link,
                        Command::SetMode {
                            thread: focus.thread,
                            request: number,
                            mode,
                        },
                    ));
                    self.forwarded = Some(Forwarded {
                        request,
                        link: focus.link,
                        thread: focus.thread,
                        number,
                        by: now + ANSWER_WITHIN,
                    });
                }
                _ => steps.push(Step::Answer(request, self.mode())),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FOCUS_IN: Event = Event::FocusIn { password: false };

    #[test]
    fn every_report_and_command_reads_back_as_written() {
        let reports = [
            Report::Focus {
                thread: 7,
                at: u64::MAX,
                mode: Mode::Kana,
            },
            Report::Mode {
                thread: u32::MAX,
                mode: Mode::Abc,
            },
            Report::Blur { thread: 0 },
            Report::Done {
                thread: 3,
                request: 9,
            },
        ];
        for report in reports {
            assert_eq!(Report::decode(&report.encode()), Some(report));
            assert!(report.encode().len() <= MAX_LINE);
        }
        let command = Command::SetMode {
            thread: 3,
            request: u64::MAX,
            mode: Mode::Kana,
        };
        assert_eq!(
            Command::decode(&command.encode(u64::MAX)),
            Some((command, u64::MAX))
        );
        assert!(command.encode(u64::MAX).len() <= MAX_LINE);
    }

    #[test]
    fn a_focus_is_not_taken_as_later_than_the_server_hears_of_it() {
        let ahead = Report::Focus {
            thread: 1,
            at: u64::MAX,
            mode: Mode::Abc,
        };
        assert_eq!(
            ahead.heard_at(100),
            Report::Focus {
                thread: 1,
                at: 100,
                mode: Mode::Abc
            }
        );
        let behind = Report::Focus {
            thread: 1,
            at: 50,
            mode: Mode::Abc,
        };
        assert_eq!(behind.heard_at(100), behind);
        let blur = Report::Blur { thread: 1 };
        assert_eq!(blur.heard_at(0), blur);
    }

    #[test]
    fn lines_that_are_no_report_are_not_read_as_one() {
        for line in [
            "",
            "focus 1 2",
            "focus 1 2 hiragana",
            "mode -1 abc",
            "blur 1 2",
            "done 1",
            "Focus 1 2 kana",
            "focus  1 2 kana",
            "set-mode 1 2 kana",
        ] {
            assert_eq!(Report::decode(line), None, "{line:?}");
        }
        assert_eq!(Command::decode("focus 1 2 kana"), None);
    }

    #[test]
    fn lines_split_where_they_end_however_they_arrive() {
        let mut lines = Lines::default();
        assert_eq!(lines.push(b"blur 1\nmo"), Some(vec!["blur 1".to_owned()]));
        assert_eq!(lines.push(b"de 1 abc"), Some(vec![]));
        assert_eq!(
            lines.push(b"\nblur 2\n"),
            Some(vec!["mode 1 abc".to_owned(), "blur 2".to_owned()])
        );
    }

    #[test]
    fn a_line_too_long_or_not_utf8_ends_the_connection() {
        assert_eq!(Lines::default().push(&[b'x'; MAX_LINE + 1]), None);
        let mut long = vec![b'x'; MAX_LINE + 1];
        long.push(b'\n');
        assert_eq!(Lines::default().push(&long), None);
        assert_eq!(Lines::default().push(b"\xff\n"), None);
    }

    #[test]
    fn a_field_reports_taking_and_losing_the_focus_and_each_change_between() {
        let mut tracker = Tracker::default();
        assert_eq!(
            tracker.follow(5, FOCUS_IN, Mode::Abc, 10),
            Some(Report::Focus {
                thread: 5,
                at: 10,
                mode: Mode::Abc
            })
        );
        assert_eq!(tracker.follow(5, Event::Flush, Mode::Abc, 11), None);
        assert_eq!(
            tracker.follow(5, Event::SetMode(Mode::Kana), Mode::Kana, 12),
            Some(Report::Mode {
                thread: 5,
                mode: Mode::Kana
            })
        );
        assert!(tracker.focused());
        assert_eq!(
            tracker.follow(5, Event::FocusOut, Mode::Kana, 13),
            Some(Report::Blur { thread: 5 })
        );
        assert!(!tracker.focused());
    }

    #[test]
    fn a_field_without_the_focus_reports_nothing() {
        let mut tracker = Tracker::default();
        assert_eq!(tracker.follow(5, Event::FocusOut, Mode::Abc, 1), None);
        assert_eq!(
            tracker.follow(5, Event::SetMode(Mode::Kana), Mode::Kana, 2),
            None
        );
    }

    fn focus(thread: u32, at: u64, mode: Mode) -> Report {
        Report::Focus { thread, at, mode }
    }

    #[test]
    fn requests_without_a_field_with_the_focus_are_answered_with_no_mode() {
        let mut desk = Desk::default();
        let now = Instant::now();
        assert_eq!(desk.request("get", None, now), [Step::Answer("get", None)]);
        assert_eq!(
            desk.request("set", Some(Mode::Kana), now),
            [Step::Answer("set", None)]
        );
    }

    #[test]
    fn the_mode_read_is_that_of_the_field_that_took_the_focus_last() {
        let mut desk = Desk::default();
        let now = Instant::now();
        assert_eq!(
            desk.report(1, focus(10, 100, Mode::Abc), now),
            [Step::Tell(Some(Mode::Abc))]
        );
        desk.report(2, focus(20, 200, Mode::Kana), now);
        assert_eq!(
            desk.request("get", None, now),
            [Step::Answer("get", Some(Mode::Kana))]
        );
    }

    #[test]
    fn a_focus_reported_late_by_a_field_that_took_it_before_is_not_followed() {
        let mut desk = Desk::default();
        let now = Instant::now();
        desk.report(2, focus(20, 200, Mode::Kana), now);
        assert_eq!(
            desk.report(1, focus(10, 100, Mode::Abc), now),
            [Step::Tell(Some(Mode::Kana))]
        );
        assert_eq!(desk.report(1, Report::Blur { thread: 10 }, now), []);
        assert_eq!(
            desk.request("get", None, now),
            [Step::Answer("get", Some(Mode::Kana))]
        );
    }

    #[test]
    fn only_the_field_with_the_focus_changes_the_mode_told_or_takes_it_away() {
        let mut desk: Desk<&str> = Desk::default();
        let now = Instant::now();
        desk.report(1, focus(10, 100, Mode::Abc), now);
        let other = Report::Mode {
            thread: 11,
            mode: Mode::Kana,
        };
        assert_eq!(desk.report(1, other, now), []);
        assert_eq!(desk.report(2, Report::Blur { thread: 10 }, now), []);
        let ours = Report::Mode {
            thread: 10,
            mode: Mode::Kana,
        };
        assert_eq!(desk.report(1, ours, now), [Step::Tell(Some(Mode::Kana))]);
        assert_eq!(
            desk.report(1, Report::Blur { thread: 10 }, now),
            [Step::Tell(None)]
        );
    }

    #[test]
    fn a_connection_that_ends_takes_the_focus_of_its_fields_with_it() {
        let mut desk: Desk<&str> = Desk::default();
        let now = Instant::now();
        desk.report(1, focus(10, 100, Mode::Kana), now);
        assert_eq!(desk.gone(2, now), []);
        assert_eq!(desk.gone(1, now), [Step::Tell(None)]);
    }

    #[test]
    fn a_mode_to_set_goes_to_the_field_and_is_answered_once_it_is_done() {
        let mut desk = Desk::default();
        let now = Instant::now();
        desk.report(3, focus(30, 100, Mode::Abc), now);
        let steps = desk.request("set", Some(Mode::Kana), now);
        let [
            Step::Send(
                3,
                Command::SetMode {
                    thread: 30,
                    request,
                    mode: Mode::Kana,
                },
            ),
        ] = steps[..]
        else {
            panic!("{steps:?}");
        };
        assert_eq!(desk.deadline(), Some(now + ANSWER_WITHIN));
        let changed = Report::Mode {
            thread: 30,
            mode: Mode::Kana,
        };
        assert_eq!(desk.report(3, changed, now), [Step::Tell(Some(Mode::Kana))]);
        assert_eq!(
            desk.report(
                3,
                Report::Done {
                    thread: 30,
                    request
                },
                now
            ),
            [Step::Answer("set", Some(Mode::Kana))]
        );
        assert_eq!(desk.deadline(), None);
    }

    #[test]
    fn requests_after_a_mode_to_set_wait_for_it_and_keep_their_order() {
        let mut desk = Desk::default();
        let now = Instant::now();
        desk.report(3, focus(30, 100, Mode::Abc), now);
        desk.request("set", Some(Mode::Kana), now);
        assert_eq!(desk.request("get", None, now), []);
        let done = Report::Done {
            thread: 30,
            request: 0,
        };
        assert_eq!(
            desk.report(3, done, now),
            [
                Step::Answer("set", Some(Mode::Abc)),
                Step::Answer("get", Some(Mode::Abc))
            ]
        );
    }

    #[test]
    fn a_field_that_does_not_answer_in_time_is_answered_for_with_the_mode_known() {
        let mut desk = Desk::default();
        let now = Instant::now();
        desk.report(3, focus(30, 100, Mode::Abc), now);
        desk.request("set", Some(Mode::Kana), now);
        desk.request("get", None, now);
        assert_eq!(desk.tick(now + ANSWER_WITHIN / 2), []);
        assert_eq!(
            desk.tick(now + ANSWER_WITHIN),
            [
                Step::Answer("set", Some(Mode::Abc)),
                Step::Answer("get", Some(Mode::Abc))
            ]
        );
        let late = Report::Done {
            thread: 30,
            request: 0,
        };
        assert_eq!(desk.report(3, late, now), [], "a late answer is not taken");
    }

    #[test]
    fn a_mode_to_set_on_a_connection_that_ends_is_answered_at_once() {
        let mut desk = Desk::default();
        let now = Instant::now();
        desk.report(3, focus(30, 100, Mode::Abc), now);
        desk.request("set", Some(Mode::Kana), now);
        assert_eq!(
            desk.gone(3, now),
            [Step::Tell(None), Step::Answer("set", None)]
        );
    }

    #[test]
    fn a_done_from_another_thread_or_connection_answers_nothing() {
        let mut desk = Desk::default();
        let now = Instant::now();
        desk.report(3, focus(30, 100, Mode::Abc), now);
        desk.request("set", Some(Mode::Kana), now);
        let elsewhere = Report::Done {
            thread: 31,
            request: 0,
        };
        assert_eq!(desk.report(3, elsewhere, now), []);
        let spoofed = Report::Done {
            thread: 30,
            request: 0,
        };
        assert_eq!(desk.report(4, spoofed, now), []);
        assert!(desk.deadline().is_some());
    }

    #[test]
    fn requests_wait_only_up_to_a_bound() {
        let mut desk = Desk::default();
        let now = Instant::now();
        desk.report(3, focus(30, 100, Mode::Abc), now);
        desk.request(0, Some(Mode::Kana), now);
        for request in 1..=MAX_WAITING {
            assert!(desk.has_room());
            desk.request(request, None, now);
        }
        assert!(!desk.has_room());
    }
}
