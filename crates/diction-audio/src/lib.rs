//! Capture du micro. Le flux cpal n'est pas Send sous macOS : il vit entièrement dans le thread `run`.

use std::fmt;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use diction_core::erreur::{Erreur, log};

pub enum Cmd {
    /// Démarre avec le micro nommé (vide = micro par défaut).
    Start(String),
    Stop,
}

pub struct Handlers {
    pub level: Box<dyn Fn(f32) + Send>,
    pub done: Box<dyn Fn(Vec<i16>, u32) + Send>,
    pub error: Box<dyn Fn(Erreur) + Send>,
}

struct Capture {
    _stream: cpal::Stream,
    buf: Arc<Mutex<Vec<i16>>>,
    rate: u32,
    level: Arc<AtomicU32>,
}

/// WAV PCM 16 bits mono.
pub fn wav(samples: &[i16], rate: u32) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut w = Vec::with_capacity(44 + data_len as usize);
    w.extend_from_slice(b"RIFF");
    w.extend_from_slice(&(36 + data_len).to_le_bytes());
    w.extend_from_slice(b"WAVEfmt ");
    w.extend_from_slice(&16u32.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes()); // PCM
    w.extend_from_slice(&1u16.to_le_bytes()); // mono
    w.extend_from_slice(&rate.to_le_bytes());
    w.extend_from_slice(&(rate * 2).to_le_bytes());
    w.extend_from_slice(&2u16.to_le_bytes());
    w.extend_from_slice(&16u16.to_le_bytes());
    w.extend_from_slice(b"data");
    w.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        w.extend_from_slice(&s.to_le_bytes());
    }
    w
}

/// Noms des micros disponibles.
pub fn input_names() -> Vec<String> {
    let Ok(devices) = cpal::default_host().input_devices() else {
        return Vec::new();
    };
    devices
        .filter_map(|d| d.description().ok().map(|desc| desc.name().to_string()))
        .collect()
}

fn pick_device(mic: &str) -> Result<cpal::Device, Erreur> {
    let host = cpal::default_host();
    if !mic.is_empty()
        && let Ok(mut devices) = host.input_devices()
        && let Some(d) = devices.find(|d| d.description().is_ok_and(|desc| desc.name() == mic))
    {
        return Ok(d);
    }
    // Micro introuvable (débranché) : repli sur le micro par défaut, signalé dans le journal.
    if !mic.is_empty() {
        log(&format!(
            "micro « {mic} » introuvable, micro par défaut utilisé"
        ));
    }
    host.default_input_device()
        .ok_or_else(|| Erreur::Micro("aucun micro".into()))
}

fn open_stream(mic_name: &str) -> Result<Capture, Erreur> {
    let micro = |e: &dyn fmt::Display| Erreur::Micro(e.to_string());
    let device = pick_device(mic_name)?;
    let config = device.default_input_config().map_err(|e| micro(&e))?;
    if config.sample_format() != cpal::SampleFormat::F32 {
        return Err(Erreur::Micro(format!(
            "format {:?} non géré",
            config.sample_format()
        )));
    }
    let (channels, rate) = (config.channels() as usize, config.sample_rate());
    let buf = Arc::new(Mutex::new(Vec::new()));
    let level = Arc::new(AtomicU32::new(0));
    let (sink, lvl) = (buf.clone(), level.clone());
    let stream = device
        .build_input_stream(
            config.into(),
            move |data: &[f32], _: &_| {
                let rms =
                    (data.iter().map(|s| s * s).sum::<f32>() / data.len().max(1) as f32).sqrt();
                lvl.store(rms.to_bits(), Ordering::Relaxed);
                if let Ok(mut b) = sink.lock() {
                    b.extend(data.chunks(channels).map(|f| {
                        let m = f.iter().sum::<f32>() / channels as f32;
                        (m.clamp(-1.0, 1.0) * i16::MAX as f32) as i16
                    }));
                }
            },
            |e| log(&format!("flux micro : {e}")),
            None,
        )
        .map_err(|e| micro(&e))?;
    stream.play().map_err(|e| micro(&e))?;
    Ok(Capture {
        _stream: stream,
        buf,
        rate,
        level,
    })
}

/// Ouvre le micro 200 ms pour vérifier l'accès (déclenche la demande d'autorisation macOS).
pub fn probe(mic: &str) -> Result<(), Erreur> {
    let c = open_stream(mic)?;
    std::thread::sleep(Duration::from_millis(200));
    drop(c);
    Ok(())
}

pub fn run(rx: mpsc::Receiver<Cmd>, h: Handlers) {
    let mut cur: Option<Capture> = None;
    loop {
        match rx.recv_timeout(Duration::from_millis(33)) {
            Ok(Cmd::Start(mic)) => match open_stream(&mic) {
                Ok(c) => cur = Some(c),
                Err(e) => (h.error)(e),
            },
            Ok(Cmd::Stop) => {
                if let Some(c) = cur.take() {
                    let rate = c.rate;
                    let samples = c.buf.lock().map(|b| b.clone()).unwrap_or_default();
                    drop(c);
                    (h.done)(samples, rate);
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                if let Some(c) = &cur {
                    (h.level)(f32::from_bits(c.level.load(Ordering::Relaxed)));
                }
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_entete_correct() {
        let w = wav(&[1, -1, 2], 16000);
        assert_eq!(&w[0..4], b"RIFF");
        assert_eq!(w.len(), 44 + 6);
        assert_eq!(u32::from_le_bytes(w[24..28].try_into().unwrap()), 16000);
        assert_eq!(u32::from_le_bytes(w[40..44].try_into().unwrap()), 6);
    }
}
