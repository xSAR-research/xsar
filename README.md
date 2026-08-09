# xSAR Core Architecture

**Autonomous drone swarm mesh architecture and telemetry orchestration library.**

Built in **Rust**, `xsar` provides the foundational data structures, state machines, and networking primitives for coordinating autonomous drone swarms engaged in search and rescue (SAR) operations, avionics telemetry synchronization, and terrain-referenced navigation (TERCOM).

| | |
|---|---|
| **License** | [AGPL-3.0-or-later](LICENSE) |
| **Repo** | https://github.com/xsar-research/xsar |
| **Contact** | charlie@xsar.com.au / info@xsar.com.au |
| **Target** | Embedded avionics platforms & standard Linux runtimes |

---

## Key Modules & Types

- **`NodeState`**: High-level operational state machine of an xSAR swarm node (`EmergencyEvent`, `Initialising`, `Standby`, `Active`).
- **`FlightModeStatus`**: Granular flight controller states (`PreFlightCheck`, `ReadyToLaunch`, `InFlight`, `Hovering`, `Investigating`, `SearchingPattern`, `AssignedMasterDrone`, `ReturningToBase`, `Land`, `EmergencyEvent`).
- **`FlightControllerState`**: Full telemetry packet encompassing node identifier, 3D Cartesian coordinates (`Position`), battery level, flight mode, and node state.
- **`MeshTelemetry`**: Compact broadcast telemetry structure for high-frequency peer-to-peer mesh gossip.

---

## Build & Test

Ensure you have a modern Rust toolchain installed:

```bash
# Check code compilation
cargo check

# Run test suite
cargo test

# Build optimized release artifact
cargo build --release
```

---

## License

This project is licensed under the [GNU Affero General Public License v3 (AGPL-3.0)](LICENSE).
