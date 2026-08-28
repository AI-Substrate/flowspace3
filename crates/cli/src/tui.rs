//! The interactive terminal dashboard.
//!
//! Snapshots use [`DaemonClient`], exactly like the non-interactive verbs. The
//! activity pane consumes the frozen `fs3.events` NDJSON wire directly. Neither
//! path shells out to the `flowspace3` binary or invents activity.

use std::collections::VecDeque;
use std::io::{self, Stdout};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use crossterm::event::{
    self, Event as TerminalEvent, KeyCode, KeyEvent, KeyEventKind, KeyModifiers,
};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use fs3_core::envelope::Envelope;
use fs3_core::events::{Event, EventKind, Hello, STREAM_NAME, STREAM_VERSION};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, BorderType, Borders, List, ListItem, ListState, Paragraph, Row, Sparkline, Table, Wrap,
};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use tokio::sync::mpsc as async_mpsc;
use tokio::task::JoinHandle;

use crate::DaemonClient;

const SNAPSHOT_INTERVAL: Duration = Duration::from_secs(2);
const STREAM_RETRY_INTERVAL: Duration = Duration::from_secs(2);
const UI_TICK: Duration = Duration::from_millis(100);
const MAX_ACTIVITY: usize = 100;
const MAX_QUEUE_HISTORY: usize = 60;
const MESSAGE_CAPACITY: usize = 256;
const MESSAGE_BUDGET: usize = 32;

/// Run the dashboard until the user presses `q` outside the search editor.
///
/// Terminal ownership is guarded before any drawing starts. Unwinding through
/// this function restores the alternate screen and raw mode through `Drop`.
pub async fn run(client: DaemonClient) -> Result<()> {
    let mut terminal = TerminalSession::enter()?;
    let messages = Mailbox::default();
    let (query_tx, query_rx) = async_mpsc::unbounded_channel();

    let snapshot_task = spawn_snapshot_worker(client.clone(), messages.clone());
    let search_task = spawn_search_worker(client.clone(), query_rx, messages.clone());
    let stream_task = spawn_event_worker(client, messages.clone());

    let mut app = App::default();
    let outcome = run_loop(&mut terminal.terminal, &mut app, &messages, &query_tx);

    snapshot_task.abort();
    search_task.abort();
    stream_task.abort();
    outcome
}

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut App,
    messages: &Mailbox,
    queries: &async_mpsc::UnboundedSender<String>,
) -> Result<()> {
    loop {
        messages.apply_to(app, MESSAGE_BUDGET);

        terminal.draw(|frame| draw(frame, app))?;

        if !event::poll(UI_TICK).context("polling terminal input")? {
            continue;
        }
        let TerminalEvent::Key(key) = event::read().context("reading terminal input")? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }

        match app.handle_key(key) {
            Action::Continue => {}
            Action::Quit => return Ok(()),
            Action::Search(query) => {
                app.searching = true;
                app.search_error = None;
                queries
                    .send(query)
                    .map_err(|_| anyhow::anyhow!("search worker stopped"))?;
            }
        }
    }
}

struct TerminalSession {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl TerminalSession {
    fn enter() -> Result<Self> {
        enable_raw_mode().context("enabling terminal raw mode")?;
        let mut stdout = io::stdout();
        if let Err(error) = execute!(stdout, EnterAlternateScreen) {
            let _ = disable_raw_mode();
            return Err(error).context("entering the alternate screen");
        }

        let backend = CrosstermBackend::new(stdout);
        match Terminal::new(backend) {
            Ok(terminal) => Ok(Self { terminal }),
            Err(error) => {
                let mut stdout = io::stdout();
                let _ = execute!(stdout, LeaveAlternateScreen);
                let _ = disable_raw_mode();
                Err(error).context("creating the terminal dashboard")
            }
        }
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
        let _ = self.terminal.show_cursor();
    }
}

#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
struct StatusData {
    #[serde(default)]
    roots: Vec<RootRow>,
    #[serde(default)]
    queue: Vec<QueueRow>,
}

#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
struct RootRow {
    identity: String,
    root_path: String,
    files: i64,
}

#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
struct QueueRow {
    kind: String,
    state: String,
    count: i64,
    #[serde(default)]
    with_error: i64,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
struct SearchData {
    #[serde(default)]
    results: Vec<SearchHit>,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
struct SearchHit {
    address: String,
    #[serde(default)]
    score: f64,
    #[serde(default)]
    path: String,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    snippet: String,
    #[serde(default)]
    smart: Option<String>,
}

#[derive(Debug)]
enum WorkerMessage {
    Snapshot(Result<StatusData, String>),
    Search(Result<Vec<SearchHit>, String>),
    StreamConnected(String),
    StreamEvent(Event),
    StreamDisconnected(String),
}

/// A bounded, newest-wins handoff from I/O workers to the draw loop.
///
/// Snapshot, search, and connection-state messages coalesce. When the queue is
/// full, an old stream event is discarded before control state, matching the
/// activity pane's own bounded newest-first history.
#[derive(Clone, Debug)]
struct Mailbox {
    queue: Arc<Mutex<VecDeque<WorkerMessage>>>,
    capacity: usize,
}

impl Default for Mailbox {
    fn default() -> Self {
        Self::new(MESSAGE_CAPACITY)
    }
}

impl Mailbox {
    fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "mailbox capacity must be positive");
        Self {
            queue: Arc::new(Mutex::new(VecDeque::with_capacity(capacity))),
            capacity,
        }
    }

    fn push(&self, message: WorkerMessage) {
        let mut queue = self
            .queue
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let superseded = match &message {
            WorkerMessage::Snapshot(_) => queue
                .iter()
                .position(|queued| matches!(queued, WorkerMessage::Snapshot(_))),
            WorkerMessage::Search(_) => queue
                .iter()
                .position(|queued| matches!(queued, WorkerMessage::Search(_))),
            WorkerMessage::StreamConnected(_) | WorkerMessage::StreamDisconnected(_) => {
                queue.iter().position(|queued| {
                    matches!(
                        queued,
                        WorkerMessage::StreamConnected(_) | WorkerMessage::StreamDisconnected(_)
                    )
                })
            }
            WorkerMessage::StreamEvent(_) => None,
        };
        if let Some(index) = superseded {
            queue.remove(index);
        }

        if queue.len() == self.capacity {
            let oldest_event = queue
                .iter()
                .position(|queued| matches!(queued, WorkerMessage::StreamEvent(_)));
            if let Some(index) = oldest_event {
                queue.remove(index);
            } else {
                queue.pop_front();
            }
        }
        queue.push_back(message);
    }

    fn apply_to(&self, app: &mut App, budget: usize) -> usize {
        let mut queue = self
            .queue
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let count = budget.min(queue.len());
        for _ in 0..count {
            if let Some(message) = queue.pop_front() {
                app.apply(message);
            }
        }
        count
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.queue
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len()
    }
}

fn spawn_snapshot_worker(client: DaemonClient, messages: Mailbox) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            let status = decode_envelope(client.status().await);
            messages.push(WorkerMessage::Snapshot(status));
            tokio::time::sleep(SNAPSHOT_INTERVAL).await;
        }
    })
}

fn spawn_search_worker(
    client: DaemonClient,
    mut queries: async_mpsc::UnboundedReceiver<String>,
    messages: Mailbox,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        while let Some(query) = queries.recv().await {
            let mut params = vec![
                ("q".to_string(), query),
                ("limit".to_string(), "30".to_string()),
            ];
            if let Ok(cwd) = std::env::current_dir() {
                params.push(("cwd".to_string(), cwd.to_string_lossy().into_owned()));
            }
            let result = decode_envelope::<SearchData>(client.search(&params).await)
                .map(|data| data.results);
            messages.push(WorkerMessage::Search(result));
        }
    })
}

fn decode_envelope<T: DeserializeOwned>(envelope: Envelope) -> Result<T, String> {
    if let Some(error) = envelope.error {
        return Err(error.render());
    }
    let data = envelope
        .data
        .ok_or_else(|| "daemon returned a successful envelope without data".to_string())?;
    serde_json::from_value(data)
        .map_err(|error| format!("daemon returned unexpected data: {error}"))
}

fn spawn_event_worker(client: DaemonClient, messages: Mailbox) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            let outcome = consume_event_stream(&client, &messages).await;
            let reason = outcome
                .err()
                .unwrap_or_else(|| "event stream ended".to_string());
            messages.push(WorkerMessage::StreamDisconnected(reason));
            tokio::time::sleep(STREAM_RETRY_INTERVAL).await;
        }
    })
}

/// The one composition seam for u-w's live endpoint.
///
/// Authentication and transport stay private to [`DaemonClient`]. The parser,
/// reconnect policy, and UI only receive its raw streamed response.
async fn event_source(client: &DaemonClient) -> Result<reqwest::Response, String> {
    client
        .events(None)
        .await
        .map_err(|failure| failure.render())
}

async fn consume_event_stream(client: &DaemonClient, messages: &Mailbox) -> Result<(), String> {
    let mut response = event_source(client).await?;

    let mut decoder = EventDecoder::default();
    loop {
        let wait = decoder.heartbeat_timeout();
        let chunk = tokio::time::timeout(wait, response.chunk())
            .await
            .map_err(|_| format!("no event or heartbeat for {}s", wait.as_secs()))?
            .map_err(|error| format!("event stream read failed: {error}"))?;
        let Some(chunk) = chunk else {
            decoder.finish()?;
            return Err("event stream closed".to_string());
        };

        for record in decoder.push(&chunk)? {
            let message = match record {
                StreamRecord::Hello(hello) => WorkerMessage::StreamConnected(hello.daemon),
                StreamRecord::Event(event) => WorkerMessage::StreamEvent(event),
            };
            messages.push(message);
        }
    }
}

#[derive(Debug)]
enum StreamRecord {
    Hello(Hello),
    Event(Event),
}

#[derive(Debug)]
struct EventDecoder {
    buffer: Vec<u8>,
    greeted: bool,
    heartbeat_ms: u64,
}

impl Default for EventDecoder {
    fn default() -> Self {
        Self {
            buffer: Vec::new(),
            greeted: false,
            heartbeat_ms: DaemonClient::TIMEOUT.as_millis() as u64,
        }
    }
}

impl EventDecoder {
    fn heartbeat_timeout(&self) -> Duration {
        Duration::from_millis(self.heartbeat_ms.saturating_mul(2).max(1_000))
    }

    fn push(&mut self, bytes: &[u8]) -> Result<Vec<StreamRecord>, String> {
        self.buffer.extend_from_slice(bytes);
        let mut records = Vec::new();

        while let Some(newline) = self.buffer.iter().position(|byte| *byte == b'\n') {
            let mut line = self.buffer.drain(..=newline).collect::<Vec<_>>();
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            if line.is_empty() {
                continue;
            }

            if !self.greeted {
                let hello: Hello = serde_json::from_slice(&line)
                    .map_err(|error| format!("invalid event stream hello: {error}"))?;
                if hello.stream != STREAM_NAME || hello.v != STREAM_VERSION {
                    return Err(format!(
                        "unsupported event stream {} v{} (expected {} v{})",
                        hello.stream, hello.v, STREAM_NAME, STREAM_VERSION
                    ));
                }
                self.heartbeat_ms = hello.heartbeat_ms;
                self.greeted = true;
                records.push(StreamRecord::Hello(hello));
                continue;
            }

            let event: Event = serde_json::from_slice(&line)
                .map_err(|error| format!("invalid event line: {error}"))?;
            if event.v != STREAM_VERSION {
                return Err(format!(
                    "unsupported event v{} (expected v{})",
                    event.v, STREAM_VERSION
                ));
            }
            records.push(StreamRecord::Event(event));
        }

        Ok(records)
    }

    fn finish(&self) -> Result<(), String> {
        if self.buffer.is_empty() {
            Ok(())
        } else {
            Err("event stream disconnected in the middle of a line".to_string())
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Focus {
    Roots,
    Queue,
    Activity,
    Search,
}

impl Focus {
    fn next(self) -> Self {
        match self {
            Self::Roots => Self::Queue,
            Self::Queue => Self::Activity,
            Self::Activity => Self::Search,
            Self::Search => Self::Roots,
        }
    }
}

#[derive(Debug, Default)]
struct InputBuffer {
    text: String,
    cursor: usize,
    history: Vec<String>,
    history_index: Option<usize>,
}

impl InputBuffer {
    fn byte_index(&self) -> usize {
        self.text
            .char_indices()
            .nth(self.cursor)
            .map_or(self.text.len(), |(index, _)| index)
    }

    fn insert(&mut self, character: char) {
        let index = self.byte_index();
        self.text.insert(index, character);
        self.cursor += 1;
        self.history_index = None;
    }

    fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        self.cursor -= 1;
        let start = self.byte_index();
        let end = self.text[start..]
            .char_indices()
            .nth(1)
            .map_or(self.text.len(), |(offset, _)| start + offset);
        self.text.replace_range(start..end, "");
    }

    fn delete(&mut self) {
        let start = self.byte_index();
        if start == self.text.len() {
            return;
        }
        let end = self.text[start..]
            .char_indices()
            .nth(1)
            .map_or(self.text.len(), |(offset, _)| start + offset);
        self.text.replace_range(start..end, "");
    }

    fn left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    fn right(&mut self) {
        self.cursor = (self.cursor + 1).min(self.text.chars().count());
    }

    fn home(&mut self) {
        self.cursor = 0;
    }

    fn end(&mut self) {
        self.cursor = self.text.chars().count();
    }

    fn commit(&mut self) -> Option<String> {
        let query = self.text.trim().to_string();
        if query.is_empty() {
            return None;
        }
        if self.history.last() != Some(&query) {
            self.history.push(query.clone());
        }
        self.history_index = None;
        Some(query)
    }

    fn older(&mut self) {
        if self.history.is_empty() {
            return;
        }
        let index = self
            .history_index
            .map_or(self.history.len() - 1, |index| index.saturating_sub(1));
        self.load_history(index);
    }

    fn newer(&mut self) {
        let Some(index) = self.history_index else {
            return;
        };
        if index + 1 >= self.history.len() {
            self.history_index = None;
            self.text.clear();
            self.cursor = 0;
        } else {
            self.load_history(index + 1);
        }
    }

    fn load_history(&mut self, index: usize) {
        self.history_index = Some(index);
        self.text.clone_from(&self.history[index]);
        self.end();
    }
}

#[derive(Debug)]
struct ActivityLine {
    at: String,
    text: String,
    color: Color,
}

#[derive(Debug)]
struct App {
    focus: Focus,
    input: InputBuffer,
    roots: Vec<RootRow>,
    queue: Vec<QueueRow>,
    queue_history: VecDeque<u64>,
    activity: VecDeque<ActivityLine>,
    results: Vec<SearchHit>,
    selected_result: usize,
    searching: bool,
    search_error: Option<String>,
    last_snapshot: Option<Instant>,
    snapshot_error: Option<String>,
    stream_connected: bool,
    stream_daemon: Option<String>,
    stream_error: Option<String>,
}

impl Default for App {
    fn default() -> Self {
        Self {
            focus: Focus::Search,
            input: InputBuffer::default(),
            roots: Vec::new(),
            queue: Vec::new(),
            queue_history: VecDeque::new(),
            activity: VecDeque::new(),
            results: Vec::new(),
            selected_result: 0,
            searching: false,
            search_error: None,
            last_snapshot: None,
            snapshot_error: None,
            stream_connected: false,
            stream_daemon: None,
            stream_error: Some("connecting to real activity stream".to_string()),
        }
    }
}

impl App {
    fn apply(&mut self, message: WorkerMessage) {
        match message {
            WorkerMessage::Snapshot(Ok(status)) => {
                self.roots = status.roots;
                self.queue = status.queue;
                self.last_snapshot = Some(Instant::now());
                self.snapshot_error = None;
                self.remember_queue_depth();
            }
            WorkerMessage::Snapshot(Err(error)) => {
                self.snapshot_error = Some(error);
            }
            WorkerMessage::Search(Ok(results)) => {
                self.results = results;
                self.selected_result = 0;
                self.searching = false;
                self.search_error = None;
            }
            WorkerMessage::Search(Err(error)) => {
                self.searching = false;
                self.search_error = Some(error);
            }
            WorkerMessage::StreamConnected(daemon) => {
                self.stream_connected = true;
                self.stream_daemon = Some(daemon);
                self.stream_error = None;
            }
            WorkerMessage::StreamDisconnected(error) => {
                self.stream_connected = false;
                self.stream_error = Some(error);
            }
            WorkerMessage::StreamEvent(event) => self.apply_event(event),
        }
    }

    fn apply_event(&mut self, event: Event) {
        match event.kind {
            EventKind::JobDone {
                job,
                subject,
                ms,
                left,
            } => self.push_activity(
                event.at,
                format!("{job} finished · {subject} · {ms}ms · {left} left"),
                Color::Green,
            ),
            EventKind::JobFailed {
                job,
                subject,
                error,
                attempts,
                terminal,
            } => self.push_activity(
                event.at,
                format!(
                    "{job} {} · {subject} · attempt {attempts}: {error}",
                    if terminal { "failed" } else { "retrying" }
                ),
                if terminal { Color::Red } else { Color::Yellow },
            ),
            EventKind::Queue { rows } => {
                self.queue = rows
                    .into_iter()
                    .map(|row| QueueRow {
                        kind: row.kind,
                        state: row.state,
                        count: row.count,
                        with_error: 0,
                    })
                    .collect();
                self.remember_queue_depth();
            }
            EventKind::ScanProgress {
                root,
                files_seen,
                enqueued,
                current,
                ..
            } => self.push_activity(
                event.at,
                format!(
                    "scan {root} · {files_seen} files · {enqueued} queued{}",
                    current.map_or_else(String::new, |path| format!(" · {path}"))
                ),
                Color::Cyan,
            ),
            EventKind::RootChanged {
                change,
                root,
                root_path,
                files,
            } => {
                if change == "removed" {
                    self.roots.retain(|item| item.root_path != root_path);
                } else if let Some(item) = self
                    .roots
                    .iter_mut()
                    .find(|item| item.root_path == root_path)
                {
                    item.identity.clone_from(&root);
                    item.files = files;
                } else {
                    self.roots.push(RootRow {
                        identity: root.clone(),
                        root_path,
                        files,
                    });
                }
                self.push_activity(
                    event.at,
                    format!("root {change} · {root} · {files} files"),
                    Color::Magenta,
                );
            }
            EventKind::Heartbeat { .. } | EventKind::Unknown => {}
        }
    }

    fn push_activity(&mut self, at: String, text: String, color: Color) {
        self.activity.push_front(ActivityLine { at, text, color });
        self.activity.truncate(MAX_ACTIVITY);
    }

    fn remember_queue_depth(&mut self) {
        let active = self
            .queue
            .iter()
            .filter(|row| row.state == "pending" || row.state == "running")
            .map(|row| row.count.max(0) as u64)
            .sum();
        self.queue_history.push_back(active);
        self.queue_history.truncate(MAX_QUEUE_HISTORY);
    }

    fn handle_key(&mut self, key: KeyEvent) -> Action {
        if key.code == KeyCode::Tab {
            self.focus = self.focus.next();
            return Action::Continue;
        }
        if key.code == KeyCode::Esc && self.focus == Focus::Search {
            self.focus = Focus::Roots;
            return Action::Continue;
        }
        if key.code == KeyCode::Char('/') && self.focus != Focus::Search {
            self.focus = Focus::Search;
            return Action::Continue;
        }
        if key.code == KeyCode::Char('q') && self.focus != Focus::Search {
            return Action::Quit;
        }
        if self.focus != Focus::Search {
            return Action::Continue;
        }

        match (key.code, key.modifiers) {
            (KeyCode::Enter, _) => self.input.commit().map_or(Action::Continue, Action::Search),
            (KeyCode::Backspace, _) => {
                self.input.backspace();
                Action::Continue
            }
            (KeyCode::Delete, _) => {
                self.input.delete();
                Action::Continue
            }
            (KeyCode::Left, _) => {
                self.input.left();
                Action::Continue
            }
            (KeyCode::Right, _) => {
                self.input.right();
                Action::Continue
            }
            (KeyCode::Home, _) => {
                self.input.home();
                Action::Continue
            }
            (KeyCode::End, _) => {
                self.input.end();
                Action::Continue
            }
            (KeyCode::Up, _) => {
                self.selected_result = self.selected_result.saturating_sub(1);
                Action::Continue
            }
            (KeyCode::Down, _) => {
                if !self.results.is_empty() {
                    self.selected_result = (self.selected_result + 1).min(self.results.len() - 1);
                }
                Action::Continue
            }
            (KeyCode::Char('p'), KeyModifiers::CONTROL) => {
                self.input.older();
                Action::Continue
            }
            (KeyCode::Char('n'), KeyModifiers::CONTROL) => {
                self.input.newer();
                Action::Continue
            }
            (KeyCode::Char(character), modifiers)
                if !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.input.insert(character);
                Action::Continue
            }
            _ => Action::Continue,
        }
    }

    fn snapshot_badge(&self) -> String {
        if let Some(error) = &self.snapshot_error {
            let age = self.last_snapshot.map_or_else(
                || "never live".to_string(),
                |at| format!("{}s old", at.elapsed().as_secs()),
            );
            format!(
                "STALE · {age} · retrying every {}s · {}",
                SNAPSHOT_INTERVAL.as_secs(),
                first_line(error)
            )
        } else if let Some(at) = self.last_snapshot {
            format!("LIVE · {}s ago", at.elapsed().as_secs())
        } else {
            "CONNECTING · no snapshot yet".to_string()
        }
    }

    fn snapshot_pane_title(&self, name: &str) -> String {
        if self.snapshot_error.is_some() {
            let age = self.last_snapshot.map_or_else(
                || "never live".to_string(),
                |at| format!("{}s old", at.elapsed().as_secs()),
            );
            format!(" {name} · STALE · {age} ")
        } else if self.last_snapshot.is_some() {
            format!(" {name} · LIVE ")
        } else {
            format!(" {name} · CONNECTING ")
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Action {
    Continue,
    Quit,
    Search(String),
}

fn first_line(value: &str) -> &str {
    value.lines().next().unwrap_or(value)
}

#[derive(Debug)]
struct DashboardLayout {
    header: Rect,
    roots: Option<Rect>,
    queue: Option<Rect>,
    activity: Option<Rect>,
    search_input: Rect,
    search_results: Rect,
    footer: Rect,
}

fn dashboard_layout(area: Rect, search_focused: bool) -> DashboardLayout {
    if search_focused {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Min(3),
                Constraint::Length(1),
            ])
            .split(area);
        return DashboardLayout {
            header: rows[0],
            roots: None,
            queue: None,
            activity: None,
            search_input: rows[1],
            search_results: rows[2],
            footer: rows[3],
        };
    }

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(9),
            Constraint::Length(6),
            Constraint::Length(1),
        ])
        .split(area);
    let panes = Layout::default()
        .direction(if area.width >= 100 {
            Direction::Horizontal
        } else {
            Direction::Vertical
        })
        .constraints([
            Constraint::Percentage(34),
            Constraint::Percentage(33),
            Constraint::Percentage(33),
        ])
        .split(rows[1]);

    DashboardLayout {
        header: rows[0],
        roots: Some(panes[0]),
        queue: Some(panes[1]),
        activity: Some(panes[2]),
        search_input: rows[2],
        search_results: rows[2],
        footer: rows[3],
    }
}

fn draw(frame: &mut ratatui::Frame<'_>, app: &mut App) {
    let area = frame.area();
    let layout = dashboard_layout(area, app.focus == Focus::Search);
    draw_header(frame, layout.header, app);

    if let Some(area) = layout.roots {
        draw_roots(frame, area, app);
    }
    if let Some(area) = layout.queue {
        draw_queue(frame, area, app);
    }
    if let Some(area) = layout.activity {
        draw_activity(frame, area, app);
    }

    draw_search_input(frame, layout.search_input, app);
    if app.focus == Focus::Search {
        draw_search_results(frame, layout.search_results, app);
    }
    draw_footer(frame, layout.footer, app);
}

fn draw_header(frame: &mut ratatui::Frame<'_>, area: Rect, app: &App) {
    let stream = if app.stream_connected {
        format!(
            "activity live{}",
            app.stream_daemon
                .as_deref()
                .map_or_else(String::new, |version| format!(" · daemon {version}"))
        )
    } else {
        format!(
            "activity disconnected · retrying every {}s · {}",
            STREAM_RETRY_INTERVAL.as_secs(),
            app.stream_error
                .as_deref()
                .map(first_line)
                .unwrap_or("waiting for the daemon")
        )
    };
    let title = Line::from(vec![
        Span::styled(
            " flowspace³ ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            app.snapshot_badge(),
            status_style(app.snapshot_error.is_none()),
        ),
        Span::raw("  "),
        Span::styled(stream, status_style(app.stream_connected)),
    ]);
    frame.render_widget(
        Paragraph::new(title)
            .block(panel(" DASHBOARD ", true))
            .alignment(Alignment::Left),
        area,
    );
}

fn draw_roots(frame: &mut ratatui::Frame<'_>, area: Rect, app: &App) {
    let rows = if app.roots.is_empty() {
        vec![Row::new(["No indexed roots", "—"])]
    } else {
        app.roots
            .iter()
            .map(|root| {
                Row::new([
                    format!("{}\n{}", root.identity, root.root_path),
                    root.files.to_string(),
                ])
                .height(2)
            })
            .collect()
    };
    let table = Table::new(rows, [Constraint::Min(16), Constraint::Length(8)])
        .header(Row::new(["ROOT / PATH", "FILES"]).style(Style::default().fg(Color::DarkGray)))
        .column_spacing(1)
        .block(panel(
            app.snapshot_pane_title("ROOTS"),
            app.focus == Focus::Roots,
        ));
    frame.render_widget(table, area);
}

fn draw_queue(frame: &mut ratatui::Frame<'_>, area: Rect, app: &App) {
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(5), Constraint::Length(4)])
        .split(area);
    let rows = if app.queue.is_empty() {
        vec![Row::new(["No queued work", "—", "—"])]
    } else {
        app.queue
            .iter()
            .map(|row| {
                let count = if row.with_error > 0 {
                    format!("{} ({} err)", row.count, row.with_error)
                } else {
                    row.count.to_string()
                };
                Row::new([row.kind.clone(), row.state.clone(), count])
            })
            .collect()
    };
    frame.render_widget(
        Table::new(
            rows,
            [
                Constraint::Percentage(42),
                Constraint::Percentage(30),
                Constraint::Percentage(28),
            ],
        )
        .header(Row::new(["KIND", "STATE", "DEPTH"]).style(Style::default().fg(Color::DarkGray)))
        .block(panel(
            app.snapshot_pane_title("OPERATIONS"),
            app.focus == Focus::Queue,
        )),
        sections[0],
    );
    frame.render_widget(
        Sparkline::default()
            .block(panel(app.snapshot_pane_title("ACTIVE HISTORY"), false))
            .data(app.queue_history.iter())
            .style(Style::default().fg(Color::Yellow)),
        sections[1],
    );
}

fn draw_activity(frame: &mut ratatui::Frame<'_>, area: Rect, app: &App) {
    let items = if app.activity.is_empty() {
        vec![ListItem::new(Line::from("No activity yet."))]
    } else {
        app.activity
            .iter()
            .map(|line| {
                ListItem::new(Line::from(vec![
                    Span::styled(short_time(&line.at), Style::default().fg(Color::DarkGray)),
                    Span::raw("  "),
                    Span::styled(&line.text, Style::default().fg(line.color)),
                ]))
            })
            .collect()
    };
    let title = if app.stream_connected {
        " ACTIVITY · LIVE "
    } else {
        " ACTIVITY · DISCONNECTED "
    };
    frame.render_widget(
        List::new(items).block(panel(title, app.focus == Focus::Activity)),
        area,
    );
}

fn draw_search_input(frame: &mut ratatui::Frame<'_>, area: Rect, app: &App) {
    let prompt = if app.searching {
        "Searching… "
    } else {
        "Ask › "
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(prompt, Style::default().fg(Color::Cyan)),
            Span::raw(&app.input.text),
        ]))
        .block(panel(" SEARCH ", app.focus == Focus::Search)),
        area,
    );

    if app.focus == Focus::Search && area.width > 3 && area.height > 2 {
        let prefix = app
            .input
            .text
            .chars()
            .take(app.input.cursor)
            .collect::<String>();
        let prefix_width = Line::from(prefix).width() as u16;
        let prompt_width = Line::from(prompt).width() as u16;
        let x = area
            .x
            .saturating_add(1 + prompt_width + prefix_width)
            .min(area.right().saturating_sub(2));
        frame.set_cursor_position((x, area.y + 1));
    }
}

fn draw_search_results(frame: &mut ratatui::Frame<'_>, area: Rect, app: &mut App) {
    if let Some(error) = &app.search_error {
        frame.render_widget(
            Paragraph::new(format!(
                "Search failed: {}\n\nThe dashboard is still live. Edit the query and retry.",
                first_line(error)
            ))
            .wrap(Wrap { trim: false })
            .block(panel(" RESULTS ", true)),
            area,
        );
        return;
    }

    let split = Layout::default()
        .direction(if area.width >= 90 {
            Direction::Horizontal
        } else {
            Direction::Vertical
        })
        .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
        .split(area);

    let items = if app.results.is_empty() {
        vec![ListItem::new(if app.searching {
            "Searching the index…"
        } else {
            "Type a question and press Enter."
        })]
    } else {
        app.results
            .iter()
            .map(|hit| {
                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!("{:.2}", hit.score),
                        Style::default().fg(Color::Yellow),
                    ),
                    Span::raw("  "),
                    Span::styled(&hit.address, Style::default().fg(Color::Cyan)),
                ]))
            })
            .collect()
    };
    let mut state = ListState::default();
    if !app.results.is_empty() {
        state.select(Some(app.selected_result));
    }
    frame.render_stateful_widget(
        List::new(items)
            .block(panel(" RESULTS ", true))
            .highlight_symbol("▸ ")
            .highlight_style(
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            ),
        split[0],
        &mut state,
    );

    let detail = app.results.get(app.selected_result).map_or_else(
        || "Select a result to inspect its context.".to_string(),
        |hit| {
            format!(
                "{} · {}\n{}\n\n{}",
                hit.kind,
                hit.path,
                hit.address,
                hit.smart.as_deref().unwrap_or(&hit.snippet)
            )
        },
    );
    frame.render_widget(
        Paragraph::new(detail)
            .wrap(Wrap { trim: false })
            .block(panel(" CONTEXT ", false)),
        split[1],
    );
}

fn draw_footer(frame: &mut ratatui::Frame<'_>, area: Rect, app: &App) {
    let q = if app.focus == Focus::Search {
        "Esc leave search · q types"
    } else {
        "q quit · / search"
    };
    frame.render_widget(
        Paragraph::new(format!(
            "Tab panes · ↑↓ select · Enter search · Ctrl-P/N history · {q}"
        ))
        .style(Style::default().fg(Color::DarkGray)),
        area,
    );
}

fn panel(title: impl Into<Line<'static>>, focused: bool) -> Block<'static> {
    Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(if focused {
            Color::Cyan
        } else {
            Color::DarkGray
        }))
}

fn status_style(live: bool) -> Style {
    Style::default()
        .fg(if live { Color::Green } else { Color::Yellow })
        .add_modifier(Modifier::BOLD)
}

fn short_time(at: &str) -> &str {
    at.get(11..19).unwrap_or(at)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fs3_core::events::{EventKind, QueueDepth};

    const FIXTURE: &str = include_str!("../tests/fixtures/tui-events.ndjson");

    #[test]
    fn layout_becomes_results_dominant_when_search_is_focused() {
        let area = Rect::new(0, 0, 120, 40);
        let dashboard = dashboard_layout(area, false);
        assert!(dashboard.roots.is_some());
        assert!(dashboard.activity.is_some());

        let search = dashboard_layout(area, true);
        assert!(search.roots.is_none());
        assert!(search.activity.is_none());
        assert!(search.search_results.height > area.height / 2);
    }

    #[test]
    fn narrow_layout_stacks_the_live_panes() {
        let layout = dashboard_layout(Rect::new(0, 0, 70, 40), false);
        let roots = layout.roots.expect("roots pane");
        let queue = layout.queue.expect("queue pane");
        assert!(queue.y > roots.y);
        assert_eq!(roots.width, queue.width);
    }

    #[test]
    fn editor_changes_unicode_scalars_without_corrupting_utf8() {
        let mut input = InputBuffer::default();
        for character in "a🦀β".chars() {
            input.insert(character);
        }
        input.left();
        input.backspace();
        assert_eq!(input.text, "aβ");
        assert_eq!(input.cursor, 1);
        input.delete();
        input.insert('界');
        assert_eq!(input.text, "a界");
        assert!(input.text.is_char_boundary(input.byte_index()));
    }

    #[test]
    fn editor_keeps_query_history_without_stealing_result_arrows() {
        let mut input = InputBuffer {
            text: "first".to_string(),
            ..InputBuffer::default()
        };
        input.end();
        assert_eq!(input.commit().as_deref(), Some("first"));
        input.text = "second".to_string();
        input.end();
        assert_eq!(input.commit().as_deref(), Some("second"));
        input.older();
        assert_eq!(input.text, "second");
        input.older();
        assert_eq!(input.text, "first");
        input.newer();
        assert_eq!(input.text, "second");
    }

    #[test]
    fn fixture_parses_hello_events_and_skips_future_kinds_in_the_ui() {
        let mut decoder = EventDecoder::default();
        let records = decoder.push(FIXTURE.as_bytes()).expect("fixture parses");
        assert!(matches!(records.first(), Some(StreamRecord::Hello(_))));
        assert!(records.iter().any(|record| matches!(
            record,
            StreamRecord::Event(Event {
                kind: EventKind::Unknown,
                ..
            })
        )));

        let mut app = App::default();
        for record in records {
            match record {
                StreamRecord::Hello(hello) => {
                    app.apply(WorkerMessage::StreamConnected(hello.daemon));
                }
                StreamRecord::Event(event) => app.apply(WorkerMessage::StreamEvent(event)),
            }
        }
        assert!(app.stream_connected);
        assert_eq!(app.activity.len(), 2, "unknown and heartbeat add no filler");
        assert_eq!(app.queue[0].kind, "embed");
        assert_eq!(app.roots[0].files, 456);
    }

    #[test]
    fn disconnect_in_the_middle_of_a_line_is_named() {
        let mut decoder = EventDecoder::default();
        decoder
            .push(br#"{"stream":"fs3.events","v":1,"daemon":"0.4.0","heartbeat_ms":15000}\n{"#)
            .expect("hello parses");
        assert_eq!(
            decoder.finish().unwrap_err(),
            "event stream disconnected in the middle of a line"
        );
    }

    #[test]
    fn stale_snapshot_keeps_numbers_but_labels_them_stale() {
        let mut app = App::default();
        app.apply(WorkerMessage::Snapshot(Ok(StatusData {
            roots: vec![RootRow {
                identity: "git:example/repo".to_string(),
                root_path: "/repo".to_string(),
                files: 7,
            }],
            queue: Vec::new(),
        })));
        app.last_snapshot = Some(Instant::now() - Duration::from_secs(5));
        app.apply(WorkerMessage::Snapshot(Err(
            "daemon unavailable".to_string()
        )));
        assert_eq!(app.roots[0].files, 7);
        assert!(app.snapshot_badge().contains("STALE · 5s old"));
        assert!(app.snapshot_badge().contains("daemon unavailable"));
        assert_eq!(app.snapshot_pane_title("ROOTS"), " ROOTS · STALE · 5s old ");
    }

    #[test]
    fn successful_snapshot_clears_stale_state() {
        let mut app = App::default();
        app.apply(WorkerMessage::Snapshot(Err("offline".to_string())));
        app.apply(WorkerMessage::Snapshot(Ok(StatusData {
            roots: Vec::new(),
            queue: Vec::new(),
        })));
        assert!(app.snapshot_error.is_none());
        assert!(app.snapshot_badge().starts_with("LIVE"));
    }

    #[test]
    fn dropped_stream_is_visible_and_recovery_clears_it() {
        let mut app = App::default();
        app.apply(WorkerMessage::StreamDisconnected(
            "heartbeat timed out".to_string(),
        ));
        assert!(!app.stream_connected);
        assert_eq!(app.stream_error.as_deref(), Some("heartbeat timed out"));
        app.apply(WorkerMessage::StreamConnected("0.4.0".to_string()));
        assert!(app.stream_connected);
        assert!(app.stream_error.is_none());
    }

    #[test]
    fn heartbeats_and_unknown_events_never_invent_activity() {
        let mut app = App::default();
        app.apply_event(Event::new(
            "2026-08-28T03:11:20.000Z",
            EventKind::Heartbeat { seq: 1 },
        ));
        app.apply_event(Event::new("2026-08-28T03:11:21.000Z", EventKind::Unknown));
        assert!(app.activity.is_empty());

        app.apply_event(Event::new(
            "2026-08-28T03:11:22.000Z",
            EventKind::Queue {
                rows: vec![QueueDepth {
                    kind: "embed".to_string(),
                    state: "pending".to_string(),
                    count: 3,
                }],
            },
        ));
        assert!(
            app.activity.is_empty(),
            "queue snapshots are not feed filler"
        );
    }

    #[test]
    fn an_event_flood_cannot_delay_draw_or_quit_beyond_one_budget() {
        let messages = Mailbox::default();
        for sequence in 0..MESSAGE_CAPACITY * 10 {
            messages.push(WorkerMessage::StreamEvent(Event::new(
                "2026-08-28T03:11:05.001Z",
                EventKind::JobDone {
                    job: "scan_file".to_string(),
                    subject: format!("src/{sequence}.rs"),
                    ms: 1,
                    left: 0,
                },
            )));
        }
        assert_eq!(messages.len(), MESSAGE_CAPACITY);

        let mut app = App {
            focus: Focus::Roots,
            ..App::default()
        };
        assert_eq!(messages.apply_to(&mut app, MESSAGE_BUDGET), MESSAGE_BUDGET);
        assert_eq!(messages.len(), MESSAGE_CAPACITY - MESSAGE_BUDGET);

        let backend = ratatui::backend::TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).expect("headless terminal");
        terminal
            .draw(|frame| draw(frame, &mut app))
            .expect("one frame draws while messages remain queued");
        assert!(messages.len() > 0, "draw did not wait for an empty queue");

        let quit = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
        assert_eq!(app.handle_key(quit), Action::Quit);
    }
}
