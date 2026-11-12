import struct
import libmtk_py
import pytest

def build_test_hello_packet(
    server_uuid=b"\x00" * 16,
    epoch=0,
    seq=0,
    min_pos=(0, 0, 0),
    size=(0, 0, 0),
    state_hash=0,
    token="secret",
    dimension="minecraft:overworld",
):
    header = b"MT\x03\x01"  # Magic 'MT', version 3, opcode 0x01 (HELLO)
    tok_bytes = token.encode("utf-8")
    dim_bytes = dimension.encode("utf-8")
    payload = struct.pack(
        "<HHII16sQQiiiIIIQH",
        3, 3, 0, 0,
        server_uuid,
        epoch, seq,
        min_pos[0], min_pos[1], min_pos[2],
        size[0], size[1], size[2],
        state_hash,
        len(tok_bytes),
    ) + tok_bytes + struct.pack("<H", len(dim_bytes)) + dim_bytes
    return header + payload


def test_encode_baseline_from_storage_python():
    storage = libmtk_py.VoxelStorage()
    storage.set_block(0, 0, 0, "minecraft:stone")
    storage.set_block(1, 0, 0, "minecraft:dirt")
    storage.set_block(2, 2, 2, "minecraft:diamond_block")

    packets, state_hash, total_chunks = libmtk_py.encode_baseline_from_storage(
        storage,
        min_pos=(0, 0, 0),
        size=(4, 4, 4),
        dimension="minecraft:overworld",
        epoch=1,
        snapshot_uuid=b"\x01" * 16,
        snapshot_seq=10,
        max_chunk_size=1024,
    )

    assert total_chunks >= 1
    assert state_hash != 0
    assert len(packets) >= total_chunks + 2  # Begin + Chunks + End

    # Verify each packet starts with protocol magic 'MT'
    for pkt in packets:
        assert isinstance(pkt, bytes)
        assert pkt[:2] == b"MT"
        assert pkt[2] == 0x03  # Protocol v3

    # Decode first packet (BaselineBegin) using unstable.testing decoder
    begin_pkt = libmtk_py.unstable.testing.decode_packet(packets[0])
    assert begin_pkt["type"] == "BASELINE_BEGIN"
    assert begin_pkt["state_hash"] == f"0x{state_hash:016x}"
    assert begin_pkt["total_chunks"] == total_chunks
    assert "minecraft:stone" in begin_pkt["block_palette"]


def test_decide_handshake_python():
    server_uuid = b"\x42" * 16
    min_pos = (10, 20, 30)
    size = (16, 16, 16)
    state_hash = 123456789012345678

    # 1. Match cached hello
    cached_hello = build_test_hello_packet(
        server_uuid=server_uuid,
        epoch=1,
        seq=5,
        min_pos=min_pos,
        size=size,
        state_hash=state_hash,
        token="secret",
        dimension="minecraft:overworld",
    )

    welcome_bytes, decision = libmtk_py.decide_handshake(
        cached_hello,
        server_uuid=server_uuid,
        epoch=1,
        sequence=5,
        min_pos=min_pos,
        size=size,
        state_hash=state_hash,
        dimension="minecraft:overworld",
        expected_auth_token="secret",
    )
    assert decision == 0  # VerifiedSkipRebuild
    welcome_pkt = libmtk_py.unstable.testing.decode_packet(welcome_bytes)
    assert welcome_pkt["type"] == "WELCOME"
    assert welcome_pkt["decision"] == "VERIFIED_SKIP_REBUILD"

    # 2. Token mismatch
    welcome_bytes_rejected, decision_rejected = libmtk_py.decide_handshake(
        cached_hello,
        server_uuid=server_uuid,
        epoch=1,
        sequence=5,
        min_pos=min_pos,
        size=size,
        state_hash=state_hash,
        dimension="minecraft:overworld",
        expected_auth_token="different_secret",
    )
    assert decision_rejected == 0xFF  # Rejected
    welcome_pkt_rej = libmtk_py.unstable.testing.decode_packet(welcome_bytes_rejected)
    assert welcome_pkt_rej["decision"] == "REJECTED"


def test_encode_delta_op_batch_python():
    changes = [
        ((0, 1, 2), 10),
        ((3, 4, 5), 20),
    ]

    delta_packets = libmtk_py.encode_delta_op_batch(
        epoch=1,
        seq_id=42,
        palette_revision=1,
        changes=changes,
    )
    assert len(delta_packets) == 1
    decoded = libmtk_py.unstable.testing.decode_packet(delta_packets[0])
    assert decoded["type"] == "DELTA_OP"
    assert decoded["count"] == 2
    assert decoded["seq_id"] == 42
    assert len(decoded["changes"]) == 2
    assert decoded["changes"][0]["rel_x"] == 0
    assert decoded["changes"][0]["rel_y"] == 1
    assert decoded["changes"][0]["rel_z"] == 2
    assert decoded["changes"][0]["state_id"] == 10
