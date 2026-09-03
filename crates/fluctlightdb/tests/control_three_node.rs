#![cfg(feature = "distributed")]

use std::collections::BTreeSet;

use fluctlightdb::control::state_machine::ControlStateMachine;
use fluctlightdb::control::types::{ControlCommand, NodeMetadata, TenantControlConfig};

#[test]
fn three_state_machines_converge_on_committed_control_metadata() {
    let pepper = [11; 32];
    let mut machines = [
        ControlStateMachine::new(&pepper).unwrap(),
        ControlStateMachine::new(&pepper).unwrap(),
        ControlStateMachine::new(&pepper).unwrap(),
    ];
    let mut commands = Vec::new();
    for node_id in 1..=3 {
        commands.push(ControlCommand::RegisterNode {
            request_id: format!("register-{node_id}"),
            node: NodeMetadata {
                node_id,
                raft_addr: format!("127.0.0.1:910{node_id}"),
                api_addr: format!("127.0.0.1:920{node_id}"),
                certificate_sha256: [node_id as u8; 32],
            },
        });
    }
    commands.extend([
        ControlCommand::CreateTenant {
            tenant_id: "tenant-a".into(),
            request_id: "create-tenant-a".into(),
            config: TenantControlConfig::default(),
        },
        ControlCommand::SetPlacement {
            tenant_id: "tenant-a".into(),
            request_id: "place-tenant-a".into(),
            nodes: BTreeSet::from([1, 2, 3]),
        },
        ControlCommand::SetVoters {
            request_id: "set-voters".into(),
            expected_membership_epoch: 0,
            voters: BTreeSet::from([1, 2, 3]),
        },
    ]);

    for command in commands {
        for machine in &mut machines {
            machine.apply(command.clone()).unwrap();
        }
    }

    assert_eq!(machines[0].state(), machines[1].state());
    assert_eq!(machines[1].state(), machines[2].state());
    assert_eq!(machines[0].state().voters, BTreeSet::from([1, 2, 3]));
}
