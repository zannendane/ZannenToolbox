//! 纯函数 DSP：混音、线性重采样、VAD 分段、WAV 编码。
//! 全部为无副作用纯逻辑，单元测试覆盖。

/// 管线内部统一采样率（16kHz 单声道 f32，STT 服务通用输入）。
pub const TARGET_SAMPLE_RATE: u32 = 16_000;

/// 交错多声道 → 单声道（各声道等权平均）。
pub fn downmix_to_mono(interleaved: &[f32], channels: u16) -> Vec<f32> {
    let ch = (channels as usize).max(1);
    if ch == 1 {
        return interleaved.to_vec();
    }
    interleaved
        .chunks(ch)
        .map(|frame| frame.iter().sum::<f32>() / frame.len() as f32)
        .collect()
}

/// 逐块有状态线性重采样器：跨块保持小数相位与上一末样本，块间无接缝。
pub struct LinearResampler {
    src_rate: u32,
    dst_rate: u32,
    /// 下一个输出点在当前块坐标系中的位置（可为负，负值用到 tail 插值）。
    next_pos: f64,
    /// 上一块最后一个样本（视作当前块坐标 -1 处的样本）。
    tail: Option<f32>,
}

impl LinearResampler {
    pub fn new(src_rate: u32, dst_rate: u32) -> Self {
        Self {
            src_rate,
            dst_rate,
            next_pos: 0.0,
            tail: None,
        }
    }

    /// 推入一段源样本，返回重采样输出。
    pub fn push(&mut self, src: &[f32]) -> Vec<f32> {
        if src.is_empty() {
            return Vec::new();
        }
        if self.src_rate == self.dst_rate {
            return src.to_vec();
        }
        let step = self.src_rate as f64 / self.dst_rate as f64;
        let len = src.len() as i64;
        let mut out = Vec::with_capacity((src.len() as f64 / step) as usize + 2);
        let mut pos = self.next_pos;
        // 可插值条件：floor(pos) >= -1（有 tail）且 floor(pos)+1 <= len-1
        loop {
            let idx = pos.floor() as i64;
            let frac = (pos - idx as f64) as f32;
            if idx > len - 2 {
                break;
            }
            let (a, b) = if idx == -1 {
                match self.tail {
                    Some(t) => (t, src[0]),
                    None => (src[0], src[0]),
                }
            } else if idx < 0 {
                break;
            } else {
                (src[idx as usize], src[(idx + 1) as usize])
            };
            out.push(a + (b - a) * frac);
            pos += step;
        }
        self.next_pos = pos - len as f64;
        self.tail = src.last().copied();
        out
    }
}

/// VAD 配置（采样数以 16kHz 为基准）。
#[derive(Debug, Clone, Copy)]
pub struct VadConfig {
    /// RMS 能量阈值（16kHz f32 语音的典型量级）。
    pub energy_threshold: f32,
    /// 段尾静音判停时长（采样数）。
    pub silence_samples: usize,
    /// 单段上限（采样数），到顶强制截断成段。
    pub max_utterance_samples: usize,
    /// 分析帧长（采样数）。
    pub frame_samples: usize,
}

impl Default for VadConfig {
    fn default() -> Self {
        Self {
            energy_threshold: 0.01,
            silence_samples: (TARGET_SAMPLE_RATE as f32 * 0.8) as usize, // ~0.8s
            max_utterance_samples: (TARGET_SAMPLE_RATE * 10) as usize,   // 10s
            frame_samples: (TARGET_SAMPLE_RATE as f32 * 0.03) as usize,  // 30ms
        }
    }
}

/// 能量阈值 VAD 分段器：语音超阈值累计成段，静音 ~0.8s 收尾，单段上限 10s。
pub struct VadSegmenter {
    cfg: VadConfig,
    /// 当前段已确认的语音样本（不含尾部待定静音）。
    utterance: Vec<f32>,
    /// 段内尾部静音样本缓存：恢复发声时并回本段，超时丢弃。
    pending: Vec<f32>,
    silence_run: usize,
    active: bool,
}

impl VadSegmenter {
    pub fn new(cfg: VadConfig) -> Self {
        Self {
            cfg,
            utterance: Vec::new(),
            pending: Vec::new(),
            silence_run: 0,
            active: false,
        }
    }

    /// 推入 16kHz 单声道样本，返回本次完成的全部语音段（可能多段）。
    pub fn push(&mut self, samples: &[f32]) -> Vec<Vec<f32>> {
        let mut done = Vec::new();
        for frame in samples.chunks(self.cfg.frame_samples.max(1)) {
            let rms = (frame.iter().map(|x| x * x).sum::<f32>() / frame.len() as f32).sqrt();
            if rms >= self.cfg.energy_threshold {
                self.active = true;
                // 段中短暂停顿的内容属于本段
                self.utterance.append(&mut self.pending);
                self.silence_run = 0;
                let room = self
                    .cfg
                    .max_utterance_samples
                    .saturating_sub(self.utterance.len());
                let take = room.min(frame.len());
                self.utterance.extend_from_slice(&frame[..take]);
                if self.utterance.len() >= self.cfg.max_utterance_samples {
                    done.push(std::mem::take(&mut self.utterance));
                    self.active = false;
                    // 帧余量作为下一段的开头（精确到样本边界切分）
                    if take < frame.len() {
                        self.active = true;
                        self.utterance.extend_from_slice(&frame[take..]);
                    }
                }
            } else if self.active {
                self.silence_run += frame.len();
                self.pending.extend_from_slice(frame);
                if self.silence_run >= self.cfg.silence_samples {
                    done.push(std::mem::take(&mut self.utterance));
                    self.pending.clear();
                    self.silence_run = 0;
                    self.active = false;
                }
            }
            // 非活跃且静音：环境底噪，丢弃
        }
        done
    }

    /// 流结束时冲刷未完成的段（不足一段的残余语音）。
    pub fn flush(&mut self) -> Option<Vec<f32>> {
        if self.active && !self.utterance.is_empty() {
            self.active = false;
            Some(std::mem::take(&mut self.utterance))
        } else {
            None
        }
    }
}

/// 16kHz 单声道 s16le PCM WAV（RIFF）编码；f32 样本裁剪到 [-1, 1]。
pub fn wav_encode_16k_mono_s16le(samples: &[f32]) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes()); // PCM fmt 块长
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM 格式
    out.extend_from_slice(&1u16.to_le_bytes()); // 单声道
    out.extend_from_slice(&TARGET_SAMPLE_RATE.to_le_bytes());
    out.extend_from_slice(&(TARGET_SAMPLE_RATE * 2).to_le_bytes()); // 字节率
    out.extend_from_slice(&2u16.to_le_bytes()); // 块对齐
    out.extend_from_slice(&16u16.to_le_bytes()); // 位深
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for &s in samples {
        let v = (s.clamp(-1.0, 1.0) * 32767.0).round() as i16;
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

/// 语音活动判定（带迟滞防抖动）：峰值高于 ON 阈值进入 speaking，
/// 低于 OFF 阈值退出；两阈值之间的迟滞带避免指示灯高频闪烁。
#[derive(Default)]
pub struct VoiceActivity {
    speaking: bool,
}

impl VoiceActivity {
    const ON: f32 = 0.02;
    const OFF: f32 = 0.008;

    pub fn update(&mut self, peak: f32) -> bool {
        if self.speaking {
            self.speaking = peak > Self::OFF;
        } else {
            self.speaking = peak > Self::ON;
        }
        self.speaking
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loud(frame: usize, n: usize) -> Vec<f32> {
        // 能量远超阈值的"语音"（0.5 幅度方波式常数即可，RMS=0.5）
        let _ = frame;
        vec![0.5f32; n]
    }

    #[test]
    fn downmix_averages_channels() {
        let stereo = [1.0, -1.0, 0.5, 0.5, 0.2, 0.0];
        let mono = downmix_to_mono(&stereo, 2);
        assert_eq!(mono.len(), 3);
        assert!((mono[0] - 0.0).abs() < 1e-6);
        assert!((mono[1] - 0.5).abs() < 1e-6);
        assert!((mono[2] - 0.1).abs() < 1e-6);
        // 单声道直通
        assert_eq!(downmix_to_mono(&[0.1, 0.2], 1), vec![0.1, 0.2]);
    }

    #[test]
    fn resampler_same_rate_passthrough() {
        let mut r = LinearResampler::new(16_000, 16_000);
        let src = [0.1, 0.2, 0.3];
        assert_eq!(r.push(&src), src.to_vec());
    }

    #[test]
    fn resampler_2x_downsample() {
        let mut r = LinearResampler::new(32_000, 16_000);
        let a = [0.0, 1.0, 2.0, 3.0, 4.0, 5.0];
        let b = [6.0, 7.0];
        let mut out = r.push(&a);
        out.extend(r.push(&b));
        assert_eq!(out, vec![0.0, 2.0, 4.0, 6.0]);
    }

    #[test]
    fn resampler_2x_upsample_seamless_across_chunks() {
        let mut r = LinearResampler::new(8_000, 16_000);
        let mut out = r.push(&[0.0, 1.0]);
        out.extend(r.push(&[2.0, 3.0]));
        // 0, 0.5, 1, 1.5, 2, 2.5（块间用 tail 无缝插值）
        assert_eq!(out, vec![0.0, 0.5, 1.0, 1.5, 2.0, 2.5]);
    }

    #[test]
    fn vad_silence_produces_nothing() {
        let mut vad = VadSegmenter::new(VadConfig::default());
        let quiet = vec![0.001f32; 16_000];
        assert!(vad.push(&quiet).is_empty());
        assert!(vad.flush().is_none());
    }

    #[test]
    fn vad_speech_then_silence_closes_segment() {
        let cfg = VadConfig::default();
        let mut vad = VadSegmenter::new(cfg);
        let mut stream = loud(0, 16_000); // 1s 语音
        stream.extend(vec![0.0f32; 16_000]); // 1s 静音
        let segs = vad.push(&stream);
        assert_eq!(segs.len(), 1);
        // 段长 ≈ 1s 语音（尾部静音不计入），帧粒度 480 样本
        let len = segs[0].len();
        assert!((15_520..=16_480).contains(&len), "len={len}");
    }

    #[test]
    fn vad_short_pause_stays_in_one_segment() {
        let cfg = VadConfig::default();
        let mut vad = VadSegmenter::new(cfg);
        let mut stream = loud(0, 8_000); // 0.5s
        stream.extend(vec![0.0f32; 4_800]); // 0.3s 停顿（< 0.8s）
        stream.extend(loud(0, 8_000));
        stream.extend(vec![0.0f32; 16_000]);
        let segs = vad.push(&stream);
        assert_eq!(segs.len(), 1);
        // 停顿内容并入本段：总长 ≈ 0.5+0.3+0.5 = 1.3s
        let len = segs[0].len();
        assert!((20_320..=21_280).contains(&len), "len={len}");
    }

    #[test]
    fn vad_two_bursts_two_segments() {
        let mut vad = VadSegmenter::new(VadConfig::default());
        let mut stream = loud(0, 8_000);
        stream.extend(vec![0.0f32; 16_000]); // 1s 静音 → 第一段关闭
        stream.extend(loud(0, 8_000));
        stream.extend(vec![0.0f32; 16_000]);
        let segs = vad.push(&stream);
        assert_eq!(segs.len(), 2);
    }

    #[test]
    fn vad_max_length_forces_split() {
        let mut vad = VadSegmenter::new(VadConfig::default());
        let stream = loud(0, 16_000 * 21); // 21s 连续语音 → 两段 10s + 尾段
        let mut segs = vad.push(&stream);
        if let Some(tail) = vad.flush() {
            segs.push(tail);
        }
        assert_eq!(segs.len(), 3);
        assert_eq!(segs[0].len(), 160_000);
        assert_eq!(segs[1].len(), 160_000);
        assert_eq!(segs[2].len(), 16_000);
    }

    #[test]
    fn voice_activity_hysteresis() {
        let mut va = crate::dsp::VoiceActivity::default();
        assert!(!va.update(0.0));
        assert!(!va.update(0.015)); // ON 阈值之下
        assert!(va.update(0.03)); // 越过 ON → speaking
        assert!(va.update(0.015)); // 迟滞带内保持
        assert!(va.update(0.009));
        assert!(!va.update(0.005)); // 跌破 OFF → 退出
        assert!(!va.update(0.015)); // 需重新越过 ON
    }

    #[test]
    fn wav_header_and_clamping() {
        let wav = wav_encode_16k_mono_s16le(&[0.0, 0.5, -0.5, 2.0, -2.0]);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(u16::from_le_bytes([wav[22], wav[23]]), 1); // mono
        assert_eq!(
            u32::from_le_bytes([wav[24], wav[25], wav[26], wav[27]]),
            16_000
        );
        assert_eq!(u16::from_le_bytes([wav[34], wav[35]]), 16); // bits
        let data_len = u32::from_le_bytes([wav[40], wav[41], wav[42], wav[43]]);
        assert_eq!(data_len, 10);
        let s = |i: usize| i16::from_le_bytes([wav[44 + i * 2], wav[45 + i * 2]]);
        assert_eq!(s(0), 0);
        assert_eq!(s(1), 16384); // 0.5 * 32767 ≈ 16384（round）
        assert_eq!(s(2), -16384);
        assert_eq!(s(3), 32767); // 裁剪
        assert_eq!(s(4), -32767);
    }
}
