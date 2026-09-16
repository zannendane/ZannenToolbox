//! MCUboot 串行 DFU 服务（SMP over serial，nRF54 路径）。
//!
//! ## 协议栈
//!
//! ```text
//! 应用层：CBOR map（{"off":n,"data":[…],"len":…,"sha":…}）
//! SMP 头（10 字节，网络字节序）：
//!   res u8 | ver u8 | op u8 | flags u8 | len u16 BE | group u16 BE | seq u8 | cmd u8
//! 传输层：base64(SMP帧) → 分片（首片带 u16 BE 总长，末片带 CRC16）→ 逐片 SLIP
//! ```
//!
//! - op：read=0 / read_rsp=1 / write=2 / write_rsp=3；flags 恒 0
//! - ver：0 = SMPv1（错误以载荷 `{"rc":n}` 返回，兼容性最好；v2 为 `{"err":{"group","rc"}}`）
//! - group/cmd：os=0（reset=cmd5）、image=1（state=cmd0 读写、upload=cmd1 写）
//!
//! ## 分片格式（Zephyr smp_serial）
//!
//! base64 帧按 124 字节切片；首片前缀 2 字节大端总长（= base64 长 + 2 字节 CRC）；
//! 末片末尾追加 CRC16-CCITT（poly 0x1021，初值 0xFFFF，对完整 base64 字节流，
//! 小端存储）。每片独立 SLIP 转义并以 END(0xC0) 结尾。
//!
//! 参考：Zephyr SMP 协议规范（smp_protocol.html）、smp_serial.c、mcumgr cli。

use std::io::{Read, Write};
use std::path::Path;
use std::time::{Duration, Instant};

use base64::Engine;
use ciborium::value::{Integer, Value as Cbor};
use serde_json::{json, Value};
use sha2::Digest;

use crate::events::EventBus;

// ---------------------------------------------------------------------------
// 常量
// ---------------------------------------------------------------------------

const SLIP_END: u8 = 0xC0;
const SLIP_ESC: u8 = 0xDB;
const SLIP_ESC_END: u8 = 0xDC;
const SLIP_ESC_ESC: u8 = 0xDD;

const OP_READ: u8 = 0;
const OP_READ_RSP: u8 = 1;
const OP_WRITE: u8 = 2;
const OP_WRITE_RSP: u8 = 3;

const GROUP_OS: u16 = 0;
const GROUP_IMAGE: u16 = 1;

const CMD_IMAGE_STATE: u8 = 0;
const CMD_IMAGE_UPLOAD: u8 = 1;
const CMD_OS_RESET: u8 = 5;

/// 单片最大载荷（mcumgr 串行 MTU 128 - 片头 2 - 余量）。
const FRAG_PAYLOAD: usize = 124;
/// 每块镜像数据字节数（CBOR 包装后远小于 SMP serial 默认缓冲）。
const CHUNK_DATA: usize = 200;
/// 单块响应超时。
const RESP_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, thiserror::Error)]
pub enum DfuError {
    #[error("[E3401] IO: {0}")]
    Io(#[from] std::io::Error),
    #[error("[E3402] response timeout")]
    Timeout,
    #[error("[E3403] CRC check failed")]
    BadCrc,
    #[error("[E3404] fragment length mismatch: declared {declared}, received {actual}")]
    BadLength { declared: usize, actual: usize },
    #[error("[E3405] SMP response error rc={0}")]
    Rc(i64),
    #[error("[E3406] offset discontinuity: expected {expected}, device acked {got}")]
    UnexpectedOffset { expected: u64, got: u64 },
    #[error("[E3407] protocol parse failed: {0}")]
    Protocol(String),
}

// ---------------------------------------------------------------------------
// CRC16-CCITT（poly 0x1021，init 0xFFFF，无反射）
// ---------------------------------------------------------------------------

pub fn crc16_ccitt(data: &[u8]) -> u16 {
    let mut crc: u16 = 0xFFFF;
    for &b in data {
        crc ^= (b as u16) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    crc
}

// ---------------------------------------------------------------------------
// SLIP
// ---------------------------------------------------------------------------

/// SLIP 编码并以 END 结尾（前置 END 冲刷链路噪声）。
pub fn slip_encode_frame(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + 2);
    out.push(SLIP_END);
    for &b in data {
        match b {
            SLIP_END => out.extend_from_slice(&[SLIP_ESC, SLIP_ESC_END]),
            SLIP_ESC => out.extend_from_slice(&[SLIP_ESC, SLIP_ESC_ESC]),
            _ => out.push(b),
        }
    }
    out.push(SLIP_END);
    out
}

/// SLIP 流解码器：跨 read 边界缓存半帧。
#[derive(Default)]
pub struct SlipDecoder {
    buf: Vec<u8>,
    in_frame: bool,
    esc: bool,
}

impl SlipDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    /// 喂入字节，返回全部完整帧。
    pub fn feed(&mut self, data: &[u8]) -> Vec<Vec<u8>> {
        let mut frames = Vec::new();
        for &b in data {
            if self.esc {
                self.esc = false;
                match b {
                    SLIP_ESC_END => self.buf.push(SLIP_END),
                    SLIP_ESC_ESC => self.buf.push(SLIP_ESC),
                    other => {
                        // 非法转义：丢弃当前帧防御
                        self.buf.clear();
                        self.in_frame = false;
                        log::warn!("slip: bad escape byte 0x{other:02x}");
                    }
                }
                continue;
            }
            match b {
                SLIP_END => {
                    if self.in_frame && !self.buf.is_empty() {
                        frames.push(std::mem::take(&mut self.buf));
                    }
                    self.in_frame = true;
                    self.buf.clear();
                }
                SLIP_ESC => self.esc = true,
                _ => {
                    if self.in_frame {
                        self.buf.push(b);
                    }
                }
            }
        }
        frames
    }
}

// ---------------------------------------------------------------------------
// SMP 头
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SmpHeader {
    pub op: u8,
    pub group: u16,
    pub seq: u8,
    pub cmd: u8,
    pub len: u16,
}

impl SmpHeader {
    pub const SIZE: usize = 10;

    /// res=0、ver=0（SMPv1）、flags=0。
    pub fn encode(&self) -> [u8; 10] {
        [
            0, // res
            0, // ver = SMPv1
            self.op,
            0, // flags
            (self.len >> 8) as u8,
            self.len as u8,
            (self.group >> 8) as u8,
            self.group as u8,
            self.seq,
            self.cmd,
        ]
    }

    pub fn decode(frame: &[u8]) -> Result<(Self, &[u8]), DfuError> {
        if frame.len() < Self::SIZE {
            return Err(DfuError::Protocol(format!(
                "frame too short: {}",
                frame.len()
            )));
        }
        let header = SmpHeader {
            op: frame[2],
            len: u16::from_be_bytes([frame[4], frame[5]]),
            group: u16::from_be_bytes([frame[6], frame[7]]),
            seq: frame[8],
            cmd: frame[9],
        };
        let payload = &frame[Self::SIZE..];
        if payload.len() != header.len as usize {
            return Err(DfuError::Protocol(format!(
                "smp len mismatch: header {} vs payload {}",
                header.len,
                payload.len()
            )));
        }
        Ok((header, payload))
    }
}

// ---------------------------------------------------------------------------
// base64 分片 / 重组
// ---------------------------------------------------------------------------

/// 把完整 SMP 帧编码为一组分片（base64 + 总长前缀 + CRC16 尾）。
pub fn fragment_frame(smp_frame: &[u8]) -> Vec<Vec<u8>> {
    let b64 = base64::engine::general_purpose::STANDARD.encode(smp_frame);
    let b64 = b64.as_bytes();
    let total_len = (b64.len() + 2) as u16; // 含 CRC2 字节

    let mut frags = Vec::new();
    let mut payload = Vec::with_capacity(b64.len() + 4);
    payload.extend_from_slice(&total_len.to_be_bytes());
    payload.extend_from_slice(b64);
    payload.extend_from_slice(&crc16_ccitt(b64).to_le_bytes());

    // 首片包含 2 字节总长前缀，之后按 FRAG_PAYLOAD 连续切片
    for chunk in payload.chunks(FRAG_PAYLOAD) {
        frags.push(chunk.to_vec());
    }
    frags
}

/// 分片重组器。
#[derive(Default)]
pub struct Defragmenter {
    buf: Vec<u8>,
    expected: Option<usize>,
}

impl Defragmenter {
    /// 喂入一个完整分片；凑齐时返回完整 SMP 帧。
    pub fn feed(&mut self, frag: &[u8]) -> Result<Option<Vec<u8>>, DfuError> {
        if self.buf.is_empty() {
            if frag.len() < 2 {
                return Err(DfuError::Protocol("fragment too short for header".into()));
            }
            let total = u16::from_be_bytes([frag[0], frag[1]]) as usize;
            self.expected = Some(total);
            self.buf.extend_from_slice(&frag[2..]);
        } else {
            self.buf.extend_from_slice(frag);
        }

        let Some(total) = self.expected else {
            return Err(DfuError::Protocol("defrag state".into()));
        };
        if self.buf.len() < total {
            return Ok(None);
        }
        if self.buf.len() > total {
            let actual = self.buf.len();
            self.buf.clear();
            self.expected = None;
            return Err(DfuError::BadLength {
                declared: total,
                actual,
            });
        }
        // 凑齐：buf = b64 + crc16le
        let data = std::mem::take(&mut self.buf);
        self.expected = None;
        let (b64, crc_bytes) = data.split_at(data.len() - 2);
        let expected_crc = u16::from_le_bytes([crc_bytes[0], crc_bytes[1]]);
        if crc16_ccitt(b64) != expected_crc {
            return Err(DfuError::BadCrc);
        }
        let frame = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .map_err(|e| DfuError::Protocol(format!("base64: {e}")))?;
        Ok(Some(frame))
    }
}

// ---------------------------------------------------------------------------
// SMP 客户端（传输无关）
// ---------------------------------------------------------------------------

/// SMP 传输层抽象：发送/接收完整 SMP 帧（串口、回环测试各一实现）。
pub trait SmpTransport {
    fn send_frame(&mut self, frame: &[u8]) -> Result<(), DfuError>;
    fn recv_frame(&mut self, timeout: Duration) -> Result<Vec<u8>, DfuError>;
}

/// SMP 请求/响应客户端。
pub struct SmpClient<T: SmpTransport> {
    transport: T,
    seq: u8,
}

impl<T: SmpTransport> SmpClient<T> {
    pub fn new(transport: T) -> Self {
        Self { transport, seq: 0 }
    }

    /// 发一次请求并等待响应，返回 CBOR 载荷。
    pub fn request(
        &mut self,
        op: u8,
        group: u16,
        cmd: u8,
        cmd_payload: Cbor,
    ) -> Result<Cbor, DfuError> {
        let payload = cbor_encode(&cmd_payload)?;
        let header = SmpHeader {
            op,
            group,
            seq: self.seq,
            cmd,
            len: payload.len() as u16,
        };
        self.seq = self.seq.wrapping_add(1);

        let mut frame = Vec::with_capacity(SmpHeader::SIZE + payload.len());
        frame.extend_from_slice(&header.encode());
        frame.extend_from_slice(&payload);
        self.transport.send_frame(&frame)?;

        loop {
            let resp_frame = self.transport.recv_frame(RESP_TIMEOUT)?;
            let (hdr, payload) = SmpHeader::decode(&resp_frame)?;
            let expected_op = if op == OP_READ {
                OP_READ_RSP
            } else {
                OP_WRITE_RSP
            };
            if hdr.op != expected_op {
                log::warn!("smp: skip frame with op {} (expect {expected_op})", hdr.op);
                continue;
            }
            let value = cbor_decode(payload)?;
            check_rc(&value)?;
            return Ok(value);
        }
    }
}

fn check_rc(value: &Cbor) -> Result<(), DfuError> {
    if let Cbor::Map(entries) = value {
        // v1: {"rc": n}；v2: {"err": {"rc": n}}
        for (k, v) in entries {
            let key = match k {
                Cbor::Text(s) => s.as_str(),
                _ => continue,
            };
            let rc_val = match (key, v) {
                ("rc", Cbor::Integer(i)) => Some(*i),
                ("err", Cbor::Map(inner)) => inner.iter().find_map(|(k2, v2)| match (k2, v2) {
                    (Cbor::Text(s), Cbor::Integer(i)) if s == "rc" => Some(*i),
                    _ => None,
                }),
                _ => None,
            };
            if let Some(rc) = rc_val {
                let rc: i64 = rc.try_into().unwrap_or(-1);
                if rc != 0 {
                    return Err(DfuError::Rc(rc));
                }
            }
        }
    }
    Ok(())
}

fn cbor_encode(value: &Cbor) -> Result<Vec<u8>, DfuError> {
    let mut out = Vec::new();
    ciborium::ser::into_writer(value, &mut out)
        .map_err(|e| DfuError::Protocol(format!("cbor encode: {e}")))?;
    Ok(out)
}

fn cbor_decode(bytes: &[u8]) -> Result<Cbor, DfuError> {
    ciborium::de::from_reader(bytes).map_err(|e| DfuError::Protocol(format!("cbor decode: {e}")))
}

fn cbor_map(entries: Vec<(&str, Cbor)>) -> Cbor {
    Cbor::Map(
        entries
            .into_iter()
            .map(|(k, v)| (Cbor::Text(k.to_string()), v))
            .collect(),
    )
}

fn cbor_int(v: u64) -> Cbor {
    Cbor::Integer(Integer::from(v))
}

/// 上传块载荷：首块带 len/sha/image，其余仅 off/data。
pub fn build_upload_payload(
    off: u64,
    data: &[u8],
    total_len: u64,
    sha256: &[u8; 32],
    image_index: u64,
) -> Cbor {
    let mut entries = vec![("data", Cbor::Bytes(data.to_vec())), ("off", cbor_int(off))];
    if off == 0 {
        entries.push(("len", cbor_int(total_len)));
        entries.push(("sha", Cbor::Bytes(sha256.to_vec())));
        entries.push(("image", cbor_int(image_index)));
        entries.push(("upgrade", Cbor::Bool(false)));
    }
    cbor_map(entries)
}

/// 从上传响应中提取已确认偏移。
pub fn parse_upload_offset(value: &Cbor) -> Option<u64> {
    if let Cbor::Map(entries) = value {
        for (k, v) in entries {
            if let (Cbor::Text(s), Cbor::Integer(i)) = (k, v) {
                if s == "off" {
                    return (*i).try_into().ok();
                }
            }
        }
    }
    None
}

/// 从 image state 响应提取首个镜像的 hash（hex）。
pub fn parse_image_hash(value: &Cbor) -> Option<String> {
    let images = value.as_map()?.iter().find_map(|(k, v)| match (k, v) {
        (Cbor::Text(s), Cbor::Array(arr)) if s == "images" => Some(arr),
        _ => None,
    })?;
    let first = images.first()?.as_map()?;
    for (k, v) in first {
        if let (Cbor::Text(s), Cbor::Bytes(b)) = (k, v) {
            if s == "hash" {
                return Some(b.iter().map(|x| format!("{x:02x}")).collect());
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// 串口传输
// ---------------------------------------------------------------------------

/// SMP 传输构造：mock-transport 下 `mock://` 路径走内存 MCUboot 模拟器。
fn open_transport(path: &str, baud: u32) -> Result<Box<dyn SmpTransport>, DfuError> {
    #[cfg(feature = "mock-transport")]
    if path.starts_with("mock://") {
        return Ok(Box::new(MockMcuTransport::default()));
    }
    let _ = baud;
    Ok(Box::new(SerialSmpTransport::open(path, baud)?))
}

/// `Box<dyn SmpTransport>` 转发实现（mock/串口统一入口）。
impl SmpTransport for Box<dyn SmpTransport> {
    fn send_frame(&mut self, frame: &[u8]) -> Result<(), DfuError> {
        (**self).send_frame(frame)
    }
    fn recv_frame(&mut self, timeout: Duration) -> Result<Vec<u8>, DfuError> {
        (**self).recv_frame(timeout)
    }
}

/// 内存 MCUboot 模拟器（mock-transport）：无硬件演示/测试 SMP 全流程。
/// 行为：upload 应答 off 累加；state 返回一个已确认镜像；confirm/reset 应答空 map。
#[cfg(feature = "mock-transport")]
#[derive(Default)]
struct MockMcuTransport {
    responses: std::collections::VecDeque<Vec<u8>>,
    uploaded: u64,
}

#[cfg(feature = "mock-transport")]
impl SmpTransport for MockMcuTransport {
    fn send_frame(&mut self, frame: &[u8]) -> Result<(), DfuError> {
        let (hdr, payload) = SmpHeader::decode(frame)?;
        let value = cbor_decode(payload)?;
        let resp_payload = match (hdr.op, hdr.group, hdr.cmd) {
            (OP_WRITE, GROUP_IMAGE, CMD_IMAGE_UPLOAD) => {
                let off = cbor_get_int(&value, "off").unwrap_or(0);
                let data_len = cbor_get_bytes_len(&value, "data").unwrap_or(0);
                self.uploaded = off + data_len;
                cbor_map(vec![("off", cbor_int(self.uploaded))])
            }
            (OP_READ, GROUP_IMAGE, CMD_IMAGE_STATE) => cbor_map(vec![(
                "images",
                Cbor::Array(vec![cbor_map(vec![
                    ("slot", cbor_int(0)),
                    ("hash", Cbor::Bytes(vec![0xAA; 32])),
                    ("active", Cbor::Bool(true)),
                    ("confirmed", Cbor::Bool(true)),
                ])]),
            )]),
            (OP_WRITE, GROUP_IMAGE, CMD_IMAGE_STATE) => cbor_map(vec![]), // confirm
            (OP_WRITE, GROUP_OS, CMD_OS_RESET) => cbor_map(vec![]),
            _ => cbor_map(vec![("rc", cbor_int(2))]), // 未知命令
        };
        let payload = cbor_encode(&resp_payload)?;
        let rh = SmpHeader {
            op: if hdr.op == OP_READ {
                OP_READ_RSP
            } else {
                OP_WRITE_RSP
            },
            group: hdr.group,
            seq: hdr.seq,
            cmd: hdr.cmd,
            len: payload.len() as u16,
        };
        let mut frame = rh.encode().to_vec();
        frame.extend_from_slice(&payload);
        self.responses.push_back(frame);
        Ok(())
    }

    fn recv_frame(&mut self, _timeout: Duration) -> Result<Vec<u8>, DfuError> {
        self.responses.pop_front().ok_or(DfuError::Timeout)
    }
}

#[cfg(feature = "mock-transport")]
fn cbor_get_int(v: &Cbor, key: &str) -> Option<u64> {
    v.as_map()?.iter().find_map(|(k, val)| match (k, val) {
        (Cbor::Text(s), Cbor::Integer(i)) if s == key => (*i).try_into().ok(),
        _ => None,
    })
}

#[cfg(feature = "mock-transport")]
fn cbor_get_bytes_len(v: &Cbor, key: &str) -> Option<u64> {
    v.as_map()?.iter().find_map(|(k, val)| match (k, val) {
        (Cbor::Text(s), Cbor::Bytes(b)) if s == key => Some(b.len() as u64),
        _ => None,
    })
}

/// 串口 SMP 传输：SLIP 分片发送 + 帧重组接收。
pub struct SerialSmpTransport {
    port: Box<dyn serialport::SerialPort>,
    slip: SlipDecoder,
    defrag: Defragmenter,
}

impl SerialSmpTransport {
    pub fn open(path: &str, baud: u32) -> Result<Self, DfuError> {
        let port = serialport::new(path, baud)
            .timeout(Duration::from_millis(100))
            .open()
            .map_err(|e| DfuError::Io(std::io::Error::other(e.to_string())))?;
        Ok(Self {
            port,
            slip: SlipDecoder::new(),
            defrag: Defragmenter::default(),
        })
    }
}

impl SmpTransport for SerialSmpTransport {
    fn send_frame(&mut self, frame: &[u8]) -> Result<(), DfuError> {
        for frag in fragment_frame(frame) {
            let slip = slip_encode_frame(&frag);
            self.port.write_all(&slip)?;
            self.port.flush()?;
        }
        Ok(())
    }

    fn recv_frame(&mut self, timeout: Duration) -> Result<Vec<u8>, DfuError> {
        let deadline = Instant::now() + timeout;
        let mut buf = [0u8; 512];
        loop {
            if Instant::now() >= deadline {
                return Err(DfuError::Timeout);
            }
            match self.port.read(&mut buf) {
                Ok(0) => {}
                Ok(n) => {
                    for slip_frame in self.slip.feed(&buf[..n]) {
                        if let Some(smp_frame) = self.defrag.feed(&slip_frame)? {
                            return Ok(smp_frame);
                        }
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {}
                Err(e) => return Err(DfuError::Io(e)),
            }
        }
    }
}

// ---------------------------------------------------------------------------
// DFU 服务
// ---------------------------------------------------------------------------

/// MCUboot 串行 DFU 服务。
#[derive(Clone)]
pub struct DfuService {
    bus: EventBus,
}

impl DfuService {
    pub fn new(bus: EventBus) -> Self {
        Self { bus }
    }

    /// 阻塞式上传镜像：image state 查询（容错）→ 分块上传 → 完成事件。
    pub fn upload(
        &self,
        job: &str,
        port_path: &str,
        baud: u32,
        image_path: &Path,
    ) -> Result<Value, DfuError> {
        let image = std::fs::read(image_path)?;
        let total = image.len();
        let sha: [u8; 32] = sha2::Sha256::digest(&image).into();

        let transport = open_transport(port_path, baud)?;
        let mut client = SmpClient::new(transport);

        // 可选：先读 image state（设备忙碌/不支持时仅记录，不阻断上传）
        match client.request(OP_READ, GROUP_IMAGE, CMD_IMAGE_STATE, cbor_map(vec![])) {
            Ok(state) => {
                if let Some(hash) = parse_image_hash(&state) {
                    log::info!("dfu: current image hash {hash}");
                }
            }
            Err(e) => log::warn!("dfu: image state query skipped: {e}"),
        }

        let mut off: u64 = 0;
        let mut chunk_index = 0usize;
        while (off as usize) < total {
            let end = ((off as usize) + CHUNK_DATA).min(total);
            let data = &image[off as usize..end];
            let payload = build_upload_payload(off, data, total as u64, &sha, 0);
            let resp = client.request(OP_WRITE, GROUP_IMAGE, CMD_IMAGE_UPLOAD, payload)?;
            let confirmed = parse_upload_offset(&resp)
                .ok_or_else(|| DfuError::Protocol("upload response missing off".into()))?;
            let expected = end as u64;
            if confirmed != expected {
                return Err(DfuError::UnexpectedOffset {
                    expected,
                    got: confirmed,
                });
            }
            off = expected;
            chunk_index += 1;
            // 每 8 块（~1.6KB）或最后一块发进度，避免事件洪泛
            if chunk_index.is_multiple_of(8) || off as usize == total {
                self.bus.publish(
                    "dfu.progress",
                    json!({
                        "job": job,
                        "written": off,
                        "total": total,
                        "percent": (off * 100 / total as u64),
                    }),
                );
            }
        }

        self.bus.publish(
            "dfu.done",
            json!({ "job": job, "bytes": total, "sha256": hex(&sha) }),
        );
        Ok(json!({ "uploaded": total, "sha256": hex(&sha) }))
    }

    /// 确认镜像（hash 缺省时先读 state 取第一个镜像的 hash）。
    pub fn confirm(
        &self,
        port_path: &str,
        baud: u32,
        hash_hex: Option<&str>,
    ) -> Result<Value, DfuError> {
        let transport = open_transport(port_path, baud)?;
        let mut client = SmpClient::new(transport);

        let hash_bytes: Vec<u8> = match hash_hex {
            Some(h) => unhex(h).map_err(DfuError::Protocol)?,
            None => {
                let state =
                    client.request(OP_READ, GROUP_IMAGE, CMD_IMAGE_STATE, cbor_map(vec![]))?;
                let hash = parse_image_hash(&state)
                    .ok_or_else(|| DfuError::Protocol("no image hash in state".into()))?;
                unhex(&hash).map_err(DfuError::Protocol)?
            }
        };

        client.request(
            OP_WRITE,
            GROUP_IMAGE,
            CMD_IMAGE_STATE,
            cbor_map(vec![
                ("hash", Cbor::Bytes(hash_bytes.clone())),
                ("confirm", Cbor::Bool(true)),
            ]),
        )?;
        Ok(json!({ "confirmed": hex(&hash_bytes) }))
    }

    /// 复位设备（os group reset）。
    pub fn reset(&self, port_path: &str, baud: u32) -> Result<Value, DfuError> {
        let transport = open_transport(port_path, baud)?;
        let mut client = SmpClient::new(transport);
        // reset 响应可能来不及发出设备就重启，超时不算错误
        match client.request(OP_WRITE, GROUP_OS, CMD_OS_RESET, cbor_map(vec![])) {
            Ok(_) | Err(DfuError::Timeout) => Ok(json!({ "reset": true })),
            Err(e) => Err(e),
        }
    }

    /// 供调度层使用的错误事件上报。
    pub fn publish_error(&self, job: &str, error: &str) {
        self.bus
            .publish("dfu.error", json!({ "job": job, "error": error }));
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(s: &str) -> Result<Vec<u8>, String> {
    let cleaned: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    if !cleaned.len().is_multiple_of(2) {
        return Err("hex length must be even".into());
    }
    (0..cleaned.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&cleaned[i..i + 2], 16).map_err(|e| e.to_string()))
        .collect()
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc16_known_vector() {
        assert_eq!(crc16_ccitt(b"123456789"), 0x29B1);
        assert_eq!(crc16_ccitt(b""), 0xFFFF);
    }

    #[test]
    fn slip_roundtrip_with_escapes() {
        let data = [0xC0, 0x01, 0xDB, 0x00, 0xC0, 0xFF];
        let encoded = slip_encode_frame(&data);
        assert!(encoded.starts_with(&[SLIP_END]) && encoded.ends_with(&[SLIP_END]));
        let mut dec = SlipDecoder::new();
        // 刻意在半截处喂入，验证跨边界重组
        let mid = encoded.len() / 2;
        let mut frames = dec.feed(&encoded[..mid]);
        frames.extend(dec.feed(&encoded[mid..]));
        assert_eq!(frames, vec![data.to_vec()]);
    }

    #[test]
    fn smp_header_roundtrip() {
        let h = SmpHeader {
            op: OP_WRITE,
            group: GROUP_IMAGE,
            seq: 7,
            cmd: CMD_IMAGE_UPLOAD,
            len: 300,
        };
        let mut frame = h.encode().to_vec();
        frame.extend_from_slice(&vec![0xAB; 300]);
        let (decoded, payload) = SmpHeader::decode(&frame).unwrap();
        assert_eq!(decoded, h);
        assert_eq!(payload.len(), 300);
        assert!(SmpHeader::decode(&frame[..100]).is_err()); // len 不符
    }

    #[test]
    fn fragment_defrag_roundtrip() {
        // 构造一个跨多片的帧（>124 字节触发分片）
        let payload = vec![0x5A; 500];
        let header = SmpHeader {
            op: OP_WRITE,
            group: GROUP_IMAGE,
            seq: 1,
            cmd: CMD_IMAGE_UPLOAD,
            len: 500,
        };
        let mut frame = header.encode().to_vec();
        frame.extend_from_slice(&payload);

        let frags = fragment_frame(&frame);
        assert!(
            frags.len() >= 3,
            "500B payload should fragment, got {}",
            frags.len()
        );
        assert!(frags[0].len() <= FRAG_PAYLOAD);

        let mut defrag = Defragmenter::default();
        let mut out = None;
        for f in &frags {
            out = defrag.feed(f).unwrap();
        }
        assert_eq!(out.unwrap(), frame);
    }

    #[test]
    fn defrag_rejects_bad_crc() {
        let payload = vec![0x11; 100];
        let header = SmpHeader {
            op: OP_READ,
            group: GROUP_OS,
            seq: 0,
            cmd: CMD_OS_RESET,
            len: 100,
        };
        let mut frame = header.encode().to_vec();
        frame.extend_from_slice(&payload);
        let mut frags = fragment_frame(&frame);
        // 翻转最后一片的最后一个字节（CRC 区）
        let last = frags.last_mut().unwrap();
        let n = last.len();
        last[n - 1] ^= 0xFF;
        let mut defrag = Defragmenter::default();
        let mut result = Ok(None);
        for f in &frags {
            result = defrag.feed(f);
        }
        assert!(matches!(result, Err(DfuError::BadCrc)));
    }

    #[test]
    fn upload_payload_shapes() {
        let sha = [0x42; 32];
        let first = build_upload_payload(0, &[1, 2, 3], 1000, &sha, 0);
        let entries = first.as_map().unwrap();
        let keys: Vec<&str> = entries.iter().filter_map(|(k, _)| k.as_text()).collect();
        assert!(keys.contains(&"len") && keys.contains(&"sha") && keys.contains(&"data"));
        let later = build_upload_payload(200, &[1, 2], 1000, &sha, 0);
        let keys: Vec<&str> = later
            .as_map()
            .unwrap()
            .iter()
            .filter_map(|(k, _)| k.as_text())
            .collect();
        assert!(!keys.contains(&"len") && !keys.contains(&"sha"));
        assert!(keys.contains(&"off") && keys.contains(&"data"));
    }

    /// 内存回环传输：按脚本自动应答 upload（off 累加）。
    struct ScriptedTransport {
        sent_frames: Vec<Vec<u8>>,
    }

    impl SmpTransport for ScriptedTransport {
        fn send_frame(&mut self, frame: &[u8]) -> Result<(), DfuError> {
            self.sent_frames.push(frame.to_vec());
            Ok(())
        }
        fn recv_frame(&mut self, _timeout: Duration) -> Result<Vec<u8>, DfuError> {
            // 根据最近一帧生成响应
            let last = self.sent_frames.last().unwrap();
            let (hdr, payload) = SmpHeader::decode(last).unwrap();
            let value = cbor_decode(payload).unwrap();
            let resp_payload = if hdr.op == OP_READ {
                cbor_map(vec![("images", Cbor::Array(vec![]))])
            } else {
                let off = value
                    .as_map()
                    .and_then(|m| {
                        m.iter().find_map(|(k, v)| match (k, v) {
                            (Cbor::Text(s), Cbor::Integer(i)) if s == "off" => {
                                let v: u64 = (*i).try_into().ok()?;
                                Some(v)
                            }
                            _ => None,
                        })
                    })
                    .unwrap_or(0);
                let data_len = value
                    .as_map()
                    .and_then(|m| {
                        m.iter().find_map(|(k, v)| match (k, v) {
                            (Cbor::Text(s), Cbor::Bytes(b)) if s == "data" => Some(b.len() as u64),
                            _ => None,
                        })
                    })
                    .unwrap_or(0);
                cbor_map(vec![("rc", cbor_int(0)), ("off", cbor_int(off + data_len))])
            };
            let payload = cbor_encode(&resp_payload).unwrap();
            let rh = SmpHeader {
                op: if hdr.op == OP_READ {
                    OP_READ_RSP
                } else {
                    OP_WRITE_RSP
                },
                group: hdr.group,
                seq: hdr.seq,
                cmd: hdr.cmd,
                len: payload.len() as u16,
            };
            let mut frame = rh.encode().to_vec();
            frame.extend_from_slice(&payload);
            Ok(frame)
        }
    }

    #[test]
    fn upload_state_machine_offsets() {
        let image = vec![0xAB; 950]; // 950B → 5 块（200*4 + 150）
        let transport = ScriptedTransport {
            sent_frames: vec![],
        };
        let mut client = SmpClient::new(transport);
        let sha = [0u8; 32];

        let mut off = 0u64;
        while (off as usize) < image.len() {
            let end = ((off as usize) + CHUNK_DATA).min(image.len());
            let resp = client
                .request(
                    OP_WRITE,
                    GROUP_IMAGE,
                    CMD_IMAGE_UPLOAD,
                    build_upload_payload(off, &image[off as usize..end], 950, &sha, 0),
                )
                .unwrap();
            off = parse_upload_offset(&resp).unwrap();
        }
        assert_eq!(off, 950);
        // 5 次 upload 写
        assert_eq!(client.transport.sent_frames.len(), 5);
    }

    #[test]
    fn rc_error_detected() {
        let v = cbor_map(vec![("rc", cbor_int(3))]);
        assert!(matches!(check_rc(&v), Err(DfuError::Rc(3))));
        let ok = cbor_map(vec![("rc", cbor_int(0)), ("off", cbor_int(42))]);
        assert!(check_rc(&ok).is_ok());
        // v2 风格
        let v2 = cbor_map(vec![("err", cbor_map(vec![("rc", cbor_int(1))]))]);
        assert!(matches!(check_rc(&v2), Err(DfuError::Rc(1))));
    }

    #[test]
    fn parse_image_hash_works() {
        let state = cbor_map(vec![(
            "images",
            Cbor::Array(vec![cbor_map(vec![
                ("slot", cbor_int(0)),
                ("hash", Cbor::Bytes(vec![0xDE, 0xAD])),
            ])]),
        )]);
        assert_eq!(parse_image_hash(&state).as_deref(), Some("dead"));
    }

    #[test]
    fn unhex_works() {
        assert_eq!(unhex("deadbeef").unwrap(), vec![0xDE, 0xAD, 0xBE, 0xEF]);
        assert!(unhex("abc").is_err());
    }

    /// mock-transport 下走完整 DFU 流程：上传 → confirm → reset。
    #[cfg(feature = "mock-transport")]
    #[tokio::test]
    async fn mock_full_dfu_flow() {
        let bus = EventBus::new(64);
        let mut rx = bus.subscribe();
        let dfu = DfuService::new(bus);

        // 造一个临时镜像文件
        let tmp = std::env::temp_dir().join(format!("zn-dfu-test-{}.bin", std::process::id()));
        std::fs::write(&tmp, vec![0x5Au8; 950]).unwrap();

        dfu.upload("job1", "mock://zannen-smol-air", 115200, &tmp)
            .unwrap();
        dfu.confirm("mock://zannen-smol-air", 115200, None).unwrap();
        dfu.reset("mock://zannen-smol-air", 115200).unwrap();

        // 收齐事件：应有 dfu.progress（含 percent=100）与 dfu.done
        let mut saw_done = false;
        let mut saw_progress_100 = false;
        while let Ok(ev) = rx.try_recv() {
            if ev.topic == "dfu.done" && ev.payload["job"] == "job1" {
                saw_done = true;
                assert_eq!(ev.payload["bytes"], 950);
            }
            if ev.topic == "dfu.progress" && ev.payload["percent"] == 100 {
                saw_progress_100 = true;
            }
        }
        assert!(saw_progress_100, "expected 100% progress");
        assert!(saw_done, "expected dfu.done");
        let _ = std::fs::remove_file(&tmp);
    }
}
