//! The microphone as a take's source (N-554 step 3): the default input through WASAPI (cpal), at whatever rate and
//! channels Windows gives it; the take makes it 16 kHz mono. Opened on the take's own thread when his press starts a
//! take, closed when the take ends (the stream dropped). Its sound goes only to the take, never to the disk.

use super::Source;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::time::Duration;

pub struct Mic {
    rate: u32,
    channels: u16,
    sound: Receiver<Vec<f32>>,
    failed: Receiver<String>,
    /// held for as long as the take runs; dropping it closes the device
    _stream: cpal::Stream,
}

/// What the owner reads when Windows refuses the microphone, or none is there.
fn said(e: &cpal::Error) -> String {
    match e.kind() {
        cpal::ErrorKind::PermissionDenied => "Microphone blocked: allow it in Windows privacy settings.".into(),
        cpal::ErrorKind::DeviceNotAvailable => "The microphone is not there any more.".into(),
        cpal::ErrorKind::DeviceBusy => "The microphone is in use by another program.".into(),
        _ => format!("The microphone could not be opened: {e}."),
    }
}

impl Mic {
    /// The default input, started.
    pub fn open() -> Result<Mic, String> {
        let host = cpal::default_host();
        let device = host.default_input_device().ok_or("No microphone was found.")?;
        let supported = device.default_input_config().map_err(|e| said(&e))?;
        let (send, sound) = channel();
        let (fail, failed) = channel();
        let config = supported.config();
        let stream = match supported.sample_format() {
            SampleFormat::F32 => build::<f32>(&device, config.clone(), send, fail),
            SampleFormat::I16 => build::<i16>(&device, config.clone(), send, fail),
            SampleFormat::I32 => build::<i32>(&device, config.clone(), send, fail),
            SampleFormat::U16 => build::<u16>(&device, config.clone(), send, fail),
            other => return Err(format!("Unsupported microphone format: {other:?}")),
        }
        .map_err(|e| said(&e))?;
        stream.play().map_err(|e| said(&e))?;
        Ok(Mic { rate: config.sample_rate, channels: config.channels, sound, failed, _stream: stream })
    }
}

fn build<T>(device: &cpal::Device, config: cpal::StreamConfig, send: Sender<Vec<f32>>, fail: Sender<String>) -> Result<cpal::Stream, cpal::Error>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    device.build_input_stream::<T, _, _>(
        config,
        move |data: &[T], _| {
            let _ = send.send(data.iter().map(|s| f32::from_sample_(*s)).collect());
        },
        move |e| {
            let _ = fail.send(said(&e));
        },
        Some(Duration::from_secs(5)),
    )
}

impl Source for Mic {
    fn rate(&self) -> u32 {
        self.rate
    }
    fn channels(&self) -> u16 {
        self.channels
    }
    fn queued(&mut self) -> Vec<Vec<f32>> {
        self.sound.try_iter().collect()
    }

    fn next(&mut self, wait: Duration) -> Result<Option<Vec<f32>>, String> {
        if let Ok(why) = self.failed.try_recv() {
            return Err(why);
        }
        match self.sound.recv_timeout(wait) {
            Ok(p) => Ok(Some(p)),
            Err(RecvTimeoutError::Timeout) => Ok(None),
            Err(RecvTimeoutError::Disconnected) => Err("The microphone stopped.".into()),
        }
    }
}
