use crate::{
    device,
    input::{self, Decoder, Layout},
};
use anyhow::{Context, Result};
use hidapi::{HidApi, HidDevice};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{File, OpenOptions},
    io::{self, Write},
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

struct OpenInterface {
    handle: HidDevice,
    decoder: Decoder,
    layout: Layout,
    number: i32,
}

pub struct Options<'a> {
    pub seconds: u64,
    pub interface: Option<i32>,
    pub output: Option<&'a Path>,
    pub label: Option<&'a str>,
    pub exclusive: bool,
}

struct Sink {
    file: Option<File>,
    start: Instant,
    label: Option<String>,
}
impl Sink {
    fn event(&mut self, kind: &str, details: Value) -> Result<()> {
        let event = json!({"schema_version":1,"elapsed_ms":self.start.elapsed().as_millis(),"event":kind,"label":self.label,"details":details});
        let line = serde_json::to_string(&event)?;
        writeln!(io::stdout().lock(), "{line}")?;
        if let Some(f) = &mut self.file {
            writeln!(f, "{line}")?;
            f.flush()?;
        }
        Ok(())
    }
}

pub fn run(mut api: HidApi, options: Options<'_>) -> Result<()> {
    #[cfg(target_os = "macos")]
    anyhow::ensure!(
        !options.exclusive || options.interface == Some(2) || crate::permissions::is_root(),
        "exclusive keyboard capture on macOS requires root. Close other device remappers, then run the built binary with sudo; shared monitoring does not require root"
    );
    #[cfg(not(target_os = "macos"))]
    anyhow::ensure!(
        !options.exclusive,
        "exclusive monitoring is currently implemented only on macOS"
    );
    let file = options
        .output
        .map(|p| {
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(p)
                .with_context(|| {
                    format!(
                        "cannot create {}; existing recordings are never overwritten",
                        p.display()
                    )
                })
        })
        .transpose()?;
    let running = Arc::new(AtomicBool::new(true));
    let signal = running.clone();
    ctrlc::set_handler(move || signal.store(false, Ordering::Relaxed))?;
    #[cfg(target_os = "macos")]
    api.set_open_exclusive(options.exclusive);
    let start = Instant::now();
    let mut sink = Sink {
        file,
        start,
        label: options.label.map(str::to_owned),
    };
    sink.event("session",json!({"vendor_id":device::VID,"supported_product_ids":[device::ORIGINAL_PID,device::CHROMA_PID],"started_unix_ms":SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis(),"exclusive":options.exclusive,"seconds":options.seconds}))?;
    eprintln!(
        "Monitoring Orbweaver only. Ctrl-C stops. {}",
        if options.exclusive {
            "Requesting exclusive access. Wait for capture_ready before pressing keys; input may still reach apps if access fails."
        } else {
            "Shared mode: factory keystrokes still reach the focused app."
        }
    );
    let mut open: BTreeMap<String, OpenInterface> = BTreeMap::new();
    let mut errors: BTreeMap<String, String> = BTreeMap::new();
    let mut refresh = Instant::now() - Duration::from_secs(2);
    let mut reports = 0u64;
    let mut opened_any = false;
    let mut ready_paths = BTreeSet::new();
    while running.load(Ordering::Relaxed)
        && (options.seconds == 0 || start.elapsed() < Duration::from_secs(options.seconds))
    {
        if refresh.elapsed() >= Duration::from_secs(1) {
            api.refresh_devices()?;
            let matched: Vec<_> = device::matching(&api)
                .into_iter()
                .filter(|d| options.interface.is_none_or(|n| d.interface_number() == n))
                .collect();
            let present: BTreeSet<_> = matched
                .iter()
                .map(|d| d.path().to_string_lossy().into_owned())
                .collect();
            let removed: Vec<_> = open
                .keys()
                .filter(|p| !present.contains(*p))
                .cloned()
                .collect();
            for path in removed {
                open.remove(&path);
                sink.event("disconnected", json!({"path":path,"state_cleared":true}))?;
                if options.exclusive {
                    open.clear();
                    sink.event(
                        "capture_failed",
                        json!({"reason":"interface disconnected; all handles released"}),
                    )?;
                    anyhow::bail!(
                        "exclusive capture ended on disconnect; reconnect the device and start a new session"
                    );
                }
            }
            errors.retain(|p, _| present.contains(p));
            for info in matched {
                let path = info.path().to_string_lossy().into_owned();
                if open.contains_key(&path) {
                    continue;
                }
                let attempt = (|| -> Result<OpenInterface> {
                    let handle = info.open_device(&api)?;
                    #[cfg(target_os = "macos")]
                    anyhow::ensure!(
                        !options.exclusive || handle.is_open_exclusive()?,
                        "exclusive handle was not granted"
                    );
                    let mut bytes = [0; hidapi::MAX_REPORT_DESCRIPTOR_SIZE];
                    let n = handle.get_report_descriptor(&mut bytes)?;
                    let layout = input::layout(&bytes[..n]);
                    Ok(OpenInterface {
                        handle,
                        decoder: Decoder::default(),
                        layout,
                        number: info.interface_number(),
                    })
                })();
                match attempt {
                    Ok(handle) => {
                        sink.event(
                            "connected",
                            json!({"path":path,"interface":handle.number,"layout":handle.layout}),
                        )?;
                        errors.remove(&path);
                        open.insert(path, handle);
                        opened_any = true;
                    }
                    Err(e) => {
                        let message = e.to_string();
                        if errors.get(&path) != Some(&message) {
                            sink.event("access_failed",json!({"path":path,"interface":info.interface_number(),"error":message,"hint":device::access_hint(&message)}))?;
                            errors.insert(path, message);
                        }
                    }
                }
            }
            if options.exclusive && !errors.is_empty() {
                open.clear();
                sink.event("capture_failed",json!({"reason":"required interface access failed; all handles released","interfaces_with_access_errors":errors.len()}))?;
                anyhow::bail!(
                    "exclusive capture unavailable; all handles released. Resolve access errors before pressing keys"
                );
            }
            if options.exclusive && options.interface.is_none() && !open.is_empty() {
                let numbers: BTreeSet<_> = open.values().map(|h| h.number).collect();
                if numbers != BTreeSet::from([0, 1, 2]) {
                    open.clear();
                    sink.event(
                        "capture_failed",
                        json!({"reason":"incomplete composite device; all handles released"}),
                    )?;
                    anyhow::bail!(
                        "expected all three Orbweaver interfaces before exclusive capture"
                    );
                }
            }
            let current_paths: BTreeSet<_> = open.keys().cloned().collect();
            if options.exclusive && !open.is_empty() && current_paths != ready_paths {
                sink.event(
                    "capture_ready",
                    json!({"interfaces":open.values().map(|h| h.number).collect::<Vec<_>>()}),
                )?;
                eprintln!(
                    "Capture ready: selected Orbweaver interfaces opened exclusively. Press and release the test controls now."
                );
            }
            ready_paths = current_paths;
            refresh = Instant::now();
        }
        let mut failed = Vec::new();
        for (path, source) in &mut open {
            // Bounded draining gives every interface and Ctrl-C time even under load.
            for _ in 0..64 {
                let mut bytes = [0; 256];
                match source.handle.read_timeout(&mut bytes, 0) {
                    Ok(0) => break,
                    Ok(n) => {
                        reports += 1;
                        let decoded = source.decoder.decode(source.layout, &bytes[..n]);
                        let (transitions, error) = match decoded {
                            Ok(t) => (t, None),
                            Err(e) => (vec![], Some(e.to_string())),
                        };
                        sink.event("report",json!({"path":path,"interface":source.number,"raw_hex":device::hex(&bytes[..n]),"transitions":transitions,"decode_error":error}))?;
                    }
                    Err(e) => {
                        source.decoder.reset();
                        sink.event(
                            "read_failed",
                            json!({"path":path,"error":e.to_string(),"state_cleared":true}),
                        )?;
                        failed.push(path.clone());
                        break;
                    }
                }
            }
        }
        for path in failed {
            open.remove(&path);
            if options.exclusive {
                open.clear();
                sink.event(
                    "capture_failed",
                    json!({"reason":"read failed; all handles released"}),
                )?;
                anyhow::bail!("exclusive capture ended after a read failure; all handles released");
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    // Drop seized handles before reporting completion.
    open.clear();
    sink.event("summary",json!({"reports":reports,"opened_any":opened_any,"interfaces_with_access_errors":errors.len(),"stopped_by_signal":!running.load(Ordering::Relaxed)}))?;
    anyhow::ensure!(
        opened_any,
        "no matching HID interface could be opened; inspect connection and access errors"
    );
    anyhow::ensure!(
        errors.is_empty(),
        "monitoring was incomplete: {} interface(s) could not be opened",
        errors.len()
    );
    Ok(())
}
