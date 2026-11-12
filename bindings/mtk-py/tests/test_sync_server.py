import time
import libmtk_py
import pytest


def test_live_sync_server_lifecycle():
    server = libmtk_py.LiveSyncServer(bind_address="127.0.0.1:0")
    assert server.is_running()
    addr = server.local_addr()
    assert addr is not None and "127.0.0.1:" in addr
    assert server.server_uuid != ""
    assert len(server.server_uuid) == 32

    server.set_selection(
        min_pos=(0, 64, 0),
        size=(16, 16, 16),
        dimension="minecraft:overworld",
    )

    events = server.poll_events()
    assert isinstance(events, list)

    server.clear_selection()

    server.stop()
    assert not server.is_running()


def test_live_sync_server_custom_uuid():
    custom_uuid = "12345678-1234-5678-1234-567812345678"
    server = libmtk_py.LiveSyncServer(
        bind_address="127.0.0.1:0",
        server_uuid=custom_uuid,
    )
    assert server.is_running()
    assert server.server_uuid != ""
    server.stop()


def test_live_sync_server_submit_baseline_and_deltas():
    server = libmtk_py.LiveSyncServer(bind_address="127.0.0.1:0")
    assert server.is_running()

    # 1. Submit baseline (16x16x16)
    size = (16, 16, 16)
    total_blocks = 16 * 16 * 16
    raw_states = [0] * total_blocks
    raw_states[0] = 0  # stone
    raw_states[1] = 1  # dirt
    block_palette = ["minecraft:stone", "minecraft:dirt"]

    total_biomes = 4 * 4 * 4
    raw_biomes = [0] * total_biomes
    biome_palette = ["minecraft:plains"]

    state_hash = server.submit_baseline(
        client_id=0,
        min_pos=(0, 64, 0),
        size=size,
        dimension="minecraft:overworld",
        raw_states=raw_states,
        block_palette=block_palette,
        raw_biomes=raw_biomes,
        biome_palette=biome_palette,
    )
    assert state_hash != 0

    # 2. Submit deltas with dynamic palette addition
    rel_coords = [(0, 0, 0), (1, 0, 0), (2, 0, 0)]
    block_states = ["minecraft:stone", "minecraft:gold_block", "minecraft:diamond_block"]
    block_entities = [((2, 0, 0), '{"id":"minecraft:chest"}')]

    server.submit_deltas(
        rel_coords=rel_coords,
        block_states=block_states,
        block_entities=block_entities,
    )

    server.stop()
    assert not server.is_running()


def test_live_sync_server_e2e_with_session():
    # 1. Start native server on ephemeral port
    server = libmtk_py.LiveSyncServer(
        bind_address="127.0.0.1:0",
        auth_token="test_token",
        allow_loopback_without_token=True,
    )
    addr = server.local_addr()
    assert addr is not None
    port = addr.split(":")[-1]

    # 2. Connect client session
    session = libmtk_py.LiveSyncSession()
    session.start(
        url=f"ws://127.0.0.1:{port}",
        auto_reconnect=False,
        auth_token="test_token",
    )

    # 3. Wait for baseline_requested event on server
    client_id = None
    start_time = time.time()
    while time.time() - start_time < 3.0:
        events = server.poll_events()
        for ev in events:
            if ev.get("type") == "baseline_requested":
                client_id = ev.get("client_id")
                break
        if client_id is not None:
            break
        time.sleep(0.05)

    assert client_id is not None, "Server must emit baseline_requested event"

    # 4. Stream baseline to requesting client
    size = (4, 4, 4)
    total_blocks = 4 * 4 * 4
    raw_states = [1] * total_blocks
    block_palette = ["minecraft:air", "minecraft:stone"]
    raw_biomes = [0] * 64
    biome_palette = ["minecraft:plains"]

    server.submit_baseline(
        client_id=client_id,
        min_pos=(0, 64, 0),
        size=size,
        dimension="minecraft:overworld",
        raw_states=raw_states,
        block_palette=block_palette,
        raw_biomes=raw_biomes,
        biome_palette=biome_palette,
    )

    # 5. Wait for client to process baseline and finish stream
    mesh_ready = False
    start_time = time.time()
    while time.time() - start_time < 3.0:
        events = session.poll_events()
        for ev in events:
            if ev.get("type") == "WORLD_MESH_READY":
                mesh_ready = True
                break
        if mesh_ready:
            break
        time.sleep(0.05)

    assert mesh_ready, "Client failed to receive baseline and build world mesh"

    session.stop()
    server.stop()
