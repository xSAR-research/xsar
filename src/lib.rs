/*!
# xSAR Architecture Core
Autonomous drone swarm mesh network and telemetry system.
*/

use std::fmt;

// ================================================================
// Node - flight controller communications
// ================================================================

/// Core operational status of an xSAR node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NodeState {
    EmergencyEvent = -1,
    Initialising = 0,
    Standby = 1,
    Active = 2,
}

impl fmt::Display for NodeState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmergencyEvent => write!(f, "EmergencyEvent"),
            Self::Initialising => write!(f, "Initialising"),
            Self::Standby => write!(f, "Standby"),
            Self::Active => write!(f, "Active"),
        }
    }
}

// ================================================================
// Mesh - flight controller communications
// ================================================================

/// Granular flight controller modes and states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FlightModeStatus {
    EmergencyEvent = -1,
    PreFlightCheck = 0,
    ReadyToLaunch = 1,
    InFlight = 2,
    Hovering = 3,
    Investigating = 4,
    SearchingPattern = 5,
    AssignedMasterDrone = 6,
    ReturningToBase = 7,
    Land = 8,
}

impl fmt::Display for FlightModeStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmergencyEvent => write!(f, "EmergencyEvent"),
            Self::PreFlightCheck => write!(f, "PreFlightCheck"),
            Self::ReadyToLaunch => write!(f, "ReadyToLaunch"),
            Self::InFlight => write!(f, "InFlight"),
            Self::Hovering => write!(f, "Hovering"),
            Self::Investigating => write!(f, "Investigating"),
            Self::SearchingPattern => write!(f, "SearchingPattern"),
            Self::AssignedMasterDrone => write!(f, "AssignedMasterDrone"),
            Self::ReturningToBase => write!(f, "ReturningToBase"),
            Self::Land => write!(f, "Land"),
        }
    }
}

// ================================================================
// Geometric Primitives for drone location and state reporting
// ================================================================

/// Represents a 3D Cartesian coordinate (X, Y, Z / Alt) for drone positioning.
pub type Position = (f64, f64, f64);

/// Represents the comprehensive internal flight controller telemetry state.
#[derive(Debug, Clone, PartialEq)]
pub struct FlightControllerState {
    pub mesh_assigned_identifier: u32,
    pub position: Position,
    pub battery_level: f32,
    pub flight_mode: FlightModeStatus,
    pub node_state: NodeState,
}

/// Represents compact telemetry data packets broadcast by a drone to peer mesh nodes.
#[derive(Debug, Clone, PartialEq)]
pub struct MeshTelemetry {
    pub node_assigned_identifier: u8,
    pub position: Position,
    pub battery_level: f32,
    pub node_state: NodeState,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_state_discriminants() {
        assert_eq!(NodeState::EmergencyEvent as i8, -1);
        assert_eq!(NodeState::Initialising as i8, 0);
        assert_eq!(NodeState::Standby as i8, 1);
        assert_eq!(NodeState::Active as i8, 2);
    }

    #[test]
    fn test_flight_mode_discriminants() {
        assert_eq!(FlightModeStatus::EmergencyEvent as i8, -1);
        assert_eq!(FlightModeStatus::PreFlightCheck as i8, 0);
        assert_eq!(FlightModeStatus::ReadyToLaunch as i8, 1);
        assert_eq!(FlightModeStatus::InFlight as i8, 2);
        assert_eq!(FlightModeStatus::Hovering as i8, 3);
        assert_eq!(FlightModeStatus::Investigating as i8, 4);
        assert_eq!(FlightModeStatus::SearchingPattern as i8, 5);
        assert_eq!(FlightModeStatus::AssignedMasterDrone as i8, 6);
        assert_eq!(FlightModeStatus::ReturningToBase as i8, 7);
        assert_eq!(FlightModeStatus::Land as i8, 8);
    }

    #[test]
    fn test_display_formatting() {
        assert_eq!(format!("{}", NodeState::Active), "Active");
        assert_eq!(format!("{}", NodeState::EmergencyEvent), "EmergencyEvent");
        assert_eq!(
            format!("{}", FlightModeStatus::SearchingPattern),
            "SearchingPattern"
        );
        assert_eq!(
            format!("{}", FlightModeStatus::ReturningToBase),
            "ReturningToBase"
        );
    }

    #[test]
    fn test_telemetry_packet_construction() {
        let fc_state = FlightControllerState {
            mesh_assigned_identifier: 1042,
            position: (-33.8688, 151.2093, 120.5),
            battery_level: 88.5,
            flight_mode: FlightModeStatus::InFlight,
            node_state: NodeState::Active,
        };

        assert_eq!(fc_state.mesh_assigned_identifier, 1042);
        assert_eq!(fc_state.position.2, 120.5);

        let mesh_packet = MeshTelemetry {
            node_assigned_identifier: 42,
            position: (-33.8688, 151.2093, 120.5),
            battery_level: 88.5,
            node_state: NodeState::Active,
        };

        assert_eq!(mesh_packet.node_assigned_identifier, 42);
    }
}
