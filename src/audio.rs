use anyhow::{bail, Context, Result};
use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    thread,
    time::{Duration, Instant},
};
use wasapi::*;

#[derive(Clone)]
pub struct Endpoint {
    pub id: String,
    pub name: String,
}
pub struct Devices {
    pub endpoints: Vec<Endpoint>,
    pub default: String,
}

// COM objects are created, used and released on their owning worker thread.
struct Com;
impl Com {
    fn new() -> Result<Self> {
        initialize_mta().ok()?;
        Ok(Self)
    }
}
impl Drop for Com {
    fn drop(&mut self) {
        deinitialize();
    }
}

pub fn devices() -> Result<Devices> {
    thread::spawn(|| -> Result<Devices> {
        let _com = Com::new()?;
        let e = DeviceEnumerator::new()?;
        let default = e.get_default_device(Direction::Render)?.get_id()?;
        let collection = e.get_device_collection(Direction::Render)?;
        let mut endpoints = Vec::new();
        for i in 0..collection.get_nbr_devices()? {
            let d = collection.get_device_at_index(i)?;
            endpoints.push(Endpoint {
                id: d.get_id()?,
                name: d.get_friendlyname()?,
            });
        }
        Ok(Devices { endpoints, default })
    })
    .join()
    .map_err(|_| anyhow::anyhow!("Falha ao listar dispositivos"))?
}

pub enum Event {
    Started,
    Failed(String),
}
pub struct Engine {
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
    pub events: mpsc::Receiver<Event>,
}
impl Engine {
    pub fn start(source: String, targets: Vec<String>, wake: eframe::egui::Context) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let (tx, events) = mpsc::channel();
        let worker = thread::spawn(move || {
            if let Err(err) = run(&source, &targets, &flag, &tx, &wake) {
                let _ = tx.send(Event::Failed(format!("{err:#}")));
                wake.request_repaint();
            }
        });
        Self {
            stop,
            worker: Some(worker),
            events,
        }
    }
}
impl Drop for Engine {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

// Stereo frames at 48 kHz. A separate queue and fractional cursor per output
// compensate small differences between independent hardware clocks.
struct Queue {
    frames: VecDeque<[f32; 2]>,
    cursor: f64,
    primed: bool,
}
impl Queue {
    fn new() -> Self {
        Self {
            frames: VecDeque::new(),
            cursor: 0.0,
            primed: false,
        }
    }
    fn push(&mut self, bytes: &[u8]) {
        for f in bytes.chunks_exact(8) {
            self.frames.push_back([
                f32::from_le_bytes(f[0..4].try_into().unwrap()),
                f32::from_le_bytes(f[4..8].try_into().unwrap()),
            ]);
        }
        // Hard limit: 160 ms. Discard old audio if a driver stalls.
        if self.frames.len() > 7680 {
            let excess = self.frames.len() - 1920;
            self.frames.drain(..excess);
            self.cursor = 0.0;
        }
    }
    fn render(&mut self, count: usize) -> Vec<u8> {
        let mut bytes = vec![0; count * 8];
        if !self.primed {
            if self.frames.len() < 1920 {
                return bytes;
            }
            self.primed = true;
        }
        let step = (1.0 + (self.frames.len() as f64 - 1920.0) / 480_000.0).clamp(0.997, 1.003);
        for out in bytes.chunks_exact_mut(8) {
            if self.frames.len() < 2 {
                self.frames.clear();
                self.cursor = 0.0;
                self.primed = false;
                break;
            }
            let a = self.frames[0];
            let b = self.frames[1];
            for channel in 0..2 {
                let v = a[channel] + (b[channel] - a[channel]) * self.cursor as f32;
                out[channel * 4..channel * 4 + 4].copy_from_slice(&v.to_le_bytes());
            }
            self.cursor += step;
            while self.cursor >= 1.0 {
                self.frames.pop_front();
                self.cursor -= 1.0;
            }
        }
        bytes
    }
}

struct Output {
    client: AudioClient,
    render: AudioRenderClient,
    queue: Queue,
    name: String,
}
// Ensures streams stop before their clients and COM apartment are released.
struct Streams {
    capture: AudioClient,
    outputs: Vec<Output>,
}
impl Drop for Streams {
    fn drop(&mut self) {
        let _ = self.capture.stop_stream();
        for output in &self.outputs {
            let _ = output.client.stop_stream();
        }
    }
}
fn run(
    source: &str,
    targets: &[String],
    stop: &AtomicBool,
    tx: &mpsc::Sender<Event>,
    wake: &eframe::egui::Context,
) -> Result<()> {
    if targets.is_empty() {
        bail!("Selecione pelo menos uma saída adicional.");
    }
    if targets.iter().any(|id| id == source) {
        bail!("A origem não pode ser uma saída adicional.");
    }
    let _com = Com::new()?;
    let enumerator = DeviceEnumerator::new()?;
    let initial_default = enumerator.get_default_device(Direction::Render)?.get_id()?;
    let format = WaveFormat::new(32, 32, SampleType::Float, 48000, 2, None);
    let mode = StreamMode::PollingShared {
        autoconvert: true,
        buffer_duration_hns: 400_000,
    };
    let mut capture = enumerator.get_device(source)?.get_iaudioclient()?;
    capture
        .initialize_client(&format, Direction::Capture, mode, None)
        .context("Não foi possível capturar o áudio da origem")?;
    let reader = capture.get_audiocaptureclient()?;
    let mut streams = Streams {
        capture,
        outputs: Vec::new(),
    };
    for id in targets {
        let device = enumerator.get_device(id)?;
        let name = device.get_friendlyname()?;
        let mut client = device.get_iaudioclient()?;
        client
            .initialize_client(&format, Direction::Render, mode, None)
            .with_context(|| format!("Não foi possível abrir {name}"))?;
        let render = client.get_audiorenderclient()?;
        streams.outputs.push(Output {
            client,
            render,
            queue: Queue::new(),
            name,
        });
    }
    for output in &streams.outputs {
        output.client.start_stream()?;
    }
    streams.capture.start_stream()?;
    let _ = tx.send(Event::Started);
    wake.request_repaint();
    let mut check = Instant::now();
    let mut bytes = Vec::new();
    while !stop.load(Ordering::Relaxed) {
        // Bounded work per pass keeps stop responsive, even under driver faults.
        for _ in 0..64 {
            let count = reader.get_next_packet_size()?.unwrap_or(0) as usize;
            if count == 0 {
                break;
            }
            bytes.resize(count * 8, 0);
            let (frames, _) = reader.read_from_device(&mut bytes)?;
            for output in &mut streams.outputs {
                output.queue.push(&bytes[..frames as usize * 8]);
            }
        }
        for output in &mut streams.outputs {
            let frames = output
                .client
                .get_available_space_in_frames()
                .with_context(|| format!("Dispositivo desconectado: {}", output.name))?
                as usize;
            if frames > 0 {
                output
                    .render
                    .write_to_device(&output.queue.render(frames), None)?;
            }
        }
        if check.elapsed() >= Duration::from_secs(1) {
            if enumerator.get_default_device(Direction::Render)?.get_id()? != initial_default {
                bail!("A saída padrão do Windows mudou. Confira a origem e ative novamente.");
            }
            check = Instant::now();
        }
        thread::sleep(Duration::from_millis(3));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unprimed_queue_is_silent() {
        assert_eq!(Queue::new().render(480), vec![0; 3840]);
    }
    #[test]
    fn stereo_and_bounds_are_preserved() {
        let mut q = Queue::new();
        let frame = [0.25f32.to_le_bytes(), (-0.5f32).to_le_bytes()].concat();
        q.push(&frame.repeat(2400));
        let out = q.render(480);
        assert!(out.chunks_exact(8).all(|f| f == frame));
        q.push(&frame.repeat(20000));
        assert_eq!(q.frames.len(), 1920);
    }
    #[test]
    fn underflow_reprimes_without_old_audio() {
        let mut q = Queue::new();
        q.push(&[0; 1920 * 8]);
        q.render(2400);
        assert!(!q.primed);
        assert!(q.frames.is_empty());
    }
}
