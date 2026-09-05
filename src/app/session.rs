//! Private, short-lived capture helper. It can only seize the two known Orbweaver keyboards.
use super::{engine::Engine, model::Profile, native::Emitter};
use crate::{
    device,
    input::{self, Decoder, Layout},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    io::{Read, Write},
    os::unix::{
        fs::PermissionsExt,
        net::{UnixListener, UnixStream},
    },
    path::Path,
    process::{Command, Stdio},
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::{Duration, Instant},
};
#[derive(Serialize, Deserialize)]
enum Wire {
    Ready,
    State(Vec<u16>),
    Error(String),
}
pub enum Event {
    Ready,
    Keys(Vec<u16>, usize),
    Learned(Vec<u16>),
    Ended(String),
}
pub enum Control {
    Stop,
    Profile(Box<Profile>),
    Learn(bool),
}
pub struct Session {
    pub tx: Sender<Control>,
    pub rx: Receiver<Event>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Session {
    pub fn start(profile: Profile) -> Self {
        let (tx, commands) = mpsc::channel();
        let (events, rx) = mpsc::channel();
        let worker = thread::spawn(move || {
            let result = client(profile, commands, &events);
            let message = match result {
                Ok(()) => "Remapping stopped. Factory input restored.".into(),
                Err(e) => format!("{e:#}"),
            };
            let _ = events.send(Event::Ended(message));
        });
        Self {
            tx,
            rx,
            worker: Some(worker),
        }
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.tx.send(Control::Stop);
        // Finish key-up delivery before the normal GUI process exits.
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
fn send(stream: &mut UnixStream, event: Wire) -> Result<()> {
    let mut line = serde_json::to_vec(&event)?;
    line.push(b'\n');
    stream.write_all(&line)?;
    Ok(())
}
struct Keyboard {
    handle: hidapi::HidDevice,
    decoder: Decoder,
    layout: Layout,
    keys: BTreeSet<u16>,
    path: String,
}
pub fn helper(path: &Path) -> Result<()> {
    ensure!(
        crate::permissions::is_root(),
        "capture helper needs administrator access"
    );
    let mut stream = UnixStream::connect(path).context("GUI connection unavailable")?;
    stream.set_nonblocking(true)?;
    let result = (|| -> Result<()> {
        let mut api = hidapi::HidApi::new()?;
        api.set_open_exclusive(true);
        let mut keyboards = vec![];
        for number in [0, 1] {
            let info = device::select(&api, number)?;
            let handle = info.open_device(&api)?;
            ensure!(
                handle.is_open_exclusive()?,
                "exclusive capture was not granted"
            );
            let mut descriptor = [0; 4096];
            let n = handle.get_report_descriptor(&mut descriptor)?;
            let layout = input::layout(&descriptor[..n]);
            ensure!(
                matches!(layout, Layout::BootKeyboard | Layout::ExtendedKeyboard),
                "unknown Orbweaver descriptor; refusing capture"
            );
            keyboards.push(Keyboard {
                handle,
                decoder: Decoder::default(),
                layout,
                keys: BTreeSet::new(),
                path: info.path().to_string_lossy().into_owned(),
            });
        }
        send(&mut stream, Wire::Ready)?;
        let mut heartbeat = Instant::now();
        let mut refresh = Instant::now();
        let mut previous = BTreeSet::new();
        loop {
            let mut control = [0; 64];
            match stream.read(&mut control) {
                Ok(0) => break,
                Ok(n) => {
                    if control[..n].contains(&b'X') {
                        break;
                    }
                    ensure!(control[..n].iter().all(|b| *b == b'H'), "invalid heartbeat");
                    heartbeat = Instant::now();
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(e) => return Err(e.into()),
            }
            ensure!(
                heartbeat.elapsed() < Duration::from_secs(3),
                "GUI heartbeat expired; releasing keypad"
            );
            if refresh.elapsed() > Duration::from_secs(1) {
                api.refresh_devices()?;
                for k in &keyboards {
                    ensure!(
                        device::matching(&api)
                            .iter()
                            .any(|d| d.path().to_string_lossy() == k.path),
                        "Orbweaver disconnected"
                    );
                }
                refresh = Instant::now();
            }
            for k in &mut keyboards {
                let mut data = [0; 64];
                let n = k.handle.read_timeout(&mut data, 0)?;
                if n > 0 {
                    for t in k.decoder.decode(k.layout, &data[..n])? {
                        if t.usage_page == 7 {
                            if t.pressed {
                                k.keys.insert(t.usage);
                            } else {
                                k.keys.remove(&t.usage);
                            }
                        }
                    }
                }
            }
            let keys: BTreeSet<_> = keyboards
                .iter()
                .flat_map(|k| k.keys.iter().copied())
                .collect();
            if keys != previous {
                send(&mut stream, Wire::State(keys.iter().copied().collect()))?;
                previous = keys;
            }
            thread::sleep(Duration::from_millis(2));
        }
        Ok(())
    })();
    if let Err(e) = &result {
        let _ = send(&mut stream, Wire::Error(format!("{e:#}")));
    }
    result
}
fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}
fn client(profile: Profile, commands: Receiver<Control>, events: &Sender<Event>) -> Result<()> {
    ensure!(
        super::native::trusted(),
        "Enable Orbweaver in System Settings → Privacy & Security → Accessibility, then start again."
    );
    let dir = tempfile::Builder::new().prefix("orbweaver-").tempdir()?;
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700))?;
    let path = dir.path().join("capture.sock");
    let listener = UnixListener::bind(&path)?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    listener.set_nonblocking(true)?;
    let exe = std::env::current_exe()?;
    let no_prompt = Command::new("/usr/bin/sudo")
        .args(["-n", "true"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?
        .success();
    let mut child = if no_prompt {
        Command::new("/usr/bin/sudo")
            .arg("-n")
            .arg(&exe)
            .arg("capture-helper")
            .arg("--socket")
            .arg(&path)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?
    } else {
        let shell = format!(
            "{} capture-helper --socket {}",
            quote(&exe.to_string_lossy()),
            quote(&path.to_string_lossy())
        );
        let script = format!(
            "do shell script {} with administrator privileges",
            serde_json::to_string(&shell)?
        );
        Command::new("/usr/bin/osascript")
            .arg("-e")
            .arg(script)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?
    };
    let started = Instant::now();
    let mut engine = Engine::new(profile);
    let mut emitter = Emitter::new()?;
    let mut stream = loop {
        match commands.try_recv() {
            Ok(Control::Stop) | Err(mpsc::TryRecvError::Disconnected) => return Ok(()),
            Ok(Control::Profile(p)) => engine = Engine::new(*p),
            _ => {}
        }
        match listener.accept() {
            Ok((s, _)) => break s,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) => return Err(e.into()),
        }
        if let Some(status) = child.try_wait()? {
            anyhow::bail!(
                "Capture helper did not start ({status}). Administrator access is required to suppress the Orbweaver's factory keystrokes."
            );
        }
        ensure!(
            started.elapsed() < Duration::from_secs(120),
            "Timed out waiting for administrator access"
        );
        thread::sleep(Duration::from_millis(50));
    };
    stream.set_nonblocking(true)?;
    let mut buffer = vec![];
    let mut heartbeat = Instant::now() - Duration::from_secs(1);
    let mut last_message = Instant::now();
    let mut ready = false;
    let mut learn = false;
    let mut learned = BTreeSet::new();
    let mut suppress = false;
    let mut latest_raw = BTreeSet::new();
    let mut pending: Option<(Instant, BTreeSet<u16>)> = None;
    let result = (|| -> Result<()> {
        loop {
            while let Ok(command) = commands.try_recv() {
                match command {
                    Control::Stop => return Ok(()),
                    Control::Profile(p) => {
                        emitter.emit(engine.reset())?;
                        engine = Engine::new(*p);
                        suppress = !latest_raw.is_empty();
                        pending = None;
                    }
                    Control::Learn(on) => {
                        emitter.emit(engine.reset())?;
                        learn = on;
                        learned.clear();
                        suppress = !latest_raw.is_empty();
                        pending = None;
                    }
                }
            }
            if heartbeat.elapsed() > Duration::from_millis(500) {
                stream.write_all(b"H")?;
                heartbeat = Instant::now();
            }
            let mut bytes = [0; 4096];
            loop {
                match stream.read(&mut bytes) {
                    Ok(0) => anyhow::bail!("Capture connection closed; factory input restored."),
                    Ok(n) => {
                        buffer.extend_from_slice(&bytes[..n]);
                        ensure!(buffer.len() < 65536, "invalid helper message");
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                    Err(e) => return Err(e.into()),
                }
            }
            while let Some(end) = buffer.iter().position(|b| *b == b'\n') {
                let line: Vec<_> = buffer.drain(..=end).collect();
                let wire: Wire = serde_json::from_slice(&line)?;
                last_message = Instant::now();
                match wire {
                    Wire::Ready => {
                        ready = true;
                        events.send(Event::Ready)?;
                    }
                    Wire::Error(e) => anyhow::bail!(e),
                    Wire::State(keys) => {
                        let state: BTreeSet<_> = keys.iter().copied().collect();
                        latest_raw = state.clone();
                        events.send(Event::Keys(keys, engine.map()))?;
                        if learn {
                            if state.len() > learned.len() {
                                learned = state.clone();
                            }
                            if state.is_empty() && !learned.is_empty() {
                                events.send(Event::Learned(learned.iter().copied().collect()))?;
                                learn = false;
                                learned.clear();
                                suppress = false;
                            }
                            continue;
                        }
                        if suppress {
                            if state.is_empty() {
                                suppress = false;
                            }
                            continue;
                        }
                        let arrow_changed = state
                            .symmetric_difference(&engine.raw)
                            .any(|u| (79..=82).contains(u));
                        if arrow_changed {
                            pending = Some((Instant::now() + Duration::from_millis(25), state));
                        } else {
                            pending = None;
                            emitter
                                .emit(engine.state(state, started.elapsed().as_millis() as u64))?;
                        }
                    }
                }
            }
            if pending
                .as_ref()
                .is_some_and(|(due, _)| Instant::now() >= *due)
            {
                let (_, state) = pending.take().unwrap();
                emitter.emit(engine.state(state, started.elapsed().as_millis() as u64))?;
                events.send(Event::Keys(
                    engine.raw.iter().copied().collect(),
                    engine.map(),
                ))?;
            }
            emitter.emit(engine.tick(started.elapsed().as_millis() as u64))?;
            emitter.tick()?;
            ensure!(
                ready || last_message.elapsed() < Duration::from_secs(10),
                "Helper did not acquire the Orbweaver"
            );
            thread::sleep(Duration::from_millis(2));
        }
    })();
    let _ = stream.write_all(b"X");
    emitter.release_all();
    drop(stream);
    // The helper exits on socket close. Reap when possible without holding the GUI.
    thread::spawn(move || {
        let _ = child.wait();
    });
    result
}
