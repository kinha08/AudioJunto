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
    pub default: Option<String>,
    pub microphones: Vec<Endpoint>,
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
        let default = e
            .get_default_device(Direction::Render)
            .ok()
            .map(|d| d.get_id())
            .transpose()?;
        let enumerate = |direction| -> Result<Vec<Endpoint>> {
            let collection = e.get_device_collection(direction)?;
            (0..collection.get_nbr_devices()?)
                .map(|i| {
                    let device = collection.get_device_at_index(i)?;
                    Ok(Endpoint {
                        id: device.get_id()?,
                        name: device.get_friendlyname()?,
                    })
                })
                .collect()
        };
        Ok(Devices {
            endpoints: enumerate(Direction::Render)?,
            default,
            microphones: enumerate(Direction::Capture)?,
        })
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
    pub fn start(
        source: Option<String>,
        microphones: Vec<String>,
        targets: Vec<String>,
        wake: eframe::egui::Context,
    ) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let (tx, events) = mpsc::channel();
        let worker = thread::spawn(move || {
            if let Err(err) = run(source.as_deref(), &microphones, &targets, &flag, &tx, &wake) {
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

// Each capture/output pair has its own queue and fractional cursor to
// compensate for differences between the two hardware clocks.
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
    fn mix_into(&mut self, mix: &mut [[f32; 2]]) {
        if !self.primed {
            if self.frames.len() < 1920 {
                return;
            }
            self.primed = true;
        }
        let step = (1.0 + (self.frames.len() as f64 - 1920.0) / 480_000.0).clamp(0.997, 1.003);
        for out in mix {
            if self.frames.len() < 2 {
                self.frames.clear();
                self.cursor = 0.0;
                self.primed = false;
                break;
            }
            let a = self.frames[0];
            let b = self.frames[1];
            for channel in 0..2 {
                let sample = a[channel] + (b[channel] - a[channel]) * self.cursor as f32;
                if sample.is_finite() {
                    out[channel] += sample;
                }
            }
            self.cursor += step;
            while self.cursor >= 1.0 {
                self.frames.pop_front();
                self.cursor -= 1.0;
            }
        }
    }
}

struct Mixer {
    inputs: Vec<Queue>,
    limit: bool,
    gain: f32,
    frames: Vec<[f32; 2]>,
    bytes: Vec<u8>,
}

impl Mixer {
    fn new(input_count: usize, limit: bool) -> Self {
        Self {
            inputs: (0..input_count).map(|_| Queue::new()).collect(),
            limit,
            gain: 1.0,
            frames: Vec::new(),
            bytes: Vec::new(),
        }
    }

    fn render(&mut self, count: usize) -> &[u8] {
        self.frames.resize(count, [0.0; 2]);
        self.frames.fill([0.0; 2]);
        for input in &mut self.inputs {
            input.mix_into(&mut self.frames);
        }
        self.bytes.clear();
        for frame in &self.frames {
            if self.limit {
                // Fast attack and gradual release (roughly 100 ms at 48 kHz).
                // Use a common gain for both channels to preserve stereo balance.
                let peak = frame[0].abs().max(frame[1].abs());
                let ceiling = 0.95 / peak.max(0.95);
                self.gain = (self.gain + (1.0 - self.gain) / 4800.0).min(ceiling);
            }
            for sample in frame {
                self.bytes
                    .extend_from_slice(&(sample * self.gain).to_le_bytes());
            }
        }
        &self.bytes
    }
}

struct Capture {
    client: AudioClient,
    reader: AudioCaptureClient,
    name: String,
}

struct Output {
    client: AudioClient,
    render: AudioRenderClient,
    mixer: Mixer,
    name: String,
}
// Ensures streams stop before their clients and COM apartment are released.
struct Streams {
    captures: Vec<Capture>,
    outputs: Vec<Output>,
}
impl Drop for Streams {
    fn drop(&mut self) {
        for capture in &self.captures {
            let _ = capture.client.stop_stream();
        }
        for output in &self.outputs {
            let _ = output.client.stop_stream();
        }
    }
}
fn run(
    source: Option<&str>,
    microphones: &[String],
    targets: &[String],
    stop: &AtomicBool,
    tx: &mpsc::Sender<Event>,
    wake: &eframe::egui::Context,
) -> Result<()> {
    if targets.is_empty() {
        bail!("Selecione pelo menos uma saída.");
    }
    if source.is_none() && microphones.is_empty() {
        bail!("Selecione pelo menos um microfone ou ative a duplicação de áudio.");
    }
    if targets.iter().any(|id| Some(id.as_str()) == source) {
        bail!("A origem não pode ser uma saída adicional.");
    }
    let _com = Com::new()?;
    let enumerator = DeviceEnumerator::new()?;
    let initial_default = source
        .map(|_| enumerator.get_default_device(Direction::Render)?.get_id())
        .transpose()?;
    let format = WaveFormat::new(32, 32, SampleType::Float, 48000, 2, None);
    let mode = StreamMode::PollingShared {
        autoconvert: true,
        buffer_duration_hns: 400_000,
    };
    let mut streams = Streams {
        captures: Vec::new(),
        outputs: Vec::new(),
    };
    for id in source
        .into_iter()
        .chain(microphones.iter().map(String::as_str))
    {
        let device = enumerator.get_device(id)?;
        let name = device.get_friendlyname()?;
        let mut client = device.get_iaudioclient()?;
        client
            .initialize_client(&format, Direction::Capture, mode, None)
            .with_context(|| format!("Não foi possível capturar {name}"))?;
        let reader = client.get_audiocaptureclient()?;
        streams.captures.push(Capture {
            client,
            reader,
            name,
        });
    }
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
            mixer: Mixer::new(streams.captures.len(), !microphones.is_empty()),
            name,
        });
    }
    for output in &streams.outputs {
        output.client.start_stream()?;
    }
    for capture in &streams.captures {
        capture
            .client
            .start_stream()
            .with_context(|| format!("Não foi possível iniciar {}", capture.name))?;
    }
    let _ = tx.send(Event::Started);
    wake.request_repaint();
    let mut check = Instant::now();
    let mut bytes = Vec::new();
    while !stop.load(Ordering::Relaxed) {
        // Bounded work per pass keeps stop responsive, even under driver faults.
        for (index, capture) in streams.captures.iter().enumerate() {
            for _ in 0..64 {
                let count = capture
                    .reader
                    .get_next_packet_size()
                    .with_context(|| format!("Falha na captura de {}", capture.name))?
                    .unwrap_or(0) as usize;
                if count == 0 {
                    break;
                }
                bytes.resize(count * 8, 0);
                let (frames, _) = capture
                    .reader
                    .read_from_device(&mut bytes)
                    .with_context(|| format!("Falha na captura de {}", capture.name))?;
                for output in &mut streams.outputs {
                    output.mixer.inputs[index].push(&bytes[..frames as usize * 8]);
                }
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
                    .write_to_device(output.mixer.render(frames), None)?;
            }
        }
        if let Some(initial_default) = &initial_default {
            if check.elapsed() >= Duration::from_secs(1) {
                if enumerator.get_default_device(Direction::Render)?.get_id()? != *initial_default {
                    bail!("A saída padrão do Windows mudou. Confira a origem e ative novamente.");
                }
                check = Instant::now();
            }
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
        assert_eq!(Mixer::new(1, false).render(480), vec![0; 3840]);
    }
    #[test]
    fn stereo_and_bounds_are_preserved() {
        let mut mixer = Mixer::new(1, false);
        let frame = [0.25f32.to_le_bytes(), (-0.5f32).to_le_bytes()].concat();
        mixer.inputs[0].push(&frame.repeat(2400));
        let out = mixer.render(480);
        assert!(out.chunks_exact(8).all(|f| f == frame));
        let q = &mut mixer.inputs[0];
        q.push(&frame.repeat(20000));
        assert_eq!(q.frames.len(), 1920);
    }
    #[test]
    fn underflow_reprimes_without_old_audio() {
        let mut q = Queue::new();
        q.push(&[0; 1920 * 8]);
        q.mix_into(&mut [[0.0; 2]; 2400]);
        assert!(!q.primed);
        assert!(q.frames.is_empty());
    }

    #[test]
    fn simultaneous_inputs_sum_and_limit_together() {
        let mut mixer = Mixer::new(3, true);
        for (input, (left, right)) in
            mixer
                .inputs
                .iter_mut()
                .zip([(0.6f32, 0.1f32), (0.6, 0.2), (0.1, 0.1)])
        {
            input.push(
                &[left.to_le_bytes(), right.to_le_bytes()]
                    .concat()
                    .repeat(2400),
            );
        }
        for frame in mixer.render(480).chunks_exact(8) {
            let left = f32::from_le_bytes(frame[..4].try_into().unwrap());
            let right = f32::from_le_bytes(frame[4..].try_into().unwrap());
            assert!((left - 0.95).abs() < 1e-6);
            assert!((right - (0.4 * 0.95 / 1.3)).abs() < 1e-6);
        }
    }

    #[test]
    fn an_unprimed_input_does_not_silence_the_others() {
        let mut mixer = Mixer::new(2, true);
        let frame = [0.3f32.to_le_bytes(), 0.3f32.to_le_bytes()].concat();
        mixer.inputs[1].push(&frame.repeat(2400));
        assert!(mixer.render(480).chunks_exact(8).all(|f| f == frame));
    }
}
