//! Physical terminal event filtering used by the hosted PTY session.
//!
//! Crossterm owns the physical input parser. A hosted child, however, cannot
//! receive the outer terminal's OSC 11 response directly: the response is
//! parsed by crossterm as a run of Alt/character events. This adapter keeps
//! crossterm as the sole reader and consumes only a fully validated OSC 11
//! response. Every malformed or timed-out candidate is replayed as the exact
//! events crossterm produced.

use std::{
    collections::VecDeque,
    io::{self, Write},
    time::Duration,
};

use crossterm::event::{Event, EventStream, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use futures_util::{Stream, StreamExt};

use super::screen::TerminalBackground;
use super::session::{TerminalSessionEventFuture, TerminalSessionEventSource};

const OSC_CAPTURE_TIMEOUT: Duration = Duration::from_millis(100);
const OSC11_PREFIX: &str = "11;rgb";

/// Crossterm-backed physical event stream with a narrow OSC 11 response
/// filter. It is intentionally the only reader of the host terminal input.
pub(crate) struct TerminalEventStream<S = EventStream> {
    inner: S,
    pending: VecDeque<Event>,
    capture: Option<Osc11Capture>,
    capture_deadline: Option<tokio::time::Instant>,
    background: Option<TerminalBackground>,
}

impl TerminalEventStream<EventStream> {
    #[must_use]
    pub(crate) fn new() -> Self {
        Self::from_stream(EventStream::new())
    }
}

impl<S> TerminalEventStream<S>
where
    S: Stream<Item = io::Result<Event>> + Unpin,
{
    fn from_stream(inner: S) -> Self {
        Self {
            inner,
            pending: VecDeque::new(),
            capture: None,
            capture_deadline: None,
            background: None,
        }
    }

    /// Sends one outer-terminal appearance query and waits briefly for its
    /// response while retaining ordinary physical events in `pending`.
    ///
    /// Unsupported terminals simply use the conservative dark fallback. The
    /// event filter remains active after the 500 ms startup deadline, so a
    /// delayed response whose events arrive within the 100 ms fragment window
    /// is consumed later instead of leaking into the child editor. A terminal
    /// that never answers, or a response split across a longer gap, keeps the
    /// conservative dark fallback and replays the partial input.
    pub(crate) async fn probe_background<W: Write>(
        &mut self,
        writer: &mut W,
    ) -> io::Result<TerminalBackground> {
        writer.write_all(b"\x1b]11;?\x07\x1b[c")?;
        writer.flush()?;

        let deadline = tokio::time::sleep(Duration::from_millis(500));
        tokio::pin!(deadline);
        loop {
            if let Some(background) = self.background {
                return Ok(background);
            }
            tokio::select! {
                result = self.pump_one() => {
                    match result {
                        Some(Ok(_)) => continue,
                        Some(Err(error)) => return Err(error),
                        None => return Ok(TerminalBackground::DARK),
                    }
                }
                () = &mut deadline => return Ok(TerminalBackground::DARK),
            }
        }
    }

    pub(crate) fn terminal_background(&self) -> Option<(u16, u16, u16)> {
        self.background.map(TerminalBackground::components)
    }

    fn feed_event(&mut self, event: Event) {
        let Some(capture) = self.capture.take() else {
            if is_osc11_start(&event) {
                self.capture = Some(Osc11Capture::prefix(event));
                self.capture_deadline = Some(tokio::time::Instant::now() + OSC_CAPTURE_TIMEOUT);
            } else {
                self.pending.push_back(event);
            }
            return;
        };

        self.capture_deadline = None;
        match capture.feed(event) {
            CaptureResult::Continue(capture) => {
                self.capture = Some(capture);
                self.capture_deadline = Some(tokio::time::Instant::now() + OSC_CAPTURE_TIMEOUT);
            }
            CaptureResult::Consumed(background) => {
                self.background = Some(background);
            }
            CaptureResult::Replay(events) => {
                self.pending.extend(events);
            }
        }
    }

    fn flush_capture(&mut self) {
        self.capture_deadline = None;
        if let Some(capture) = self.capture.take() {
            self.pending.extend(capture.events());
        }
    }

    async fn next_filtered(&mut self) -> Option<io::Result<Event>> {
        loop {
            if let Some(event) = self.pending.pop_front() {
                return Some(Ok(event));
            }
            match self.pump_one().await {
                Some(Ok(())) => {}
                Some(Err(error)) => return Some(Err(error)),
                None => return self.pending.pop_front().map(Ok),
            }
        }
    }

    /// Reads and classifies one crossterm event without consuming the queued
    /// user events. The startup probe uses this operation so ordinary keys
    /// typed while the query is in flight remain available to the session.
    async fn pump_one(&mut self) -> Option<io::Result<()>> {
        // The session's select! cancels this future on redraws and PTY output.
        // Keep the fragment deadline in the stream so cancellation cannot
        // indefinitely postpone replay of a partial reply or ordinary Alt+].
        let result = if let Some(deadline) = self.capture_deadline {
            match tokio::time::timeout_at(deadline, StreamExt::next(&mut self.inner)).await {
                Ok(result) => result,
                Err(_) => {
                    self.flush_capture();
                    return Some(Ok(()));
                }
            }
        } else {
            StreamExt::next(&mut self.inner).await
        };

        match result {
            Some(Ok(event)) => {
                self.feed_event(event);
                Some(Ok(()))
            }
            Some(Err(error)) => Some(Err(error)),
            None => {
                self.flush_capture();
                None
            }
        }
    }
}

impl<S> TerminalSessionEventSource for TerminalEventStream<S>
where
    S: Stream<Item = io::Result<Event>> + Unpin,
{
    fn next(&mut self) -> TerminalSessionEventFuture<'_> {
        Box::pin(self.next_filtered())
    }

    fn terminal_background(&self) -> Option<(u16, u16, u16)> {
        self.terminal_background()
    }
}

#[derive(Debug)]
enum Osc11Capture {
    Prefix { events: Vec<Event>, text: String },
    Color { events: Vec<Event>, text: String },
}

#[derive(Debug)]
enum CaptureResult {
    Continue(Osc11Capture),
    Consumed(TerminalBackground),
    Replay(Vec<Event>),
}

impl Osc11Capture {
    fn prefix(event: Event) -> Self {
        Self::Prefix {
            events: vec![event],
            text: String::new(),
        }
    }

    fn events(self) -> Vec<Event> {
        match self {
            Self::Prefix { events, .. } | Self::Color { events, .. } => events,
        }
    }

    fn feed(mut self, event: Event) -> CaptureResult {
        match &mut self {
            Self::Prefix { events, text } => {
                let Some(character) = plain_character(&event) else {
                    return CaptureResult::Replay(replay_with(events, event));
                };
                let mut candidate = text.clone();
                candidate.push(character);
                if !OSC11_PREFIX.starts_with(candidate.as_str()) {
                    return CaptureResult::Replay(replay_with(events, event));
                }
                events.push(event);
                text.push(character);
                if text == OSC11_PREFIX {
                    return CaptureResult::Continue(Self::Color {
                        events: std::mem::take(events),
                        text: text.clone(),
                    });
                }
                CaptureResult::Continue(self)
            }
            Self::Color { events, text } => {
                if is_osc_terminator(&event) {
                    let Some(background) = parse_osc11_text(text) else {
                        return CaptureResult::Replay(replay_with(events, event));
                    };
                    return CaptureResult::Consumed(background);
                }
                let Some(character) = plain_character(&event) else {
                    return CaptureResult::Replay(replay_with(events, event));
                };
                let mut candidate = text.clone();
                candidate.push(character);
                if !valid_osc11_body(&candidate) {
                    return CaptureResult::Replay(replay_with(events, event));
                }
                events.push(event);
                text.push(character);
                CaptureResult::Continue(self)
            }
        }
    }
}

fn replay_with(events: &[Event], event: Event) -> Vec<Event> {
    let mut replay = Vec::with_capacity(events.len() + 1);
    replay.extend(events.iter().cloned());
    replay.push(event);
    replay
}

fn is_osc11_start(event: &Event) -> bool {
    matches!(
        event,
        Event::Key(KeyEvent {
            code: KeyCode::Char(']'),
            modifiers,
            kind: KeyEventKind::Press,
            ..
        }) if *modifiers == KeyModifiers::ALT
    )
}

fn plain_character(event: &Event) -> Option<char> {
    let Event::Key(key) = event else {
        return None;
    };
    if key.kind != KeyEventKind::Press
        || !(key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT)
    {
        return None;
    }
    match key.code {
        KeyCode::Char(character) => Some(character),
        _ => None,
    }
}

fn is_osc_terminator(event: &Event) -> bool {
    match event {
        Event::Key(KeyEvent {
            code: KeyCode::Char('g' | 'G'),
            modifiers,
            kind: KeyEventKind::Press,
            ..
        }) => modifiers.contains(KeyModifiers::CONTROL),
        Event::Key(KeyEvent {
            code: KeyCode::Char('\\'),
            modifiers,
            kind: KeyEventKind::Press,
            ..
        }) => modifiers.contains(KeyModifiers::ALT),
        _ => false,
    }
}

fn valid_osc11_body(text: &str) -> bool {
    let Some(color) = text.strip_prefix("11;rgb:") else {
        return text == OSC11_PREFIX;
    };
    if color.is_empty() {
        return true;
    }
    let components: Vec<&str> = color.split('/').collect();
    if components.len() > 3
        || components
            .iter()
            .take(components.len().saturating_sub(1))
            .any(|component| component.is_empty())
        || components.iter().any(|component| component.len() > 4)
    {
        return false;
    }
    components
        .iter()
        .all(|component| component.bytes().all(|byte| byte.is_ascii_hexdigit()))
}

fn parse_osc11_text(text: &str) -> Option<TerminalBackground> {
    let (prefix, color) = text.split_once(':')?;
    if prefix != "11;rgb" {
        return None;
    }
    if color.split('/').count() != 3 {
        return None;
    }
    let mut components = color.as_bytes().split(|byte| *byte == b'/');
    TerminalBackground::from_rgb_components(
        components.next()?,
        components.next()?,
        components.next()?,
    )
}

#[cfg(test)]
mod tests {
    use std::io;

    use futures_util::stream;

    use super::*;

    fn key(character: char) -> Event {
        Event::Key(KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE))
    }

    fn osc_start() -> Event {
        Event::Key(KeyEvent::new(KeyCode::Char(']'), KeyModifiers::ALT))
    }

    #[test]
    fn valid_osc11_rgb_body_parses_and_unsupported_variants_are_rejected() {
        assert!(valid_osc11_body("11;rgb:ffff/0000/0000"));
        assert!(!valid_osc11_body("11;rgba:ffff/0000/0000"));
        assert!(!valid_osc11_body("11;rgb:ffff/0000/0000/ffff"));
        assert_eq!(
            parse_osc11_text("11;rgb:ffff/0000/0000"),
            TerminalBackground::from_rgb_components(b"ffff", b"0000", b"0000")
        );
        assert_eq!(parse_osc11_text("11;rgba:ffff/0000/0000"), None);
    }

    #[test]
    fn malformed_candidate_replays_every_event_in_order() {
        let capture = Osc11Capture::prefix(osc_start());
        let CaptureResult::Replay(events) = capture.feed(key('x')) else {
            panic!("malformed candidate must be replayed");
        };
        assert_eq!(events, vec![osc_start(), key('x')]);
    }

    fn response_events(body: &str, terminator: Event) -> Vec<Event> {
        let mut events = vec![osc_start()];
        events.extend(body.chars().map(|character| {
            let modifiers = if character.is_ascii_uppercase() {
                KeyModifiers::SHIFT
            } else {
                KeyModifiers::NONE
            };
            Event::Key(KeyEvent::new(KeyCode::Char(character), modifiers))
        }));
        events.push(terminator);
        events
    }

    fn bel() -> Event {
        Event::Key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL))
    }

    fn st() -> Event {
        Event::Key(KeyEvent::new(KeyCode::Char('\\'), KeyModifiers::ALT))
    }

    #[tokio::test(flavor = "current_thread")]
    async fn probe_preserves_prelaunch_events_and_consumes_uppercase_bel_reply() {
        let pasted = Event::Paste("prélaunch\t🙂".to_owned());
        let released = Event::Key(KeyEvent::new_with_kind(
            KeyCode::Char('x'),
            KeyModifiers::CONTROL,
            KeyEventKind::Release,
        ));
        let trailing = Event::Key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::ALT));
        let mut input = vec![pasted.clone(), released.clone()];
        input.extend(response_events("11;rgb:FFFF/8000/00AB", bel()));
        input.push(trailing.clone());
        let mut events = TerminalEventStream::from_stream(stream::iter(
            input.into_iter().map(Ok::<Event, io::Error>),
        ));
        let mut query = Vec::new();

        let background = events.probe_background(&mut query).await.unwrap();

        assert_eq!(
            background,
            TerminalBackground::from_rgb_components(b"FFFF", b"8000", b"00AB")
                .expect("valid measured color")
        );
        assert_eq!(events.terminal_background(), Some((65535, 32768, 171)));
        assert_eq!(query, b"\x1b]11;?\x07\x1b[c");
        assert_eq!(events.next().await.unwrap().unwrap(), pasted);
        assert_eq!(events.next().await.unwrap().unwrap(), released);
        assert_eq!(events.next().await.unwrap().unwrap(), trailing);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn probe_consumes_st_reply_and_falls_back_dark_without_reply() {
        let trailing = key('z');
        let mut input = response_events("11;rgb:0000/0000/FFFF", st());
        input.push(trailing.clone());
        let mut events = TerminalEventStream::from_stream(stream::iter(
            input.into_iter().map(Ok::<Event, io::Error>),
        ));
        let mut query = Vec::new();
        assert_eq!(
            events.probe_background(&mut query).await.unwrap(),
            TerminalBackground::from_rgb_components(b"0000", b"0000", b"FFFF")
                .expect("valid measured color")
        );
        assert_eq!(events.next().await.unwrap().unwrap(), trailing);

        let mut unsupported = TerminalEventStream::from_stream(stream::empty());
        let mut unsupported_query = Vec::new();
        assert_eq!(
            unsupported
                .probe_background(&mut unsupported_query)
                .await
                .unwrap(),
            TerminalBackground::DARK
        );
        assert_eq!(unsupported.terminal_background(), None);
        assert_eq!(unsupported_query, b"\x1b]11;?\x07\x1b[c");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn capture_timeout_replays_partial_candidate() {
        let mut events = TerminalEventStream::from_stream(stream::pending());
        events.feed_event(osc_start());

        assert!(matches!(events.pump_one().await, Some(Ok(()))));
        assert_eq!(events.pending.pop_front(), Some(osc_start()));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn capture_timeout_survives_session_scheduler_cancellation() {
        let mut events = TerminalEventStream::from_stream(stream::pending());
        events.feed_event(osc_start());
        let replayed = tokio::time::timeout(Duration::from_millis(500), async {
            loop {
                tokio::select! {
                    event = events.next() => break event.unwrap().unwrap(),
                    () = tokio::time::sleep(Duration::from_millis(10)) => {}
                }
            }
        })
        .await
        .expect("repeated session polling must not extend the fragment timeout");
        assert_eq!(replayed, osc_start());
    }

    #[test]
    fn release_terminator_is_replayed_instead_of_consumed() {
        let response = response_events("11;rgb:ffff/0000/0000", bel());
        let mut capture = Osc11Capture::prefix(response[0].clone());
        for event in response.iter().skip(1).take(response.len() - 2) {
            capture = match capture.feed(event.clone()) {
                CaptureResult::Continue(capture) => capture,
                other => panic!("unexpected capture result: {other:?}"),
            };
        }
        let release = Event::Key(KeyEvent::new_with_kind(
            KeyCode::Char('g'),
            KeyModifiers::CONTROL,
            KeyEventKind::Release,
        ));
        let CaptureResult::Replay(events) = capture.feed(release.clone()) else {
            panic!("release terminator must not consume a response");
        };
        assert_eq!(events.last(), Some(&release));
    }
}
