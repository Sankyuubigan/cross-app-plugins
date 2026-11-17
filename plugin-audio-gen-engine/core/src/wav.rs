//! WAV-постобработка: декод → нормализация → луп-кроссфейд → кодек i16 PCM.
//!
//! Использует hound (уже в workspace-депах проекта). Все сэмплы внутри — f32
//! интерливом; на выходе 16-bit PCM WAV с исходными sr/каналами.

use std::io::Cursor;

use hound::{SampleFormat, WavReader, WavWriter, WavSpec};

/// Декодированный WAV: интерливнутые сэмплы f32.
#[derive(Debug, Clone)]
pub struct WavData {
    pub samples: Vec<f32>,
    pub channels: u16,
    pub sample_rate: u32,
}

impl WavData {
    /// Длительность в секундах.
    pub fn duration_secs(&self) -> f32 {
        let frames = self.samples.len() / self.channels.max(1) as usize;
        frames as f32 / self.sample_rate.max(1) as f32
    }
}

/// Декод WAV-байтов (16/24/32-bit int и 32-bit float) в f32.
pub fn decode(bytes: &[u8]) -> Result<WavData, String> {
    let reader = WavReader::new(Cursor::new(bytes))
        .map_err(|e| format!("не WAV или битый заголовок: {e}"))?;
    let spec = reader.spec();
    let channels = spec.channels;
    let sample_rate = spec.sample_rate;
    let mut samples = Vec::with_capacity(reader.duration() as usize * channels as usize);
    match (spec.sample_format, spec.bits_per_sample) {
        (SampleFormat::Float, _) => {
            for s in reader.into_samples::<f32>() {
                samples.push(s.map_err(|e| e.to_string())?);
            }
        }
        (SampleFormat::Int, 16) => {
            for s in reader.into_samples::<i16>() {
                samples.push(s.map_err(|e| e.to_string())? as f32 / 32768.0);
            }
        }
        (SampleFormat::Int, 32) => {
            for s in reader.into_samples::<i32>() {
                samples.push(s.map_err(|e| e.to_string())? as f32 / 2147483648.0);
            }
        }
        (SampleFormat::Int, 24) => {
            for s in reader.into_samples::<i32>() {
                samples.push(s.map_err(|e| e.to_string())? as f32 / 8388608.0);
            }
        }
        (fmt, bits) => return Err(format!("неподдерживаемый формат WAV: {fmt:?} {bits} бит")),
    }
    Ok(WavData { samples, channels, sample_rate })
}

/// Кодирует в 16-bit PCM WAV (sr/каналы из `data`).
pub fn encode_i16(data: &WavData) -> Result<Vec<u8>, String> {
    let spec = WavSpec {
        channels: data.channels,
        sample_rate: data.sample_rate,
        bits_per_sample: 16,
        sample_format: SampleFormat::Int,
    };
    let mut buf = Cursor::new(Vec::new());
    {
        let mut writer = WavWriter::new(&mut buf, spec).map_err(|e| e.to_string())?;
        for s in &data.samples {
            let v = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
            writer.write_sample(v).map_err(|e| e.to_string())?;
        }
        writer.finalize().map_err(|e| e.to_string())?;
    }
    Ok(buf.into_inner())
}

/// Нормализация громкости: RMS клипа тянется к `target_rms` в ОБЕ стороны
/// (тихие поднимаются, громкие опускаются — генеративные модели выдают
/// непредсказуемый уровень), но гейн ограничен так, чтобы пик не превысил
/// `peak_ceil` (-1 dBFS: защита от клиппинга после усиления). Около-тишина
/// (RMS < -60 dBFS) — брак генерации, не трогаем.
pub fn normalize_loudness(data: &mut WavData, target_rms: f32, peak_ceil: f32) {
    if data.samples.is_empty() {
        return;
    }
    let sum_sq: f32 = data.samples.iter().map(|s| s * s).sum();
    let rms = (sum_sq / data.samples.len() as f32).sqrt();
    if rms < 0.001 {
        return; // тишина/брак — не усиливаем
    }
    let mut gain = target_rms / rms;
    let peak = data.samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    if gain * peak > peak_ceil {
        gain = peak_ceil / peak;
    }
    for s in &mut data.samples {
        *s *= gain;
    }
}

/// Биквад-фильтр (транспонированная DF1) для измерительной цепочки K-громкости.
/// Коэффициенты — константы BS.1770-4 для 48 кГц (MOSS выдаёт только 48 кГц;
/// прочие частоты сэмплирования идут по RMS-фолбэку).
fn biquad_forward(samples: &[f32], ch: usize, b: [f32; 3], a: [f32; 3]) -> Vec<f32> {
    let mut z = vec![[0.0f32; 2]; ch];
    let mut out = Vec::with_capacity(samples.len());
    for chunk in samples.chunks(ch) {
        for c in 0..ch {
            let x = chunk[c];
            let y = b[0] * x + z[c][0];
            z[c][0] = b[1] * x - a[1] * y + z[c][1];
            z[c][1] = b[2] * x - a[2] * y;
            out.push(y);
        }
    }
    out
}

/// Средний квадрат K-взвешенного сигнала (BS.1770-4): pre-filter (high-shelf
/// +4 дБ выше ~1.5 кГц) → RLB high-pass → сумма средних квадратов по каналам.
/// Моделирует восприятие уха: RMS слеп к спектру — бас давит, шипение звенит.
pub fn k_weighted_mean_square(data: &WavData) -> f32 {
    let ch = data.channels.max(1) as usize;
    let frames = (data.samples.len() / ch).max(1);
    let shelf_b = [1.53512485958697, -2.69169618940638, 1.19839281085285];
    let shelf_a = [1.0, -1.69065929318241, 0.73248097495126];
    let rlb_b = [1.0, -2.0, 1.0];
    let rlb_a = [1.0, -1.99004745483398, 0.99007225036621];
    let shelf_out = biquad_forward(&data.samples, ch, shelf_b, shelf_a);
    biquad_forward(&shelf_out, ch, rlb_b, rlb_a)
        .iter()
        .map(|s| s * s)
        .sum::<f32>()
        / frames as f32
}

/// Нормализация по воспринимаемой громкости (BS.1770-4): K-громкость клипа
/// тянется к `target_lufs`, гейн ограничен потолком пика `peak_ceil`
/// (-1 dBFS). Около-тишина — брак генерации, не трогаем. 48 кГц — коэффициенты
/// биквадов посчитаны под неё; прочие частоты — RMS-фолбэк normalize_loudness.
pub fn normalize_lufs(data: &mut WavData, target_lufs: f32, peak_ceil: f32) {
    if data.samples.is_empty() {
        return;
    }
    let ms = k_weighted_mean_square(data);
    if !ms.is_finite() || ms < 1e-7 {
        return; // тишина/брак — не усиливаем
    }
    let loudness = -0.691 + 10.0 * ms.log10();
    let mut gain = 10.0f32.powf((target_lufs - loudness) / 20.0);
    let peak = data.samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    if gain * peak > peak_ceil {
        gain = peak_ceil / peak;
    }
    for s in &mut data.samples {
        *s *= gain;
    }
}

/// Луп-кроссфейд: хвост (последние `secs`) вмиксовывается в начало с линейным
/// рампой, после чего клип обрезается до исходной длины. Классика для
/// бесшовных музыкальных лупов без стыка конец-в-начало.
pub fn loop_crossfade(data: &mut WavData, secs: f32) {
    let ch = data.channels.max(1) as usize;
    let fade_frames = ((secs * data.sample_rate as f32) as usize).min(data.samples.len() / ch / 2);
    if fade_frames == 0 {
        return;
    }
    let total_frames = data.samples.len() / ch;
    let tail_start = (total_frames - fade_frames) * ch;
    let tail: Vec<f32> = data.samples[tail_start..].to_vec();
    for i in 0..fade_frames {
        let t = i as f32 / fade_frames as f32; // 0 → 1
        for c in 0..ch {
            let head = &mut data.samples[i * ch + c];
            *head = *head * t + tail[i * ch + c] * (1.0 - t);
        }
    }
    // Обрезаем хвост, который мы уже вмиксовали в начало.
    data.samples.truncate(tail_start);
}

/// Полный конвейер: bytes → decode → нормализация громкости → опц. кроссфейд
/// → encode. Нормализация автоматична после КАЖДОЙ генерации (решение юзера):
/// RMS к -18 dBFS (≈0.126), пик не выше -1 dBFS.
pub fn postprocess(wav_bytes: &[u8], normalize_loud: bool, loop_secs: f32) -> Result<(Vec<u8>, f32), String> {
    let mut data = decode(wav_bytes)?;
    if normalize_loud {
        // 48 кГц (весь MOSS) — K-взвешенная LUFS-норма (воспринимаемая
        // громкость, BS.1770-4); прочие частоты — RMS-фолбэк, т.к.
        // коэффициенты биквадов посчитаны под 48 кГц.
        if data.sample_rate == 48_000 {
            normalize_lufs(&mut data, -18.0, 0.891);
        } else {
            normalize_loudness(&mut data, 0.126, 0.891);
        }
    }
    if loop_secs > 0.0 {
        loop_crossfade(&mut data, loop_secs);
    }
    let dur = data.duration_secs();
    let out = encode_i16(&data)?;
    Ok((out, dur))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Синтез стерео-синуса в WAV-байты (16-bit, амплитуда 0.95 — почти клип).
    fn sine_wav(sr: u32, freq: f32, secs: f32) -> Vec<u8> {
        let spec = WavSpec { channels: 2, sample_rate: sr, bits_per_sample: 16, sample_format: SampleFormat::Int };
        let mut buf = Cursor::new(Vec::new());
        {
            let mut w = WavWriter::new(&mut buf, spec).unwrap();
            let frames = (sr as f32 * secs) as usize;
            for i in 0..frames {
                let v = (2.0 * std::f32::consts::PI * freq * i as f32 / sr as f32).sin() * 0.95;
                w.write_sample((v * 32767.0) as i16).unwrap();
                w.write_sample((v * 32767.0) as i16).unwrap();
            }
            w.finalize().unwrap();
        }
        buf.into_inner()
    }

    #[test]
    fn decode_encode_roundtrip_preserves_format() {
        let bytes = sine_wav(44100, 440.0, 0.5);
        let data = decode(&bytes).unwrap();
        assert_eq!(data.channels, 2);
        assert_eq!(data.sample_rate, 44100);
        assert!((data.duration_secs() - 0.5).abs() < 0.01);

        let (out, dur) = postprocess(&bytes, true, 0.0).unwrap();
        assert!((dur - 0.5).abs() < 0.01);
        let re = decode(&out).unwrap();
        assert_eq!(re.sample_rate, 44100);
        // RMS-нормализация жмёт синус (RMS 0.672) к 0.126 → peak ≈ 0.178.
        let rms: f32 = {
            let sum_sq: f32 = re.samples.iter().map(|s| s * s).sum();
            (sum_sq / re.samples.len() as f32).sqrt()
        };
        assert!((rms - 0.126).abs() < 0.01, "RMS должен быть у цели, rms={rms}");
    }

    #[test]
    fn loop_crossfade_shortens_and_keeps_signal() {
        let bytes = sine_wav(48000, 220.0, 2.0);
        let mut data = decode(&bytes).unwrap();
        let before = data.samples.len();
        loop_crossfade(&mut data, 0.5);
        assert!(data.samples.len() < before, "хвост должен обрезаться");
        let peak = data.samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        assert!(peak > 0.2, "кроссфейд не должен заглушить сигнал, peak={peak}");
    }

    /// RMS синуса амплитуды `amp` = amp/√2.
    fn rms_of(data: &WavData) -> f32 {
        let sum_sq: f32 = data.samples.iter().map(|s| s * s).sum();
        (sum_sq / data.samples.len() as f32).sqrt()
    }

    #[test]
    fn normalize_loudness_quiets_loud_signal() {
        let bytes = sine_wav(44100, 880.0, 0.2); // пик 0.95, RMS ≈ 0.672
        let mut data = decode(&bytes).unwrap();
        normalize_loudness(&mut data, 0.126, 0.891);
        let out_rms = rms_of(&data);
        assert!((out_rms - 0.126).abs() < 0.01, "RMS должен прижаться к цели, rms={out_rms}");
        let peak = data.samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        assert!(peak <= 0.892, "пик не должен превысить потолок, peak={peak}");
    }

    #[test]
    fn normalize_loudness_boosts_quiet_signal() {
        // Тихий синус (амплитуда 0.05 → RMS ≈ 0.035): должен быть поднят до цели.
        let spec = WavSpec { channels: 1, sample_rate: 48000, bits_per_sample: 16, sample_format: SampleFormat::Int };
        let mut buf = Cursor::new(Vec::new());
        {
            let mut w = WavWriter::new(&mut buf, spec).unwrap();
            for i in 0..48000 {
                let v = (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 48000.0).sin() * 0.05;
                w.write_sample((v * 32767.0) as i16).unwrap();
            }
            w.finalize().unwrap();
        }
        let mut data = decode(&buf.into_inner()).unwrap();
        normalize_loudness(&mut data, 0.126, 0.891);
        let out_rms = rms_of(&data);
        assert!((out_rms - 0.126).abs() < 0.01, "тихий клип должен быть поднят до цели, rms={out_rms}");
    }

    /// K-громкость клипа в LUFS (для ассертов).
    fn loudness_of(data: &WavData) -> f32 {
        -0.691 + 10.0 * k_weighted_mean_square(data).log10()
    }

    #[test]
    fn normalize_lufs_equalizes_bright_and_bass() {
        // Суть LUFS: яркий (6 кГц) и басовитый (200 Гц) сигналы с РАЗНЫМ
        // raw-уровнем должны выровняться по воспринимаемой громкости, а не по RMS.
        let mut bright = decode(&sine_wav(48000, 6000.0, 0.5)).unwrap();
        let mut bass = decode(&sine_wav(48000, 200.0, 0.5)).unwrap();
        for s in &mut bright.samples {
            *s *= 0.2;
        }
        for s in &mut bass.samples {
            *s *= 0.6;
        }
        normalize_lufs(&mut bright, -18.0, 0.891);
        normalize_lufs(&mut bass, -18.0, 0.891);
        let lb = loudness_of(&bright);
        let ls = loudness_of(&bass);
        assert!(
            (lb - ls).abs() < 0.5,
            "LUFS должен выравнивать восприятие: bright={lb} bass={ls}"
        );
    }

    #[test]
    fn normalize_lufs_respects_peak_ceiling() {
        let mut data = decode(&sine_wav(48000, 880.0, 0.2)).unwrap();
        normalize_lufs(&mut data, -6.0, 0.891); // цель выше, чем позволяет пик
        let peak = data.samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        assert!(peak <= 0.892, "пик не должен превысить потолок, peak={peak}");
    }

    #[test]
    fn postprocess_normalizes_lufs_at_48k() {
        let (out, _) = postprocess(&sine_wav(48000, 880.0, 0.5), true, 0.0).unwrap();
        let re = decode(&out).unwrap();
        let l = loudness_of(&re);
        assert!((l - (-18.0)).abs() < 0.5, "48к должен идти по LUFS, lufs={l}");
    }

    #[test]
    fn normalize_loudness_skips_silence() {
        let spec = WavSpec { channels: 1, sample_rate: 48000, bits_per_sample: 16, sample_format: SampleFormat::Int };
        let mut buf = Cursor::new(Vec::new());
        {
            let mut w = WavWriter::new(&mut buf, spec).unwrap();
            for _ in 0..48000 {
                w.write_sample(0i16).unwrap();
            }
            w.finalize().unwrap();
        }
        let mut data = decode(&buf.into_inner()).unwrap();
        normalize_loudness(&mut data, 0.126, 0.891); // не должно паниковать/усиливать
        assert!(data.samples.iter().all(|s| s.abs() < 1e-6), "тишину не трогаем");
    }
}
