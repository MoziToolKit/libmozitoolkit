#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Protocol magic header bytes: 'M' (0x4D), 'T' (0x54).
pub const PROTOCOL_MAGIC: [u8; 2] = [0x4D, 0x54];

/// Canonical protocol version number (Version 3.1, Protocol v3).
pub const PROTOCOL_VERSION: u8 = 0x03;

/// Standard 4-byte header size: Magic(2) + Version(1) + Opcode(1).
pub(crate) const HEADER_SIZE: usize = 4;

/// Resource limit guards (RFC Section 2.2).
pub(crate) const MAX_PALETTE_ENTRIES: usize = 65535;
pub(crate) const MAX_BASELINE_DECODED_BYTES: usize = 256 * 1024 * 1024; // 256 MiB
pub(crate) const MAX_SELECTION_VOLUME: u64 = 33_554_432; // 32M voxels
pub(crate) const MAX_DELTA_BATCH_CHANGES: usize = 16384;
pub(crate) const MAX_DELTA_STAGING_BATCHES: usize = 256;

/// Protocol Opcode enumeration (RFC Section 4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[repr(u8)]
pub enum Opcode {
    /// 0x01: Client Handshake Initiation (C -> S)
    Hello = 0x01,
    /// 0x02: Server Handshake Response (S -> C)
    Welcome = 0x02,
    /// 0x03: Full Baseline Header Metadata (S -> C)
    BaselineBegin = 0x03,
    /// 0x04: Baseline Voxel Chunk Stream (S -> C)
    BaselineChunk = 0x04,
    /// 0x05: Baseline Finished Notification (S -> C)
    BaselineEnd = 0x05,
    /// 0x06: Baseline Client Commit Acknowledgment (C -> S)
    BaselineAck = 0x06,
    /// 0x07: Incremental Block Delta Batch (S -> C)
    DeltaOp = 0x07,
    /// 0x08: Dynamic Palette Append (S -> C)
    PaletteUpdate = 0x08,
    /// 0x0B: Task / Session Cancellation (Bidirectional)
    Cancel = 0x0B,
    /// 0x0D: Resync and Gap Recovery Request (C -> S)
    ResyncRequest = 0x0D,
    /// 0x0E: Keepalive Ping (Bidirectional)
    Ping = 0x0E,
    /// 0x0F: Keepalive Pong (Bidirectional)
    Pong = 0x0F,
    /// 0x10: Diagnostic Error Response (Bidirectional)
    Error = 0x10,
    /// 0x11: Sparse BlockEntity Stream (RFC v3.2, S -> C)
    BaselineEntities = 0x11,
    /// 0x12: Sparse BlockEntity Delta Update (RFC v3.2, S -> C)
    DeltaEntity = 0x12,
    /// 0x13: Continuous Visual Entity Stream (RFC v3.2, S -> C)
    BaselineVisualEntities = 0x13,
    /// 0x14: Continuous Visual Entity Delta Update (RFC v3.2, S -> C)
    DeltaVisualEntity = 0x14,
}

impl Opcode {
    pub fn from_u8(val: u8) -> Option<Self> {
        match val {
            0x01 => Some(Opcode::Hello),
            0x02 => Some(Opcode::Welcome),
            0x03 => Some(Opcode::BaselineBegin),
            0x04 => Some(Opcode::BaselineChunk),
            0x05 => Some(Opcode::BaselineEnd),
            0x06 => Some(Opcode::BaselineAck),
            0x07 => Some(Opcode::DeltaOp),
            0x08 => Some(Opcode::PaletteUpdate),
            0x0B => Some(Opcode::Cancel),
            0x0D => Some(Opcode::ResyncRequest),
            0x0E => Some(Opcode::Ping),
            0x0F => Some(Opcode::Pong),
            0x10 => Some(Opcode::Error),
            0x11 => Some(Opcode::BaselineEntities),
            0x12 => Some(Opcode::DeltaEntity),
            0x13 => Some(Opcode::BaselineVisualEntities),
            0x14 => Some(Opcode::DeltaVisualEntity),
            _ => None,
        }
    }
}

/// SyncDecision returned in WELCOME response (RFC Section 4.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[repr(u8)]
pub enum SyncDecision {
    VerifiedSkipRebuild = 0x00,
    FullBaselineRequired = 0x02,
    Rejected = 0xFF,
}

impl SyncDecision {
    pub fn from_u8(val: u8) -> Option<Self> {
        match val {
            0x00 => Some(SyncDecision::VerifiedSkipRebuild),
            0x02 => Some(SyncDecision::FullBaselineRequired),
            0xFF => Some(SyncDecision::Rejected),
            _ => None,
        }
    }
}

/// Baseline ACK result code (RFC Section 4.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[repr(u8)]
pub enum BaselineAckResult {
    SuccessCommitted = 0x00,
    ValidationFailed = 0x01,
    BufferOverflow = 0x02,
}

impl BaselineAckResult {
    pub fn from_u8(val: u8) -> Option<Self> {
        match val {
            0x00 => Some(BaselineAckResult::SuccessCommitted),
            0x01 => Some(BaselineAckResult::ValidationFailed),
            0x02 => Some(BaselineAckResult::BufferOverflow),
            _ => None,
        }
    }
}

/// Cancel reason code (RFC Section 4.12).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[repr(u8)]
pub enum CancelReason {
    SelectionChanged = 0x01,
    UserAbort = 0x02,
    ReasonBadChunk = 0x03,
    Timeout = 0x04,
}

impl CancelReason {
    pub fn from_u8(val: u8) -> Option<Self> {
        match val {
            0x01 => Some(CancelReason::SelectionChanged),
            0x02 => Some(CancelReason::UserAbort),
            0x03 => Some(CancelReason::ReasonBadChunk),
            0x04 => Some(CancelReason::Timeout),
            _ => None,
        }
    }
}

/// Resync request reason code (RFC Section 4.9).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[repr(u8)]
pub enum ResyncReason {
    SequenceGap = 0x01,
    PaletteRevisionMismatch = 0x02,
    StateHashMismatch = 0x03,
    ChunkCorrupted = 0x04,
    VisualEntitiesHashMismatch = 0x06,
}

impl ResyncReason {
    pub fn from_u8(val: u8) -> Option<Self> {
        match val {
            0x01 => Some(ResyncReason::SequenceGap),
            0x02 => Some(ResyncReason::PaletteRevisionMismatch),
            0x03 => Some(ResyncReason::StateHashMismatch),
            0x04 => Some(ResyncReason::ChunkCorrupted),
            0x06 => Some(ResyncReason::VisualEntitiesHashMismatch),
            _ => None,
        }
    }
}

/// Client capability flag: supports continuous visual entity stream (RFC v3.2).
pub const CAP_SUPPORTS_VISUAL_ENTITIES: u32 = 0x0000_0004;

/// Delta Visual Entity action: Upsert entity.
pub const DELTA_VISUAL_ACTION_UPSERT: u8 = 0x01;
/// Delta Visual Entity action: Remove entity by UUID.
pub const DELTA_VISUAL_ACTION_REMOVE: u8 = 0x02;

/// Protocol Error code (RFC Section 4.14).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[repr(u16)]
pub enum ErrorCode {
    UnsupportedVersion = 0x0001,
    InvalidBounds = 0x0002,
    SequenceGapUnrecoverable = 0x0003,
    ChecksumMismatch = 0x0004,
    ResourceLimitExceeded = 0x0005,
    PermissionDenied = 0x0006,
    UnknownOpcode = 0x0007,
    InvalidPalette = 0x0008,
}

impl ErrorCode {
    pub fn from_u16(val: u16) -> Option<Self> {
        match val {
            0x0001 => Some(ErrorCode::UnsupportedVersion),
            0x0002 => Some(ErrorCode::InvalidBounds),
            0x0003 => Some(ErrorCode::SequenceGapUnrecoverable),
            0x0004 => Some(ErrorCode::ChecksumMismatch),
            0x0005 => Some(ErrorCode::ResourceLimitExceeded),
            0x0006 => Some(ErrorCode::PermissionDenied),
            0x0007 => Some(ErrorCode::UnknownOpcode),
            0x0008 => Some(ErrorCode::InvalidPalette),
            _ => None,
        }
    }
}
